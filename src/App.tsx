import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { enable, isEnabled, disable } from "@tauri-apps/plugin-autostart";
import { Activity, BatteryCharging, Bell, Bluetooth, Cable, Camera, Gamepad2, HardDrive, Headphones, Keyboard, Mouse, Printer, RefreshCw, Settings2, Usb, X, Zap, type LucideIcon } from "lucide-react";
import type { DeviceEvent, Peripheral, Settings } from "./types";
import { MouseButtonsPanel } from "./MouseButtons";

const hex = (n: number) => n.toString(16).padStart(4, "0").toUpperCase();
const displayName = (d: Peripheral) => d.name?.trim() || d.manufacturer || `Dispositivo ${d.transport} ${hex(d.vendorId)}:${hex(d.productId)}`;

function deviceIcon(d: Peripheral): LucideIcon {
  const text = `${d.name} ${d.deviceClass ?? ""}`.toLowerCase();
  if (/keyboard|teclado/.test(text)) return Keyboard;
  if (/mouse/.test(text)) return Mouse;
  if (/headset|headphone|fone|audio|áudio|speaker/.test(text)) return Headphones;
  if (/camera|câmera|webcam|\bimage\b/.test(text)) return Camera;
  if (/controller|gamepad|xbox|joystick|controle/.test(text)) return Gamepad2;
  if (/printer|impressora/.test(text)) return Printer;
  if (/storage|flash|disk|drive|armazenamento|sandisk|kingston/.test(text)) return HardDrive;
  return d.transport === "Bluetooth" ? Bluetooth : Usb;
}

/** Mesmas faixas do driver oficial: 4 barras, a partir de 75/50/25 %. */
function batteryLabel(d: Peripheral | undefined): string {
  if (!d) return "mouse não identificado";
  // Carregando, o mouse informa 100% fixo; o nível real só volta ao sair do carregador.
  if (d.batteryCharging) return d.batteryPercent == null ? "nível aparece ao tirar do suporte" : `estava em ${d.batteryPercent}% ao começar a carregar`;
  if (d.batteryPercent == null) return "aguardando resposta do mouse";
  const bars = d.batteryPercent >= 75 ? 4 : d.batteryPercent >= 50 ? 3 : d.batteryPercent >= 25 ? 2 : 1;
  return `${"▮".repeat(bars)}${"▯".repeat(4 - bars)} ${bars === 1 ? "bateria baixa" : "no receptor 2.4G"}`;
}

/** Bateria vertical: o líquido sobe até o nível; carregando, a superfície ondula e sobem bolhas. */
function BatteryGauge({ percent, charging }: { percent: number | null; charging: boolean }) {
  // Sem leitura real (abriu já carregando), mostra o líquido pela metade.
  const level = percent == null ? 50 : Math.max(6, Math.min(100, percent));
  const tone = charging ? "charging" : percent != null && percent < 25 ? "low" : "";
  return <div className={`battery-gauge ${tone}`} role="img" aria-label={`Bateria ${percent ?? "?"}%${charging ? ", carregando" : ""}`}>
    <div className="battery-cap"/>
    <div className="battery-body">
      <div className="battery-liquid" style={{ height: `${level}%` }}>
        {charging && <>
          <svg className="battery-wave back" viewBox="0 0 120 12" preserveAspectRatio="none" aria-hidden><path d="M0 6 Q15 0 30 6 T60 6 T90 6 T120 6 V12 H0 Z"/></svg>
          <svg className="battery-wave front" viewBox="0 0 120 12" preserveAspectRatio="none" aria-hidden><path d="M0 6 Q15 12 30 6 T60 6 T90 6 T120 6 V12 H0 Z"/></svg>
          <span className="bubble b1"/><span className="bubble b2"/><span className="bubble b3"/>
        </>}
      </div>
      {charging && <Zap className="battery-bolt" size={16} strokeWidth={2.4}/>}
    </div>
  </div>;
}

export default function App() {
  const [devices, setDevices] = useState<Peripheral[]>([]);
  const [history, setHistory] = useState<DeviceEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [autostart, setAutostart] = useState(false);
  const [settings, setSettings] = useState<Settings>({ notifyConnection: true, notifyBattery: true });
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true); setError("");
    try { setDevices(await invoke<Peripheral[]>("refresh_devices")); }
    catch (e) { setError(String(e)); }
    finally { setLoading(false); }
  }, []);
  // Com o cabo conectado o mouse aparece duas vezes (receptor + USB); prefere quem tem leitura.
  const ajazz = useMemo(() => {
    const matches = devices.filter(d => /ajazz|aj179/i.test(`${d.name} ${d.manufacturer}`));
    return matches.find(d => d.batteryPercent != null) ?? matches[0];
  }, [devices]);

  useEffect(() => {
    void invoke<Peripheral[]>("list_devices").then(setDevices).catch(e => setError(String(e)));
    void invoke<Settings>("get_settings").then(setSettings).catch(() => undefined);
    void isEnabled().then(setAutostart).catch(() => undefined);
    // A notificação do Windows é enviada pelo Rust; aqui só atualizamos a tela.
    let unlisten: (() => void) | undefined;
    let active = true;
    void listen<DeviceEvent>("peripheral-event", ({ payload }) => {
      if (!active) return;
      setHistory(old => [payload, ...old].slice(0, 50));
      void invoke<Peripheral[]>("list_devices").then(setDevices).catch(() => undefined);
    }).then(fn => { if (active) unlisten = fn; else fn(); });
    // Nova leitura de bateria.
    let unlistenBattery: (() => void) | undefined;
    void listen("peripherals-changed", () => {
      if (active) void invoke<Peripheral[]>("list_devices").then(setDevices).catch(() => undefined);
    }).then(fn => { if (active) unlistenBattery = fn; else fn(); });
    return () => { active = false; unlisten?.(); unlistenBattery?.(); };
  }, []);

  async function updateSettings(patch: Partial<Settings>) {
    const previous = settings;
    const next = { ...settings, ...patch };
    setSettings(next);
    try { await invoke("set_settings", { settings: next }); }
    catch (e) { setSettings(previous); setError(String(e)); }
  }

  async function toggleAutostart(next: boolean) {
    try { if (next) await enable(); else await disable(); setAutostart(next); }
    catch (e) { setError(`Não foi possível alterar a inicialização automática: ${String(e)}`); }
  }

  return <main className="shell">
    <header className="topbar"><div className="brand-mark"><Activity size={23}/></div><div className="brand-copy"><span className="eyebrow">WINDOWS HARDWARE</span><h1>Peripheral Monitor</h1></div><span className="live-pill"><span/> MONITORAMENTO USB</span></header>
    {error && <div className="error-banner"><span>{error}</span><button onClick={() => setError("")} aria-label="Fechar"><X size={16}/></button></div>}
    <section className="hero"><div><p className="eyebrow">PAINEL DE DISPOSITIVOS</p><h2>Seu hardware.<br/><em>Sob controle.</em></h2><p className="hero-description">Acompanhe periféricos USB e Bluetooth conectados e receba alertas quando o hardware mudar.</p></div><div className="hero-art"><div className="orbit orbit-one"/><div className="orbit orbit-two"/><Usb size={78} strokeWidth={1.1}/></div></section>
    <section className="stats"><article className="stat-card"><span className="stat-label">DISPOSITIVOS</span><strong>{String(devices.length).padStart(2,"0")}</strong><span className="stat-foot"><Cable size={14}/> detectados agora</span></article><article className="stat-card"><span className="stat-label">EVENTOS RECENTES</span><strong>{String(history.length).padStart(2,"0")}</strong><span className="stat-foot"><Activity size={14}/> nesta sessão</span></article><article className="stat-card battery-card">{ajazz && (ajazz.batteryPercent != null || ajazz.batteryCharging) && <BatteryGauge percent={ajazz.batteryPercent} charging={ajazz.batteryCharging}/>}<span className="stat-label">BATERIA AJAZZ</span>{ajazz?.batteryCharging
        ? <strong className="charging-text">Carregando</strong>
        : <strong>{ajazz?.batteryPercent == null ? "—" : `${ajazz.batteryPercent}%`}</strong>}<span className="stat-foot"><BatteryCharging size={14}/> {batteryLabel(ajazz)}</span></article></section>
    <section className="content-grid"><article className="panel"><div className="panel-heading"><div><p className="eyebrow">HARDWARE ATIVO</p><h3>Dispositivos conectados</h3></div><button className="icon-button" onClick={() => void refresh()} title="Atualizar lista"><RefreshCw size={17} className={loading ? "spin" : ""}/></button></div>
      {devices.length === 0 ? <div className="empty-state"><div className="empty-icon"><Usb size={25}/></div><strong>Nenhum dispositivo encontrado</strong><p>Conecte um periférico e atualize a lista.</p></div> : <div className="device-list">{devices.map(d => { const Icon = deviceIcon(d); return <div className="device-row" key={d.id}><div className="device-icon"><Icon size={20}/></div><div className="device-info"><strong>{displayName(d)}</strong><span>{d.manufacturer || "Fabricante não informado"} · {d.transport} · VID {hex(d.vendorId)} / PID {hex(d.productId)}</span><small>{d.serialNumber ? `S/N ${d.serialNumber}` : "Número de série indisponível"}</small></div>{(d.batteryPercent != null || d.batteryCharging) && <span className={`battery-tag ${d.batteryPercent != null && d.batteryPercent < 25 && !d.batteryCharging ? "low" : ""}`}>{d.batteryCharging ? "⚡ carregando" : `${d.batteryPercent}%`}</span>}<span className="connected-tag">CONECTADO</span></div>; })}</div>}
      <p className="panel-note">Cada periférico aparece uma vez, mesmo que exponha várias interfaces. Fechar a janela mantém o monitor ativo na bandeja do sistema.</p></article>
      <article className="panel"><div className="panel-heading"><div><p className="eyebrow">PREFERÊNCIAS</p><h3>Comportamento</h3></div><Settings2 size={20} className="muted"/></div>
        <Setting title="Notificações de conexão" description="Alertar quando um dispositivo entrar ou sair." checked={settings.notifyConnection} onChange={v => void updateSettings({ notifyConnection: v })}/>
        <Setting title="Alertas de bateria" description="Preparado para alertas após validar o protocolo do AJAZZ." checked={settings.notifyBattery} onChange={v => void updateSettings({ notifyBattery: v })}/>
        <Setting title="Iniciar com o Windows" description="Abrir minimizado na bandeja ao entrar no Windows." checked={autostart} onChange={v => void toggleAutostart(v)}/>
        <div className="settings-foot"><Bell size={15}/><span>As notificações são enviadas mesmo com a janela fechada.</span></div></article></section>
    {ajazz && <MouseButtonsPanel key={ajazz.id}/>}
    <section className="panel activity-panel"><div className="panel-heading"><div><p className="eyebrow">REGISTRO LOCAL</p><h3>Atividade recente</h3></div><span className="count-label">{history.length} eventos</span></div>{history.length === 0 ? <p className="activity-empty">Os eventos de conexão e desconexão aparecerão aqui.</p> : <div className="event-list">{history.map((e,i) => <div className="event-row" key={`${e.occurredAt}-${e.device.id}-${i}`}><span className={`event-dot ${e.kind}`}/><div><strong>{displayName(e.device)}</strong><p>{e.kind === "connected" ? "Conectado" : "Desconectado"} · {new Date(e.occurredAt).toLocaleString("pt-BR")}</p></div><span className="event-type">{e.kind === "connected" ? "CONNECTED" : "REMOVED"}</span></div>)}</div>}</section>
    <footer><span>PERIPHERAL MONITOR <b>0.1.0</b></span><span>Detecção USB · Bluetooth · Windows</span></footer>
  </main>;
}
function Setting({title,description,checked,onChange}:{title:string;description:string;checked:boolean;onChange:(v:boolean)=>void}) { return <div className="setting-row"><div><strong>{title}</strong><p>{description}</p></div><button className={`switch ${checked?"checked":""}`} role="switch" aria-checked={checked} onClick={() => onChange(!checked)}><span/></button></div>; }
