//! Windows: tao/winit yerine düz Win32 mesaj döngüsü (en düşük bellek).
//! Varsayılan görünüm taskbar şeridi (okun solu); şerit kurulamazsa tepsi simgeleri.
//! Sorgu iş parçacığı yeni veri gönderince WM_APP ile uyandırır; 2 sn'lik iş parçacığı
//! zamanlayıcısı şeridin konumunu ve Explorer yeniden başlamalarını takip eder.

use crate::autostart;
use crate::poller::{self, Update};
use crate::taskbar::{Widget, ID_AUTOSTART, ID_QUIT, ID_REFRESH, WM_APP_MENU};
use crate::tray::{Action, Tray};
use std::sync::mpsc;
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, SetTimer, TranslateMessage, MSG, WM_APP,
    WM_TIMER,
};

enum Ui {
    Strip(Widget),
    Tray(Tray),
    /// Şerit vardı ama Explorer yeniden başladı; taskbar dönünce yeniden kurulur.
    Waiting,
}

pub fn run() {
    // Simge üstündeki fare hareketleri kuyrukta birikmesin.
    TrayIconEvent::set_event_handler(Some(|_| {}));

    let mut ui = match Widget::create() {
        Some(w) => Ui::Strip(w),
        None => match Tray::new() {
            Ok(t) => Ui::Tray(t),
            Err(_) => return,
        },
    };

    let main_thread = unsafe { GetCurrentThreadId() };
    let (tx, rx) = mpsc::channel::<Update>();
    let refresh = poller::spawn(move |u| {
        let _ = tx.send(u);
        unsafe { PostThreadMessageW(main_thread, WM_APP, 0, 0) };
    });
    unsafe { SetTimer(std::ptr::null_mut(), 0, 2000, None) };

    let mut last: Option<Update> = None;
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        if unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } <= 0 {
            break;
        }
        let thread_msg = msg.hwnd.is_null();
        if thread_msg && msg.message == WM_TIMER {
            ui = match ui {
                Ui::Strip(w) if !w.alive() => Ui::Waiting,
                Ui::Strip(mut w) => {
                    w.tick();
                    Ui::Strip(w)
                }
                Ui::Waiting => match Widget::create() {
                    Some(mut w) => {
                        if let Some(u) = &last {
                            w.set(u.clone());
                        }
                        Ui::Strip(w)
                    }
                    None => Ui::Waiting,
                },
                other => other,
            };
        } else if thread_msg && msg.message == WM_APP_MENU {
            if let Ui::Strip(w) = &ui {
                match w.menu(autostart::is_enabled()) {
                    ID_REFRESH => {
                        let _ = refresh.send(());
                    }
                    ID_AUTOSTART => {
                        let _ = autostart::set_enabled(!autostart::is_enabled());
                    }
                    ID_QUIT => return,
                    _ => {}
                }
            }
        } else {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        while let Ok(u) = rx.try_recv() {
            match &mut ui {
                Ui::Strip(w) => w.set(u.clone()),
                Ui::Tray(t) => t.apply(&u),
                Ui::Waiting => {}
            }
            last = Some(u);
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            if let Ui::Tray(t) = &ui {
                match t.on_menu(&e) {
                    Action::Refresh => {
                        let _ = refresh.send(());
                    }
                    Action::Quit => return,
                    Action::None => {}
                }
            }
        }
    }
}
