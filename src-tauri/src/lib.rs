mod ajazz;
mod devices;
mod mouse_buttons;
mod watcher;

use devices::Peripheral;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

const MINIMIZED_ARG: &str = "--minimized";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    notify_connection: bool,
    notify_battery: bool,
}
impl Default for Settings {
    fn default() -> Self { Self { notify_connection: true, notify_battery: true } }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceEvent {
    kind: &'static str,
    device: Peripheral,
    /// Milissegundos desde a época Unix.
    occurred_at: u64,
}

struct Monitor {
    known: Mutex<HashMap<String, Peripheral>>,
    settings: Mutex<Settings>,
    settings_path: Option<PathBuf>,
    tray_hint_shown: AtomicBool,
    /// Última leitura da bateria do mouse AJAZZ.
    battery: Mutex<Option<ajazz::Battery>>,
    /// Menor limite de bateria baixa já notificado (evita repetir o alerta).
    battery_alerted: Mutex<Option<u8>>,
    /// Acorda a thread de bateria para ler agora.
    battery_wake: Sender<()>,
}

/// Alertas de bateria baixa, do mais alto ao mais baixo.
const LOW_BATTERY_THRESHOLDS: [u8; 2] = [20, 10];
const BATTERY_POLL: Duration = Duration::from_secs(120);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn sorted(monitor: &Monitor, map: &HashMap<String, Peripheral>) -> Vec<Peripheral> {
    let battery = monitor.battery.lock().ok().and_then(|b| *b);
    let mut list: Vec<_> = map.values().cloned().collect();
    for device in list.iter_mut().filter(|d| ajazz::is_ajazz(d.vendor_id, d.product_id)) {
        device.battery_percent = battery.and_then(|b| b.percent);
        device.battery_charging = battery.is_some_and(|b| b.charging);
    }
    list.sort_by_key(|d| d.name.to_lowercase());
    list
}

/// Mantém a interface vendor do mouse aberta: recebe na hora os avisos que ele
/// manda ao entrar/sair do suporte ou do cabo, e consulta a bateria a cada
/// `BATTERY_POLL` (ou quando acordado por `wake`).
fn start_battery_monitor(app: AppHandle, wake: Receiver<()>) {
    thread::spawn(move || loop {
        let Some(link) = ajazz::Link::open() else {
            update_battery(&app, None);
            match wake.recv_timeout(BATTERY_POLL) {
                // O mouse leva um instante para parear com o receptor depois de ligar.
                Ok(()) => thread::sleep(Duration::from_millis(1500)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            continue;
        };
        let mut tracker = ajazz::Tracker::default();
        let mut next_query = Instant::now();
        loop {
            let woken = wake.try_recv().is_ok();
            if woken || Instant::now() >= next_query {
                while wake.try_recv().is_ok() {}
                if !link.request_info() { break; }
                next_query = Instant::now() + BATTERY_POLL;
            }
            match link.next(Duration::from_millis(500)) {
                Ok(Some(status)) => {
                    tracker.apply(status);
                    update_battery(&app, Some(tracker.battery()));
                }
                Ok(None) => {}
                // Receptor removido: volta a esperar o mouse aparecer.
                Err(()) => break,
            }
        }
    });
}

fn update_battery(app: &AppHandle, reading: Option<ajazz::Battery>) {
    let monitor = app.state::<Monitor>();
    // O cabo também carrega, mesmo que o receptor não avise.
    let wired = monitor.known.lock().map(|k| k.values().any(|d| ajazz::is_wired(d.vendor_id, d.product_id))).unwrap_or(false);
    let reading = reading.map(|b| ajazz::Battery { charging: b.charging || wired, ..b });
    let changed = {
        let Ok(mut battery) = monitor.battery.lock() else { return };
        let changed = *battery != reading;
        *battery = reading;
        changed
    };
    if changed { let _ = app.emit("peripherals-changed", ()); }

    let Some(battery) = reading else { return };
    let Some(percent) = battery.percent else { return };
    let Ok(mut alerted) = monitor.battery_alerted.lock() else { return };
    if battery.charging || percent > LOW_BATTERY_THRESHOLDS[0] + 5 {
        *alerted = None;
        return;
    }
    let Some(&threshold) = LOW_BATTERY_THRESHOLDS.iter().rev().find(|&&t| percent <= t) else { return };
    if alerted.is_some_and(|a| a <= threshold) { return; }
    *alerted = Some(threshold);
    if monitor.settings.lock().map(|s| s.notify_battery).unwrap_or(true) {
        let _ = app.notification().builder()
            .title("Bateria baixa no mouse AJAZZ")
            .body(format!("O AJ179P MC está com {percent}% de bateria. Coloque-o no suporte para carregar."))
            .show();
    }
}

/// Refaz o inventário, compara com o anterior e emite/notifica as diferenças.
fn rescan(app: &AppHandle) -> Result<Vec<Peripheral>, String> {
    let monitor = app.state::<Monitor>();
    let current = devices::snapshot()?;
    let mut events = Vec::new();
    let list = {
        let mut known = monitor.known.lock().map_err(|_| "Inventário de dispositivos indisponível")?;
        for (id, device) in &current {
            if !known.contains_key(id) { events.push(("connected", device.clone())); }
        }
        for (id, device) in known.iter() {
            if !current.contains_key(id) { events.push(("disconnected", device.clone())); }
        }
        *known = current;
        sorted(&monitor, &known)
    };

    // Mouse ligado/desligado ou cabo conectado: lê a bateria de novo.
    if events.iter().any(|(_, d)| ajazz::is_ajazz(d.vendor_id, d.product_id)) {
        let _ = monitor.battery_wake.send(());
    }
    let notify = monitor.settings.lock().map(|s| s.notify_connection).unwrap_or(true);
    for (kind, device) in events {
        if notify { notify_device(app, kind, &device); }
        let _ = app.emit("peripheral-event", DeviceEvent { kind, device, occurred_at: now_ms() });
    }
    Ok(list)
}

fn notify_device(app: &AppHandle, kind: &str, device: &Peripheral) {
    let title = if kind == "connected" { "Dispositivo conectado" } else { "Dispositivo desconectado" };
    let mut body = device.name.clone();
    if let Some(m) = &device.manufacturer { body.push_str(&format!("\n{m}")); }
    body.push_str(&format!("\n{} · VID {:04X} / PID {:04X}", device.transport, device.vendor_id, device.product_id));
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("Falha ao exibir notificação: {e}");
    }
}

fn start_monitor(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    let event_driven = watcher::register(tx);
    // Com eventos do Windows, a varredura periódica é só uma rede de segurança.
    let wait = if event_driven { Duration::from_secs(30) } else { Duration::from_secs(2) };
    thread::spawn(move || loop {
        match rx.recv_timeout(wait) {
            Ok(()) => {
                // Um único plugue gera vários eventos (uma interface por função);
                // espera a rajada terminar e faz uma varredura só.
                thread::sleep(Duration::from_millis(800));
                while rx.try_recv().is_ok() {}
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(wait),
        }
        if let Err(e) = rescan(&app) { eprintln!("{e}"); }
    });
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn load_settings(path: Option<&PathBuf>) -> Settings {
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn list_devices(monitor: State<Monitor>) -> Result<Vec<Peripheral>, String> {
    let known = monitor.known.lock().map_err(|_| "Inventário de dispositivos indisponível")?;
    Ok(sorted(&monitor, &known))
}

#[tauri::command]
fn refresh_devices(app: AppHandle) -> Result<Vec<Peripheral>, String> {
    let _ = app.state::<Monitor>().battery_wake.send(());
    rescan(&app)
}

#[tauri::command]
fn get_settings(monitor: State<Monitor>) -> Settings {
    monitor.settings.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
fn set_settings(monitor: State<Monitor>, settings: Settings) -> Result<(), String> {
    if let Some(path) = &monitor.settings_path {
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
        let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("Falha ao salvar preferências: {e}"))?;
    }
    *monitor.settings.lock().map_err(|_| "Preferências indisponíveis")? = settings;
    Ok(())
}

/// Roda a conversa com o mouse fora da thread principal (leva até ~2 s).
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_mouse_buttons() -> Result<Vec<mouse_buttons::MouseButton>, String> {
    blocking(mouse_buttons::read_buttons).await
}

#[tauri::command]
async fn set_mouse_button(
    slot: usize,
    action: mouse_buttons::ButtonAction,
    macro_steps: Option<Vec<mouse_buttons::MacroStep>>,
) -> Result<Vec<mouse_buttons::MouseButton>, String> {
    blocking(move || mouse_buttons::write_button(slot, &action, macro_steps.as_deref())).await
}

#[tauri::command]
async fn reset_mouse_buttons() -> Result<Vec<mouse_buttons::MouseButton>, String> {
    blocking(mouse_buttons::reset_buttons).await
}

pub fn run() {
    tauri::Builder::default()
        // Precisa ser o primeiro plugin: uma segunda execução só reabre a janela.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .invoke_handler(tauri::generate_handler![
            list_devices, refresh_devices, get_settings, set_settings,
            get_mouse_buttons, set_mouse_button, reset_mouse_buttons,
        ])
        .on_window_event(|window, event| {
            // Fechar a janela só a esconde; o monitor continua na bandeja.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                let app = window.app_handle();
                if !app.state::<Monitor>().tray_hint_shown.swap(true, Ordering::Relaxed) {
                    let _ = app.notification().builder()
                        .title("Peripheral Monitor continua ativo")
                        .body("O monitoramento segue na bandeja do sistema. Use \"Sair\" no menu do ícone para encerrar.")
                        .show();
                }
            }
        })
        .setup(|app| {
            let settings_path = app.path().app_config_dir().ok().map(|d| d.join("settings.json"));
            let initial = devices::snapshot().unwrap_or_else(|e| { eprintln!("{e}"); HashMap::new() });
            let (battery_wake, battery_rx) = mpsc::channel();
            app.manage(Monitor {
                known: Mutex::new(initial),
                settings: Mutex::new(load_settings(settings_path.as_ref())),
                settings_path,
                tray_hint_shown: AtomicBool::new(false),
                battery: Mutex::new(None),
                battery_alerted: Mutex::new(None),
                battery_wake,
            });

            let open = MenuItem::with_id(app, "open", "Abrir Peripheral Monitor", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Peripheral Monitor")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() { tray = tray.icon(icon.clone()); }
            tray.build(app)?;

            // Ao iniciar com o Windows, abre direto na bandeja.
            if !std::env::args().any(|a| a == MINIMIZED_ARG) { show_main(app.handle()); }

            start_monitor(app.handle().clone());
            start_battery_monitor(app.handle().clone(), battery_rx);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao executar Peripheral Monitor");
}
