//! macOS arayüzü: menü çubuğunda tek bir durum öğesi, başlığı ör. "🎧 75%  🖱 64% ⚡".
//! Menü çubuğu öğesi NSApplication olay döngüsü ister; bunu `tao` sağlar. Dock'ta simge çıkmaz.
//! Not: CI'da derleniyor; macOS'ta ilk kez 2026-11-19'da (alıcısız) çalıştırıldı.

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

/// Menü çubuğu başlığı. Başlık tek renk olduğundan durum sembolle anlatılır.
/// Hiç alıcı takılı değilse "🔋 –": öğe kaybolmasın, uygulamanın çalıştığı görünsün.
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
    /// "Güncellemeleri denetle"; elle denetimin sonucu da bu satırda yazar.
    check: MenuItem,
    quit: MenuItem,
    /// Yayımlanmış yeni sürüm ve menünün başına eklenen "Güncelle" öğesi.
    update: Option<(Update, MenuItem)>,
}

impl Ui {
    /// Yeni sürümü menünün başına "Güncelle (x.y.z)" olarak ekler (bir kez).
    fn offer(&mut self, u: Update) {
        if self.update.is_some() {
            return;
        }
        let item = MenuItem::new(format!("Güncelle ({})", u.version), true, None);
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
            // Menü çubuğu öğesi ancak olay döngüsü başladıktan sonra oluşturulabilir.
            Event::NewEvents(StartCause::Init) => {
                let refresh_item = MenuItem::new("Yenile", true, None);
                let autostart = CheckMenuItem::new("Açılışta başlat", true, autostart::is_enabled(), None);
                let check = MenuItem::new("Güncellemeleri denetle", true, None);
                let quit = MenuItem::new("Çıkış", true, None);
                // Yalnızca bilgi: soluk, tıklanmaz.
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
                    // Elle denetim; sonuç menüde yazar, yeni sürüm varsa en üstte "Güncelle" çıkar.
                    match update::check() {
                        Ok(Some(u)) => {
                            ui.check.set_text(format!("Yeni sürüm var: {} (en üstte)", u.version));
                            ui.offer(u);
                        }
                        Ok(None) => ui.check.set_text(format!("Güncel ({}) ✓ — tekrar denetle", update::CURRENT)),
                        Err(()) => ui.check.set_text("Denetlenemedi — tekrar dene"),
                    }
                } else if let Some((u, item)) = &ui.update {
                    // İndirme birkaç saniye sürer; kurulum betiği bu süreç çıkınca paketi değiştirip
                    // yeniden açar. Başarısızsa öğe menüde kalır, tekrar denenebilir.
                    if e.id == *item.id() && update::download(u).is_some_and(|f| update::install(&f)) {
                        *flow = ControlFlow::Exit;
                    }
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
