//! Inventário de periféricos.
//!
//! - USB: enumerado pela SetupAPI do Windows (enumerador "USB"). Só o nó do
//!   dispositivo físico entra na lista; as interfaces de dispositivos compostos
//!   (`&MI_xx`) e os root hubs são ignorados, então um teclado com 3 interfaces
//!   HID aparece uma vez só.
//! - Bluetooth: dispositivos HID via `hidapi`, agrupados por VID/PID/serial.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peripheral {
    pub id: String,
    pub name: String,
    pub manufacturer: Option<String>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    /// Instance ID do Windows (USB) ou caminho HID (Bluetooth).
    pub path: String,
    /// Classe de dispositivo do Windows (ex.: "HIDClass", "USB", "Image").
    pub device_class: Option<String>,
    pub battery_percent: Option<u8>,
    pub battery_charging: bool,
    /// "USB" ou "Bluetooth".
    pub transport: String,
}

/// Dispositivos cujo receptor se identifica com um nome genérico do fabricante
/// do chip (VID, PID, nome, fabricante).
const KNOWN_DEVICES: &[(u16, u16, &str, &str)] = &[
    // O receptor 2.4 GHz do AJAZZ AJ179P MC se apresenta como "XCTECH Wireless-Receiver".
    (0x249A, 0x5C2F, "Mouse AJAZZ AJ179P MC", "AJAZZ"),
    (0x248A, 0x5C2F, "Mouse AJAZZ AJ179P MC", "AJAZZ"),
    (0x248A, 0x5C2E, "Mouse AJAZZ AJ179P MC (cabo)", "AJAZZ"),
];

fn apply_known_name(device: &mut Peripheral) {
    if let Some((_, _, name, manufacturer)) = KNOWN_DEVICES.iter().find(|(v, p, ..)| *v == device.vendor_id && *p == device.product_id) {
        device.name = (*name).into();
        device.manufacturer = Some((*manufacturer).into());
    }
}

/// Strings de produto/fabricante lidas via HID, usadas para dar nome melhor
/// a dispositivos USB cujo descritor do Windows é genérico.
struct HidStrings {
    product: Option<String>,
    manufacturer: Option<String>,
}

pub fn snapshot() -> Result<HashMap<String, Peripheral>, String> {
    let api = hidapi::HidApi::new().map_err(|e| format!("Falha ao inicializar HID: {e}"))?;
    let mut hid_by_ids: HashMap<(u16, u16), HidStrings> = HashMap::new();
    let mut devices: HashMap<String, Peripheral> = HashMap::new();

    for info in api.device_list() {
        let product = clean(info.product_string());
        let manufacturer = clean(info.manufacturer_string());
        let entry = hid_by_ids
            .entry((info.vendor_id(), info.product_id()))
            .or_insert(HidStrings { product: None, manufacturer: None });
        if entry.product.is_none() { entry.product = product.clone(); }
        if entry.manufacturer.is_none() { entry.manufacturer = manufacturer.clone(); }

        if matches!(info.bus_type(), hidapi::BusType::Bluetooth) {
            let serial = clean(info.serial_number());
            let id = format!("bt:{:04x}:{:04x}:{}", info.vendor_id(), info.product_id(), serial.as_deref().unwrap_or(""));
            devices.entry(id.clone()).or_insert_with(|| Peripheral {
                id,
                name: product.unwrap_or_else(|| format!("Dispositivo Bluetooth {:04X}:{:04X}", info.vendor_id(), info.product_id())),
                manufacturer,
                vendor_id: info.vendor_id(),
                product_id: info.product_id(),
                serial_number: serial,
                path: info.path().to_string_lossy().to_string(),
                device_class: Some("HIDClass".into()),
                battery_percent: None, battery_charging: false,
                transport: "Bluetooth".into(),
            });
        }
    }

    for mut device in usb_devices() {
        if let Some(hid) = hid_by_ids.get(&(device.vendor_id, device.product_id)) {
            if is_generic_name(&device.name) {
                if let Some(product) = &hid.product { device.name = product.clone(); }
            }
            if device.manufacturer.is_none() { device.manufacturer = hid.manufacturer.clone(); }
        }
        devices.insert(device.id.clone(), device);
    }
    devices.values_mut().for_each(apply_known_name);
    Ok(devices)
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// Descrições que o Windows usa quando o driver não informa um nome real.
fn is_generic_name(name: &str) -> bool {
    let n = name.to_lowercase();
    ["composite", "composto", "input device", "dispositivo de entrada", "usb device", "dispositivo usb", "hid-compliant", "compatível com hid"]
        .iter()
        .any(|g| n.contains(g))
}

/// Extrai os 4 dígitos hex depois de `key` (ex.: "VID_") num instance ID.
fn hex_after(instance_id: &str, key: &str) -> u16 {
    instance_id
        .to_uppercase()
        .find(key)
        .and_then(|i| instance_id.get(i + key.len()..i + key.len() + 4))
        .and_then(|h| u16::from_str_radix(h, 16).ok())
        .unwrap_or(0)
}

/// O último segmento do instance ID é o número de série quando o dispositivo
/// informa um; quando não, o Windows gera algo como "5&2a3b4c&0&1".
fn serial_from_instance_id(instance_id: &str) -> Option<String> {
    instance_id.rsplit('\\').next().filter(|s| !s.contains('&') && !s.is_empty()).map(str::to_owned)
}

#[cfg(windows)]
fn usb_devices() -> Vec<Peripheral> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows_sys::Win32::Devices::Properties::DEVPKEY_Device_BusReportedDeviceDesc;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    let enumerator = wide("USB");
    let mut out = Vec::new();
    unsafe {
        let set = SetupDiGetClassDevsW(std::ptr::null(), enumerator.as_ptr(), std::ptr::null_mut(), DIGCF_ALLCLASSES | DIGCF_PRESENT);
        if set == INVALID_HANDLE_VALUE as HDEVINFO { return out; }

        let mut index = 0;
        loop {
            let mut data: SP_DEVINFO_DATA = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
            if SetupDiEnumDeviceInfo(set, index, &mut data) == 0 { break; }
            index += 1;

            let Some(instance_id) = instance_id(set, &data) else { continue };
            let upper = instance_id.to_uppercase();
            // Interfaces de dispositivo composto e hubs raiz não são periféricos.
            if upper.contains("&MI_") || upper.contains("ROOT_HUB") { continue; }

            let class = registry_string(set, &data, SPDRP_CLASS);
            let name = device_property_string(set, &data, &DEVPKEY_Device_BusReportedDeviceDesc)
                .or_else(|| registry_string(set, &data, SPDRP_FRIENDLYNAME))
                .or_else(|| registry_string(set, &data, SPDRP_DEVICEDESC))
                .unwrap_or_else(|| "Dispositivo USB".into());
            if class.as_deref() == Some("USB") && name.to_lowercase().contains("hub") { continue; }
            // "(Dispositivos USB padrão)" e similares não identificam o fabricante.
            let manufacturer = registry_string(set, &data, SPDRP_MFG).filter(|m| !m.starts_with('('));

            out.push(Peripheral {
                id: format!("usb:{upper}"),
                vendor_id: hex_after(&instance_id, "VID_"),
                product_id: hex_after(&instance_id, "PID_"),
                serial_number: serial_from_instance_id(&instance_id),
                name,
                manufacturer,
                path: instance_id,
                device_class: class,
                battery_percent: None, battery_charging: false,
                transport: "USB".into(),
            });
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    out
}

#[cfg(not(windows))]
fn usb_devices() -> Vec<Peripheral> { Vec::new() }

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

#[cfg(windows)]
fn from_wide(buf: &[u16]) -> Option<String> {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..end]).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

#[cfg(windows)]
unsafe fn instance_id(
    set: windows_sys::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    data: &windows_sys::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
) -> Option<String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::SetupDiGetDeviceInstanceIdW;
    let mut buf = [0u16; 512];
    let ok = SetupDiGetDeviceInstanceIdW(set, data, buf.as_mut_ptr(), buf.len() as u32, std::ptr::null_mut());
    if ok == 0 { None } else { from_wide(&buf) }
}

#[cfg(windows)]
unsafe fn registry_string(
    set: windows_sys::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    data: &windows_sys::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
    property: u32,
) -> Option<String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::SetupDiGetDeviceRegistryPropertyW;
    let mut buf = [0u16; 512];
    let ok = SetupDiGetDeviceRegistryPropertyW(
        set, data, property, std::ptr::null_mut(),
        buf.as_mut_ptr() as *mut u8, (buf.len() * 2) as u32, std::ptr::null_mut(),
    );
    if ok == 0 { None } else { from_wide(&buf) }
}

#[cfg(windows)]
unsafe fn device_property_string(
    set: windows_sys::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    data: &windows_sys::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
    key: &windows_sys::Win32::Foundation::DEVPROPKEY,
) -> Option<String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::SetupDiGetDevicePropertyW;
    use windows_sys::Win32::Devices::Properties::DEVPROP_TYPE_STRING;
    let mut buf = [0u16; 512];
    let mut prop_type = 0u32;
    let ok = SetupDiGetDevicePropertyW(
        set, data, key, &mut prop_type,
        buf.as_mut_ptr() as *mut u8, (buf.len() * 2) as u32, std::ptr::null_mut(), 0,
    );
    if ok == 0 || prop_type != DEVPROP_TYPE_STRING { None } else { from_wide(&buf) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vid_pid_and_serial() {
        let id = r"USB\VID_046D&PID_C52B\5&2A3B4C&0&1";
        assert_eq!(hex_after(id, "VID_"), 0x046D);
        assert_eq!(hex_after(id, "PID_"), 0xC52B);
        assert_eq!(serial_from_instance_id(id), None);
        assert_eq!(serial_from_instance_id(r"USB\VID_0781&PID_5581\4C530001231122"), Some("4C530001231122".into()));
    }

    /// Lista os dispositivos desta máquina: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn prints_snapshot() {
        let mut list: Vec<_> = snapshot().unwrap().into_values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        for d in list {
            println!("{:<10} {:04X}:{:04X} {:<40} {:?} [{:?}]", d.transport, d.vendor_id, d.product_id, d.name, d.manufacturer, d.device_class);
        }
    }

    #[test]
    fn renames_ajazz_receiver() {
        let mut d = Peripheral {
            id: "usb:x".into(), name: "Wireless-Receiver".into(), manufacturer: Some("XCTECH".into()),
            vendor_id: 0x249A, product_id: 0x5C2F, serial_number: Some("2025".into()), path: String::new(),
            device_class: Some("USB".into()), battery_percent: None, battery_charging: false, transport: "USB".into(),
        };
        apply_known_name(&mut d);
        assert_eq!(d.name, "Mouse AJAZZ AJ179P MC");
        assert_eq!(d.manufacturer.as_deref(), Some("AJAZZ"));
    }

    #[test]
    fn generic_names() {
        assert!(is_generic_name("Dispositivo composto USB"));
        assert!(!is_generic_name("USB Receiver"));
    }
}
