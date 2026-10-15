//! Avisa quando o conjunto de dispositivos pode ter mudado.
//!
//! No Windows usa `CM_Register_Notification` para todas as classes de
//! interface: qualquer chegada/remoção acorda o monitor, que então refaz o
//! inventário e calcula a diferença. Se o registro falhar, cai para polling.

use std::sync::mpsc::Sender;

/// Registra o callback do Windows. Retorna `false` se não for possível, para
/// que o chamador use polling.
#[cfg(windows)]
pub fn register(tx: Sender<()>) -> bool {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;

    unsafe extern "system" fn callback(
        _notify: HCMNOTIFICATION,
        context: *const core::ffi::c_void,
        _action: CM_NOTIFY_ACTION,
        _data: *const CM_NOTIFY_EVENT_DATA,
        _size: u32,
    ) -> u32 {
        // SAFETY: `context` é o Box<Sender<()>> vazado em `register`, válido
        // durante toda a vida do processo.
        let tx = unsafe { &*(context as *const Sender<()>) };
        let _ = tx.send(());
        0 // ERROR_SUCCESS
    }

    let filter = CM_NOTIFY_FILTER {
        cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
        Flags: CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES,
        FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
        ..Default::default()
    };

    // O contexto e o handle ficam vivos até o processo terminar.
    let context = Box::into_raw(Box::new(tx));
    let mut handle: HCMNOTIFICATION = std::ptr::null_mut();
    let result = unsafe { CM_Register_Notification(&filter, context as *const _, Some(callback), &mut handle) };
    if result != CR_SUCCESS {
        // SAFETY: o registro falhou, então o Windows não guardou o ponteiro.
        drop(unsafe { Box::from_raw(context) });
        eprintln!("CM_Register_Notification falhou (CONFIGRET {result}); usando polling");
        return false;
    }
    true
}

#[cfg(not(windows))]
pub fn register(_tx: Sender<()>) -> bool { false }
