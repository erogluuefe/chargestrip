//! macOS UI: a single status item in the menu bar, titled e.g. "🎧 75%  🖱 64% ⚡".
//! A menu bar item needs an NSApplication event loop, which `tao` provides. No Dock icon.

use crate::i18n::{fill, t};
use crate::model::{Kind, Shown, Tone};
use crate::poller;
use crate::update::{self, Update};
use std::sync::mpsc::Sender;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

enum AppEvent {
    Readings(Vec<Shown>),
    NewVersion(Update),
    Menu(MenuEvent),
}

fn emoji(kind: Kind) -> &'static str {
    match kind {
        Kind::Headset => "🎧",
        Kind::Mouse => "🖱",
        Kind::Keyboard => "⌨️",
        Kind::Gamepad => "🎮",
        Kind::Other => "🔵",
    }
}

/// Menu bar title. The title has a single color, so the state is shown with a symbol.
/// "🔋 –" when nothing is connected, so the item never disappears and the app is visibly running.
fn title(items: &[Shown]) -> String {
    if items.is_empty() {
        return "🔋 –".into();
    }
    let parts: Vec<String> = items
        .iter()
        .map(|s| {
            let mark = match s.tone {
                Tone::Normal => "",
                Tone::Low => " ⚠",
                Tone::Charging => " ⚡",
                Tone::Inactive => " ·",
            };
            format!("{} {}{mark}", emoji(s.kind), s.text)
        })
        .collect();
    parts.join("  ")
}

struct Ui {
    icon: TrayIcon,
    menu: Menu,
    refresh: MenuItem,
    autostart: CheckMenuItem,
    /// "Check for updates"; a manual check's result is also written here.
    check: MenuItem,
    quit: MenuItem,
    /// A published new version and the "Update" entry added at the top of the menu.
    update: Option<(Update, MenuItem)>,
}

impl Ui {
    /// Adds the new version at the top of the menu as "Update to x.y.z" (once).
    fn offer(&mut self, u: Update) {
        if self.update.is_some() {
            return;
        }
        let item = MenuItem::new(fill(t().update_to, &u.version), true, None);
        let _ = self.menu.prepend_items(&[&item, &PredefinedMenuItem::separator()]);
        self.update = Some((u, item));
    }
}

pub fn run() {
    let mut event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(AppEvent::Menu(e));
    }));

    let mut ui: Option<Ui> = None;
    let mut refresh: Option<Sender<()>> = None;
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            // The menu bar item can only be created once the event loop has started.
            Event::NewEvents(StartCause::Init) => {
                let refresh_item = MenuItem::new(t().refresh, true, None);
                let autostart = CheckMenuItem::new(t().autostart, true, autostart::is_enabled(), None);
                let check = MenuItem::new(t().check_updates, true, None);
                let quit = MenuItem::new(t().quit, true, None);
                // Information only: dimmed, not clickable.
                let version = MenuItem::new(format!("Chargestrip {}", update::CURRENT), false, None);
                let menu = Menu::new();
                let _ = menu.append_items(&[
                    &refresh_item,
                    &autostart,
                    &PredefinedMenuItem::separator(),
                    &check,
                    &version,
                    &quit,
                ]);
                let Ok(icon) = TrayIconBuilder::new().with_title("…").with_menu(Box::new(menu.clone())).build()
                else {
                    *flow = ControlFlow::Exit;
                    return;
                };
                ui = Some(Ui { icon, menu, refresh: refresh_item, autostart, check, quit, update: None });
                let p = proxy.clone();
                refresh = Some(poller::spawn(move |items| {
                    let _ = p.send_event(AppEvent::Readings(items));
                }));
                let p = proxy.clone();
                update::spawn(move |u| {
                    let _ = p.send_event(AppEvent::NewVersion(u));
                });
            }
            Event::UserEvent(AppEvent::Readings(items)) => {
                if let Some(ui) = &ui {
                    ui.icon.set_title(Some(title(&items)));
                }
            }
            Event::UserEvent(AppEvent::NewVersion(u)) => {
                if let Some(ui) = &mut ui {
                    ui.offer(u);
                }
            }
            Event::UserEvent(AppEvent::Menu(e)) => {
                let Some(ui) = &mut ui else { return };
                if e.id == *ui.refresh.id() {
                    if let Some(r) = &refresh {
                        let _ = r.send(());
                    }
                } else if e.id == *ui.autostart.id() {
                    autostart::set_enabled(ui.autostart.is_checked());
                } else if e.id == *ui.quit.id() {
                    *flow = ControlFlow::Exit;
                } else if e.id == *ui.check.id() {
                    // Manual check; the result is written in the menu, and a new version adds "Update" at the top.
                    match update::check() {
                        Ok(Some(u)) => {
                            ui.check.set_text(fill(t().label_new_version, &u.version));
                            ui.offer(u);
                        }
                        Ok(None) => ui.check.set_text(fill(t().label_up_to_date, "")),
                        Err(()) => ui.check.set_text(t().label_check_failed),
                    }
                } else if let Some((u, item)) = &ui.update {
                    // The download takes a few seconds; the install script swaps the bundle once this
                    // process exits and reopens it. On failure the entry stays for another try.
                    if e.id == *item.id() && update::download(u).is_some_and(|f| update::install(&f)) {
                        *flow = ControlFlow::Exit;
                    }
                }
            }
            _ => {}
        }
    });
}

/// Start at login: a LaunchAgent file in ~/Library/LaunchAgents.
mod autostart {
    use std::path::PathBuf;

    fn plist() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/LaunchAgents/com.chargestrip.plist"))
    }

    pub fn is_enabled() -> bool {
        plist().is_some_and(|p| p.exists())
    }

    pub fn set_enabled(on: bool) {
        let Some(path) = plist() else { return };
        if !on {
            let _ = std::fs::remove_file(path);
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let exe = exe.display().to_string().replace('&', "&amp;").replace('<', "&lt;");
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.chargestrip</string>
  <key>ProgramArguments</key><array><string>{exe}</string></array>
  <key>RunAtLoad</key><true/>
</dict></plist>
"#
        );
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, body);
    }
}
