//! Windows: tao/winit yerine düz Win32 mesaj döngüsü (en düşük bellek).
//! tray-icon kendi gizli pencerelerini bu iş parçacığında oluşturur; bize sadece
//! mesajları dağıtmak kalır. Sorgu iş parçacığı yeni veri gönderince WM_APP ile uyandırır.

use crate::poller::{self, Update};
use crate::tray::{Action, Tray};
use std::sync::mpsc;
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_APP,
};

pub fn run() {
    // Simge üstündeki fare hareketleri kuyrukta birikmesin.
    TrayIconEvent::set_event_handler(Some(|_| {}));

    let Ok(tray) = Tray::new() else { return };

    let main_thread = unsafe { GetCurrentThreadId() };
    let (tx, rx) = mpsc::channel::<Update>();
    let refresh = poller::spawn(move |u| {
        let _ = tx.send(u);
        unsafe { PostThreadMessageW(main_thread, WM_APP, 0, 0) };
    });

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        if unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        while let Ok(u) = rx.try_recv() {
            tray.apply(&u);
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            match tray.on_menu(&e) {
                Action::Refresh => {
                    let _ = refresh.send(());
                }
                Action::Quit => return,
                Action::None => {}
            }
        }
    }
}
