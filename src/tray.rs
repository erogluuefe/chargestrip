//! İki tepsi simgesi (kulaklık, fare) ve ortak sağ tık menüsü. Platformdan bağımsız.

use crate::autostart;
use crate::icon::{render_icon, Kind, SIZE};
use crate::model::{Shown, DASH, GRAY};
use crate::poller::{Update, HEADSET_NAME, MOUSE_NAME};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub enum Action {
    None,
    Refresh,
    Quit,
}

pub struct Tray {
    headset: TrayIcon,
    mouse: TrayIcon,
    refresh: MenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
}

fn icon(kind: Kind, s: &Shown) -> Option<Icon> {
    Icon::from_rgba(render_icon(kind, &s.text, s.color), SIZE, SIZE).ok()
}

fn waiting(name: &str) -> Shown {
    Shown { text: DASH.into(), color: GRAY, tooltip: format!("{name}: okunuyor…") }
}

fn build(id: &str, kind: Kind, s: &Shown, menu: &Menu) -> Result<TrayIcon, String> {
    let mut b = TrayIconBuilder::new()
        .with_id(id)
        .with_tooltip(&s.tooltip)
        .with_menu(Box::new(menu.clone()));
    if let Some(i) = icon(kind, s) {
        b = b.with_icon(i);
    }
    b.build().map_err(|e| e.to_string())
}

impl Tray {
    /// macOS'ta olay döngüsü başladıktan sonra çağrılmalı.
    pub fn new() -> Result<Self, String> {
        let refresh = MenuItem::new("Yenile", true, None);
        let autostart = CheckMenuItem::new("Açılışta başlat", true, autostart::is_enabled(), None);
        let quit = MenuItem::new("Çıkış", true, None);
        let menu = Menu::new();
        menu.append_items(&[&refresh, &autostart, &PredefinedMenuItem::separator(), &quit])
            .map_err(|e| e.to_string())?;

        let mouse = build("fare", Kind::Mouse, &waiting(MOUSE_NAME), &menu)?;
        let headset = build("kulaklik", Kind::Headset, &waiting(HEADSET_NAME), &menu)?;
        Ok(Self { headset, mouse, refresh, autostart, quit })
    }

    pub fn apply(&self, u: &Update) {
        for (tray, kind, s) in [(&self.headset, Kind::Headset, &u.headset), (&self.mouse, Kind::Mouse, &u.mouse)] {
            let _ = tray.set_icon(icon(kind, s));
            let _ = tray.set_tooltip(Some(&s.tooltip));
        }
    }

    pub fn on_menu(&self, e: &MenuEvent) -> Action {
        if e.id == *self.refresh.id() {
            Action::Refresh
        } else if e.id == *self.quit.id() {
            Action::Quit
        } else if e.id == *self.autostart.id() {
            // Menü işareti tıklamayla zaten değişti; ayar başarısızsa geri al.
            let want = self.autostart.is_checked();
            if autostart::set_enabled(want).is_err() {
                self.autostart.set_checked(!want);
            }
            Action::None
        } else {
            Action::None
        }
    }
}
