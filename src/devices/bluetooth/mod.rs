//! Bluetooth devices: the battery level the operating system reads from connected devices and
//! keeps (the value shown in Windows Settings → Bluetooth and in the macOS Bluetooth menu). No
//! per-model driver is needed and nothing is sent to the device. Bluetooth has no charging flag.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use super::Reading;

/// Connected Bluetooth devices with a known battery level, in a stable order (by key).
pub fn poll() -> Vec<Reading> {
    #[cfg(windows)]
    return windows::poll();
    #[cfg(target_os = "macos")]
    return macos::poll();
}
