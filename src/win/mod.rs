//! Windows UI: the taskbar strip and a plain Win32 message loop (no window framework or GUI
//! library, for the lowest memory use).
//!
//! The main thread only waits for messages. What wakes it up:
//! - `WM_APP`: the polling thread sent new readings, or the update check found a new version
//! - `WM_TIMER` (2 s): did the strip's position or theme change, did Explorer restart
//! - `strip::WM_APP_MENU`: the strip was clicked (left or right)

mod autostart;
mod strip;

use crate::i18n::{fill, t};
use crate::model::Shown;
use crate::poller;
use crate::update::{self, Update};
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::mpsc;
use strip::{Command, Strip, WM_APP_MENU};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, SetTimer, TranslateMessage, MSG, WM_APP,
    WM_TIMER,
};

const TICK_MS: u32 = 2000;

pub fn run() {
    let main_thread = unsafe { GetCurrentThreadId() };
    // The timer is the first USER32 call and creates this thread's message queue; messages the
    // polling thread posted before that would be lost.
    unsafe { SetTimer(null_mut(), 0, TICK_MS, None) };

    let (tx, rx) = mpsc::channel::<Vec<Shown>>();
    let refresh = poller::spawn(move |items| {
        let _ = tx.send(items);
        unsafe { PostThreadMessageW(main_thread, WM_APP, 0, 0) };
    });
    let (update_tx, update_rx) = mpsc::channel::<Update>();
    update::spawn(move |u| {
        let _ = update_tx.send(u);
        unsafe { PostThreadMessageW(main_thread, WM_APP, 0, 0) };
    });
    let mut pending: Option<Update> = None;

    // `None` while there is no taskbar (Explorer closed or restarting); retried on every tick.
    let mut strip = Strip::create();
    let mut items: Vec<Shown> = Vec::new();
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        if !msg.hwnd.is_null() {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else if msg.message == WM_TIMER {
            match &mut strip {
                Some(s) if s.alive() => s.tick(),
                _ => {
                    strip = Strip::create();
                    if let Some(s) = &mut strip {
                        s.set(&items);
                    }
                }
            }
        } else if msg.message == WM_APP_MENU {
            let Some(s) = &strip else { continue };
            match s.menu(autostart::is_enabled(), pending.as_ref().map(|u| u.version.as_str())) {
                Some(Command::Refresh) => {
                    let _ = refresh.send(());
                }
                Some(Command::ToggleAutostart) => {
                    autostart::set_enabled(!autostart::is_enabled());
                }
                Some(Command::Quit) => return,
                // The download takes a few seconds (~2 MB); exit once the installer starts, it
                // relaunches the app. If the download fails the entry stays for another try.
                Some(Command::Update) => {
                    if pending.as_ref().is_some_and(install) {
                        return;
                    }
                }
                // Manual check: the result is shown in a message box; a new version offers to install now.
                Some(Command::CheckUpdate) => match update::check() {
                    Ok(Some(u)) => {
                        if s.message(&fill(t().ask_update, &u.version), true) && install(&u) {
                            return;
                        }
                        pending = Some(u);
                    }
                    Ok(None) => {
                        s.message(&fill(t().up_to_date, ""), false);
                    }
                    Err(()) => {
                        s.message(t().check_failed, false);
                    }
                },
                None => {}
            }
        }

        if let Ok(u) = update_rx.try_recv() {
            pending = Some(u);
        }
        while let Ok(new) = rx.try_recv() {
            items = new;
            if let Some(s) = &mut strip {
                s.set(&items);
            }
        }
    }
}

/// Downloads the new version and starts the installer; `true` if it started (exit right away).
fn install(u: &Update) -> bool {
    update::download(u).is_some_and(|file| update::install(&file))
}

/// NUL-terminated UTF-16 string for Win32 APIs.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A DWORD value under HKCU; `None` if missing.
fn reg_dword(key: &str, value: &str) -> Option<u32> {
    let (key, value) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_DWORD,
            null_mut(), &mut data as *mut u32 as *mut c_void, &mut size,
        )
    };
    (status == 0).then_some(data)
}
