//! Windows: Bluetooth battery levels from Device Manager properties.
//!
//! Windows keeps a connected device's level (HFP for classic devices, GATT Battery Service for LE)
//! in the `BATTERY` property of one of the device's nodes. A physical device has several nodes
//! (the device itself plus its services); all share one container ID, which is used to merge
//! them. Connection state and kind come from the device's own node.

use crate::devices::Reading;
use crate::model::{Battery, Kind};
use std::collections::BTreeMap;
use std::ptr::{null, null_mut};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW,
    SetupDiGetDevicePropertyW, DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SP_DEVINFO_DATA,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_DeviceContainer_Category, DEVPKEY_Device_ContainerId, DEVPKEY_Device_FriendlyName, DEVPROPTYPE,
    DEVPROP_TYPE_BOOLEAN, DEVPROP_TYPE_BYTE, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING, DEVPROP_TYPE_STRING_LIST,
    DEVPROP_TYPE_UINT32,
};
use windows_sys::Win32::Foundation::DEVPROPKEY;

/// Battery level read by Windows (byte, 0–100).
const BATTERY: DEVPROPKEY = key(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5, 2);
/// Whether the device is connected right now (DEVPROP_BOOLEAN).
const IS_CONNECTED: DEVPROPKEY = key(0x83DA6326_97A6_4088_9453_A1923F573B29, 15);
/// Classic Bluetooth "Class of Device" (u32); usually missing on LE devices.
const CLASS_OF_DEVICE: DEVPROPKEY = key(0x2BD67D8B_8BEB_48D5_87E0_6CDA3428040A, 10);

const fn key(fmtid: u128, pid: u32) -> DEVPROPKEY {
    DEVPROPKEY { fmtid: GUID::from_u128(fmtid), pid }
}

/// What was collected for one physical device.
#[derive(Default)]
struct Device {
    name: String,
    kind: Option<Kind>,
    /// Connection state of the device's own node (connected if either its classic or LE node is).
    connected: Option<bool>,
    /// (percent, whether the node carrying it is connected)
    battery: Option<(u8, bool)>,
}

pub fn poll() -> Vec<Reading> {
    let mut devices: BTreeMap<String, Device> = BTreeMap::new();
    unsafe {
        let set = SetupDiGetClassDevsW(null(), null(), null_mut(), DIGCF_ALLCLASSES | DIGCF_PRESENT);
        if set == -1 {
            return Vec::new(); // INVALID_HANDLE_VALUE
        }
        let mut node = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
        let mut i = 0;
        while SetupDiEnumDeviceInfo(set, i, &mut node) != 0 {
            i += 1;
            let id = instance_id(set, &node);
            if !["BTHENUM\\", "BTHLE\\", "BTHLEDEVICE\\"].iter().any(|p| id.starts_with(p)) {
                continue;
            }
            let p = |key: &DEVPROPKEY, ty: DEVPROPTYPE, buf: &mut [u8]| property(set, &node, key, ty, buf);
            let mut guid = [0u8; 16];
            let container = match p(&DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID, &mut guid) {
                Some(_) => guid.iter().map(|b| format!("{b:02x}")).collect(),
                None => id.clone(),
            };
            let dev = devices.entry(container).or_default();
            let mut flag = [0u8; 1];
            let connected = p(&IS_CONNECTED, DEVPROP_TYPE_BOOLEAN, &mut flag).map(|_| flag[0] != 0);

            // The device's own node ("...\DEV_<address>"); the others are its services.
            if id.starts_with("BTHENUM\\DEV_") || id.starts_with("BTHLE\\DEV_") {
                let mut cod = [0u8; 4];
                let mut category = [0u8; 256];
                let mut name = [0u8; 256];
                if let Some(n) = p(&DEVPKEY_Device_FriendlyName, DEVPROP_TYPE_STRING, &mut name) {
                    dev.name = first_string(&name[..n]);
                }
                dev.kind = dev
                    .kind
                    .or_else(|| p(&CLASS_OF_DEVICE, DEVPROP_TYPE_UINT32, &mut cod).and_then(|_| kind_from_class(u32::from_le_bytes(cod))))
                    .or_else(|| {
                        let n = p(&DEVPKEY_DeviceContainer_Category, DEVPROP_TYPE_STRING_LIST, &mut category)?;
                        kind_from_category(&first_string(&category[..n]))
                    });
                if let Some(c) = connected {
                    dev.connected = Some(dev.connected.unwrap_or(false) || c);
                }
            }

            let mut level = [0u8; 1];
            if p(&BATTERY, DEVPROP_TYPE_BYTE, &mut level).is_some() && level[0] <= 100 {
                // Headphones keep the level on the Hands-Free node, which reads as disconnected
                // while audio plays; prefer a connected node's value. The device's own node decides.
                let live = connected.unwrap_or(true);
                if !(dev.battery.is_some_and(|(_, prev_live)| prev_live) && !live) {
                    dev.battery = Some((level[0], live));
                }
            }
        }
        SetupDiDestroyDeviceInfoList(set);
    }

    devices
        .into_iter()
        .filter_map(|(container, d)| {
            let (percent, live) = d.battery?;
            d.connected.unwrap_or(live).then(|| Reading {
                key: format!("bt:{container}"),
                name: if d.name.is_empty() { "Bluetooth device".into() } else { d.name },
                kind: d.kind.unwrap_or(Kind::Other),
                battery: Some(Battery { percent, charging: false }),
                tentative: false,
            })
        })
        .collect()
}

/// The node's device instance ID, e.g. `BTHENUM\DEV_6CFBED129847\...`.
unsafe fn instance_id(set: HDEVINFO, node: &SP_DEVINFO_DATA) -> String {
    let mut buf = [0u16; 512];
    if SetupDiGetDeviceInstanceIdW(set, node, buf.as_mut_ptr(), buf.len() as u32, null_mut()) == 0 {
        return String::new();
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Reads a property into `buf`: the number of bytes written, or `None` if missing or not of type `ty`.
unsafe fn property(set: HDEVINFO, node: &SP_DEVINFO_DATA, key: &DEVPROPKEY, ty: DEVPROPTYPE, buf: &mut [u8]) -> Option<usize> {
    let (mut found_ty, mut size) = (0, 0);
    let ok = SetupDiGetDevicePropertyW(set, node, key, &mut found_ty, buf.as_mut_ptr(), buf.len() as u32, &mut size, 0);
    (ok != 0 && found_ty == ty).then_some(size as usize)
}

/// First entry of a UTF-16 string list (NUL-separated).
fn first_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
    let len = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..len])
}

/// Classic Bluetooth "Class of Device": major class (bits 8–12) and minor class (bits 2–7).
fn kind_from_class(cod: u32) -> Option<Kind> {
    let (major, minor) = ((cod >> 8) & 0x1F, (cod >> 2) & 0x3F);
    match major {
        0x04 => Some(Kind::Headset), // audio/video
        0x05 if matches!(minor & 0x0F, 0x01 | 0x02) => Some(Kind::Gamepad), // joystick, gamepad
        0x05 if minor >> 4 == 1 => Some(Kind::Keyboard),
        0x05 if minor >> 4 == 2 => Some(Kind::Mouse),
        _ => None,
    }
}

/// Device category Windows uses in Settings (instead of Class of Device on LE devices).
fn kind_from_category(category: &str) -> Option<Kind> {
    match category {
        "Input.Keyboard" => Some(Kind::Keyboard),
        "Input.Mouse" => Some(Kind::Mouse),
        "Input.Gaming" => Some(Kind::Gamepad),
        "Audio.Headphones" => Some(Kind::Headset),
        c if c.starts_with("Communication.Headset") => Some(Kind::Headset),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_of_device() {
        assert_eq!(kind_from_class(0x24_0404), Some(Kind::Headset)); // wearable headset
        assert_eq!(kind_from_class(0x0540), Some(Kind::Keyboard));
        assert_eq!(kind_from_class(0x0580), Some(Kind::Mouse));
        assert_eq!(kind_from_class(0x0508), Some(Kind::Gamepad));
        assert_eq!(kind_from_class(0x5A_020C), None); // phone
        assert_eq!(kind_from_class(0), None);
    }

    #[test]
    fn category() {
        assert_eq!(kind_from_category("Input.Mouse"), Some(Kind::Mouse));
        assert_eq!(kind_from_category("Communication.Headset.Bluetooth"), Some(Kind::Headset));
        assert_eq!(kind_from_category("Communication.Phone"), None);
    }

    #[test]
    fn string_list_first_entry() {
        let bytes: Vec<u8> = "Input.Mouse\0Other\0\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(first_string(&bytes), "Input.Mouse");
    }
}
