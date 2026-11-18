//! Windows açılışında başlatma: HKCU\…\Run altında programın yolu.

use super::wide;
use std::ptr::null_mut;
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "PilGostergesi";

pub fn is_enabled() -> bool {
    let (key, value) = (wide(RUN_KEY), wide(VALUE));
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ,
            null_mut(), null_mut(), &mut size,
        )
    };
    status == 0
}

/// Kaydı ekler/siler; çalışan programın şu anki yolu yazılır.
pub fn set_enabled(on: bool) {
    let (key, value) = (wide(RUN_KEY), wide(VALUE));
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let data = wide(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(
                HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ,
                data.as_ptr().cast(), (data.len() * 2) as u32,
            );
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr());
        }
    }
}
