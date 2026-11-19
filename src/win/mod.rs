//! Windows arayüzü: taskbar şeridi + düz Win32 mesaj döngüsü (pencere çerçevesi/kütüphanesi yok,
//! en düşük bellek için).
//!
//! Ana iş parçacığı yalnızca mesaj bekler. Uyandıran şeyler:
//! - `WM_APP`: sorgu iş parçacığı yeni sonuç ya da güncelleme denetimi yeni sürüm gönderdi
//! - `WM_TIMER` (2 sn): şeridin konumu/teması değişti mi, Explorer yeniden başladı mı
//! - `strip::WM_APP_MENU`: şeride tıklandı (sol ya da sağ)

mod autostart;
mod strip;

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
    // Zamanlayıcı ilk USER32 çağrısıdır ve iş parçacığının mesaj kuyruğunu oluşturur; sorgu
    // iş parçacığının mesajları ondan önce gönderilirse kaybolurdu.
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

    // Taskbar yoksa (Explorer kapalı/yeniden başlıyor) `None` kalır, her tick'te yeniden denenir.
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
                // İndirme birkaç saniye sürer (~2 MB); kurulum başlayınca çıkılır, kurulum
                // uygulamayı yeniden açar. İndirme başarısızsa menüde kalır, tekrar denenebilir.
                Some(Command::Update) => {
                    if pending.as_ref().is_some_and(install) {
                        return;
                    }
                }
                // Elle denetim: sonuç bir pencereyle söylenir; yeni sürüm varsa hemen kurmak sorulur.
                Some(Command::CheckUpdate) => match update::check() {
                    Ok(Some(u)) => {
                        let question = format!(
                            "Chargestrip {} yayımlandı (kurulu: {}).\nŞimdi güncellensin mi?",
                            u.version,
                            update::CURRENT
                        );
                        if s.message(&question, true) && install(&u) {
                            return;
                        }
                        pending = Some(u);
                    }
                    Ok(None) => {
                        s.message(&format!("Chargestrip güncel ({}).", update::CURRENT), false);
                    }
                    Err(()) => {
                        s.message("Güncelleme denetlenemedi. İnternet bağlantısını kontrol edip tekrar dene.", false);
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

/// Yeni sürümü indirip kurulumu başlatır; başladıysa `true` (çağıran hemen çıkmalı).
fn install(u: &Update) -> bool {
    update::download(u).is_some_and(|file| update::install(&file))
}

/// Win32 API'leri için sonu sıfırla biten UTF-16 dizgi.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// HKCU altındaki bir DWORD değeri; yoksa `None`.
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
