//! Açılışta başlatma: Windows'ta HKCU\...\Run, macOS'ta LaunchAgent.

use auto_launch::{AutoLaunch, AutoLaunchBuilder};

const APP_NAME: &str = "PilGostergesi";

fn launcher() -> Result<AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_str().ok_or("exe yolu UTF-8 değil")?;
    AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(exe)
        .set_use_launch_agent(true)
        .build()
        .map_err(|e| e.to_string())
}

pub fn is_enabled() -> bool {
    launcher().and_then(|l| l.is_enabled().map_err(|e| e.to_string())).unwrap_or(false)
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    let l = launcher()?;
    if on { l.enable() } else { l.disable() }.map_err(|e| e.to_string())
}
