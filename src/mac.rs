//! macOS arayüzü: menü çubuğunda tek bir durum öğesi, başlığı ör. "🎧 75%  🖱 64% ⚡".
//! Menü çubuğu öğesi NSApplication olay döngüsü ister; bunu `tao` sağlar. Dock'ta simge çıkmaz.
//! Not: Bu dosya Windows'ta yazıldı; macOS'ta henüz derlenip denenmedi.

use crate::model::{Kind, Shown, Tone};
use crate::poller;
use std::sync::mpsc::Sender;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

enum AppEvent {
    Update(Vec<Shown>),
    Menu(MenuEvent),
}

fn emoji(kind: Kind) -> &'static str {
    match kind {
        Kind::Headset => "🎧",
        Kind::Mouse => "🖱",
    }
}

/// Menü çubuğu başlığı. Başlık tek renk olduğundan durum sembolle anlatılır.
fn title(items: &[Shown]) -> String {
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
    _icon: TrayIcon,
    refresh: MenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
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
            // Menü çubuğu öğesi ancak olay döngüsü başladıktan sonra oluşturulabilir.
            Event::NewEvents(StartCause::Init) => {
                let refresh_item = MenuItem::new("Yenile", true, None);
                let autostart = CheckMenuItem::new("Açılışta başlat", true, autostart::is_enabled(), None);
                let quit = MenuItem::new("Çıkış", true, None);
                let menu = Menu::new();
                let _ = menu.append_items(&[&refresh_item, &autostart, &PredefinedMenuItem::separator(), &quit]);
                let Ok(icon) = TrayIconBuilder::new().with_title("…").with_menu(Box::new(menu)).build() else {
                    *flow = ControlFlow::Exit;
                    return;
                };
                ui = Some(Ui { _icon: icon, refresh: refresh_item, autostart, quit });
                let p = proxy.clone();
                refresh = Some(poller::spawn(move |items| {
                    let _ = p.send_event(AppEvent::Update(items));
                }));
            }
            Event::UserEvent(AppEvent::Update(items)) => {
                if let Some(ui) = &ui {
                    ui._icon.set_title(Some(title(&items)));
                }
            }
            Event::UserEvent(AppEvent::Menu(e)) => {
                let Some(ui) = &ui else { return };
                if e.id == *ui.refresh.id() {
                    if let Some(r) = &refresh {
                        let _ = r.send(());
                    }
                } else if e.id == *ui.autostart.id() {
                    autostart::set_enabled(ui.autostart.is_checked());
                } else if e.id == *ui.quit.id() {
                    *flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

/// Açılışta başlatma: ~/Library/LaunchAgents altında bir LaunchAgent dosyası.
mod autostart {
    use std::path::PathBuf;

    fn plist() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/LaunchAgents/com.pilgostergesi.plist"))
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
  <key>Label</key><string>com.pilgostergesi</string>
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
