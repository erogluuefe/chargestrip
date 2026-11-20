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
use crate::update::{self, Update};
use crate::{devices, poller};
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::mpsc;
use strip::{Command, Owner, Strip, WM_APP_MENU};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Controls::{InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX};
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
    // Tooltips come from the common controls library.
    let icc = INITCOMMONCONTROLSEX { dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_WIN95_CLASSES };
    unsafe { InitCommonControlsEx(&icc) };

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

    let owner = Owner::create();
    // One strip per taskbar (one per monitor); empty while Explorer is closed or restarting.
    // Rebuilt on a tick when taskbars come or go (monitor plugged in, Explorer restarted).
    let mut strips: Vec<Strip> = Vec::new();
    let mut items: Vec<Shown> = Vec::new();
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        if !msg.hwnd.is_null() {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else if msg.message == WM_TIMER {
            let bars = strip::taskbars();
            let current: Vec<HWND> = strips.iter().map(Strip::taskbar).collect();
            if bars != current || !strips.iter().all(Strip::alive) {
                strips = bars.into_iter().filter_map(Strip::create).collect();
                strips.iter_mut().for_each(|s| s.set(&items));
            } else {
                strips.iter_mut().for_each(Strip::tick);
            }
        } else if msg.message == WM_APP_MENU {
            match owner.menu(autostart::is_enabled(), pending.as_ref().map(|u| u.version.as_str())) {
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
                        if owner.message(&fill(t().ask_update, &u.version), true) && install(&u) {
                            return;
                        }
                        pending = Some(u);
                    }
                    Ok(None) => {
                        owner.message(&fill(t().up_to_date, ""), false);
                    }
                    Err(()) => {
                        owner.message(t().check_failed, false);
                    }
                },
                // Fresh HID access on this thread; reading the devices takes up to a few seconds.
                Some(Command::CopyDiagnostics) => {
                    let mut api = hidapi::HidApi::new().ok();
                    if copy_to_clipboard(owner.hwnd(), &devices::diagnostics(api.as_mut())) {
                        owner.message(t().diagnostics_copied, false);
                    }
                }
                None => {}
            }
        }

        if let Ok(u) = update_rx.try_recv() {
            pending = Some(u);
        }
        while let Ok(new) = rx.try_recv() {
            items = new;
            strips.iter_mut().for_each(|s| s.set(&items));
        }
    }
}

/// Puts `text` on the clipboard as Unicode text.
fn copy_to_clipboard(owner: HWND, text: &str) -> bool {
    const CF_UNICODETEXT: u32 = 13;
    let data: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(owner) == 0 {
            return false;
        }
        EmptyClipboard();
        // On success the clipboard owns the memory; it must not be freed here.
        let mem = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2);
        let ok = !mem.is_null() && {
            let dst = GlobalLock(mem) as *mut u16;
            !dst.is_null() && {
                std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
                GlobalUnlock(mem);
                !SetClipboardData(CF_UNICODETEXT, mem).is_null()
            }
        };
        CloseClipboard();
        ok
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
