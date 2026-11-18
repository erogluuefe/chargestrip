#![windows_subsystem = "windows"]

mod autostart;
mod devices;
mod icon;
mod model;
mod poller;
#[cfg(windows)]
mod taskbar;
mod tray;

#[cfg_attr(windows, path = "platform/windows.rs")]
#[cfg_attr(target_os = "macos", path = "platform/macos.rs")]
mod platform;

fn main() {
    platform::run();
}
