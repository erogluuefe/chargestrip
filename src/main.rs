//! Chargestrip: shows the battery level of wireless headsets, mice and other devices in the
//! Windows taskbar and the macOS menu bar.
//!
//! Layers:
//! - `devices`: device drivers (read-only battery queries over USB HID, plus Bluetooth)
//! - `poller`:  periodic polling on a background thread
//! - `model`:   platform-independent display decisions
//! - `update`:  release check, download and install
//! - `i18n`:    UI strings (English / Turkish, following the system language)
//! - `win` / `mac`: platform-specific UI

#![windows_subsystem = "windows"]

mod devices;
mod i18n;
mod model;
mod poller;
mod update;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    win::run();
    #[cfg(target_os = "macos")]
    mac::run();
}
