//! macOS: menü çubuğu simgeleri NSApplication döngüsü ister; bunu tao sağlar.
//! Dock'ta simge görünmez (Accessory).

use crate::poller::{self, Update};
use crate::tray::{Action, Tray};
use std::sync::mpsc::Sender;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;

enum AppEvent {
    Update(Update),
    Menu(MenuEvent),
}

pub fn run() {
    let mut event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(AppEvent::Menu(e));
    }));
    TrayIconEvent::set_event_handler(Some(|_| {}));

    let mut tray: Option<Tray> = None;
    let mut refresh: Option<Sender<()>> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                tray = Tray::new().ok();
                if tray.is_none() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                let p = proxy.clone();
                refresh = Some(poller::spawn(move |u| {
                    let _ = p.send_event(AppEvent::Update(u));
                }));
            }
            Event::UserEvent(AppEvent::Update(u)) => {
                if let Some(t) = &tray {
                    t.apply(&u);
                }
            }
            Event::UserEvent(AppEvent::Menu(e)) => {
                let action = tray.as_ref().map_or(Action::None, |t| t.on_menu(&e));
                match action {
                    Action::Refresh => {
                        if let Some(r) = &refresh {
                            let _ = r.send(());
                        }
                    }
                    Action::Quit => {
                        tray = None;
                        *control_flow = ControlFlow::Exit;
                    }
                    Action::None => {}
                }
            }
            _ => {}
        }
    });
}
