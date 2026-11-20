//! macOS UI: a single status item in the menu bar showing a monochrome symbol and percentage per
//! device ([`title`]). A menu bar item needs an NSApplication event loop, which `tao` provides.
//! No Dock icon.

mod title;

use crate::i18n::{fill, t};
use crate::model::{Kind, Shown, Tone};
use crate::poller;
use crate::update::{self, Update};
use objc2::MainThreadMarker;
use objc2_app_kit::NSCellImagePosition;
use objc2_foundation::NSString;
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

/// Text version of the title: the tooltip, and the title itself if the symbols can't be drawn.
/// "🔋 –" when nothing is connected, so the item never disappears and the app is visibly running.
fn text_title(items: &[Shown]) -> String {
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
    diagnostics: MenuItem,
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

/// Shows the readings: the symbol image as the status item's button image, the text version as
/// its tooltip. Falls back to a text title if the image can't be drawn.
fn show(icon: &TrayIcon, items: &[Shown]) {
    let text = text_title(items);
    // One line per device with its name and state.
    let tip = if items.is_empty() {
        t().tip_none.to_string()
    } else {
        items.iter().map(|s| s.tip.as_str()).collect::<Vec<_>>().join("\n")
    };
    let _ = icon.set_tooltip(Some(&tip));
    let button = MainThreadMarker::new().zip(icon.ns_status_item()).and_then(|(mtm, item)| item.button(mtm));
    match (button, title::render(items)) {
        (Some(button), Some(image)) => {
            button.setTitle(&NSString::from_str(""));
            button.setImage(Some(&image));
            button.setImagePosition(NSCellImagePosition::ImageOnly);
        }
        _ => icon.set_title(Some(text)),
    }
}

/// Puts `text` on the clipboard with the system's `pbcopy`.
fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new("/usr/bin/pbcopy").stdin(Stdio::piped()).spawn() else { return false };
    let written = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok_and(|status| status.success()) && written
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
                let diagnostics = MenuItem::new(t().copy_diagnostics, true, None);
                let quit = MenuItem::new(t().quit, true, None);
                // Information only: dimmed, not clickable.
                let version = MenuItem::new(format!("Chargestrip {}", update::CURRENT), false, None);
                let menu = Menu::new();
                let _ = menu.append_items(&[
                    &refresh_item,
                    &autostart,
                    &PredefinedMenuItem::separator(),
                    &check,
                    &diagnostics,
                    &version,
                    &quit,
                ]);
                let Ok(icon) = TrayIconBuilder::new().with_title("…").with_menu(Box::new(menu.clone())).build()
                else {
                    *flow = ControlFlow::Exit;
                    return;
                };
                ui = Some(Ui { icon, menu, refresh: refresh_item, autostart, check, diagnostics, quit, update: None });
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
                    show(&ui.icon, &items);
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
                } else if e.id == *ui.diagnostics.id() {
                    // Fresh HID access on this thread; reading the devices takes up to a few seconds.
                    let mut api = hidapi::HidApi::new().ok();
                    if copy_to_clipboard(&crate::devices::diagnostics(api.as_mut())) {
                        ui.diagnostics.set_text(t().label_diagnostics_copied);
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
