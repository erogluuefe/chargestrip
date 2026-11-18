//! Taskbar şeridi: `Shell_TrayWnd`'nin (taskbar) içine, `TrayNotifyWnd`'nin (bildirim alanı,
//! ^ oku) hemen soluna yerleşen saydam bir alt pencere. Her cihaz için Segoe Fluent Icons ikonu +
//! Segoe UI yüzde yazar. Taskbar'ın alt penceresi olduğundan tam ekran oyunda onunla gizlenir.
//!
//! Çizim: her parça beyaz olarak bir DIB'e yazılır, yazının kenar yumuşatması kapsama (alfa)
//! olarak okunup kendi rengiyle ön-çarpımlı BGRA'ya aktarılır ve `UpdateLayeredWindow` ile
//! basılır. Katmanlı *alt* pencere, programın Windows 8+ uyumluluk bildirmesini ister (build.rs).

use super::{reg_dword, wide};
use crate::model::{Kind, Shown, Tone};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Şeride sağ tıklanınca ana döngüye gönderilen iş parçacığı mesajı.
pub const WM_APP_MENU: u32 = WM_APP + 1;

/// Sağ tık menüsünden seçilen komut.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Refresh = 1,
    ToggleAutostart = 2,
    Quit = 3,
}

type Rgb = [u8; 3];
const RED: Rgb = [0xE5, 0x39, 0x35];
const GREEN: Rgb = [0x43, 0xA0, 0x47];
const GRAY: Rgb = [0x9E, 0x9E, 0x9E];
const DARK_TASKBAR_FG: Rgb = [0xFF, 0xFF, 0xFF];
const LIGHT_TASKBAR_FG: Rgb = [0x1A, 0x1A, 0x1A];

/// 96 DPI'daki ölçüler; çizimde taskbar'ın DPI'ına göre büyütülür.
const ICON_PT: i32 = 16;
const TEXT_PT: i32 = 12;
const EDGE: i32 = 6; // şeridin iki yanındaki boşluk
const ICON_GAP: i32 = 4; // ikon ile yazı arası
const ITEM_GAP: i32 = 10; // iki cihaz arası
const TRAY_GAP: i32 = 2; // şerit ile ^ oku arası

/// Segoe Fluent Icons / Segoe MDL2 Assets kod noktası.
fn glyph(kind: Kind) -> &'static str {
    match kind {
        Kind::Headset => "\u{E7F6}",
        Kind::Mouse => "\u{E962}",
    }
}

/// (ikon rengi, yazı rengi). Kapalı/uykudaki cihazın tamamı soluk görünür.
fn colors(tone: Tone, light_taskbar: bool) -> (Rgb, Rgb) {
    let fg = if light_taskbar { LIGHT_TASKBAR_FG } else { DARK_TASKBAR_FG };
    match tone {
        Tone::Normal => (fg, fg),
        Tone::Low => (fg, RED),
        Tone::Charging => (fg, GREEN),
        Tone::Inactive => (GRAY, GRAY),
    }
}

/// Şeridin taskbar içindeki x konumu: `^` okunun solunda.
fn place_x(taskbar_left: i32, tray_left: i32, width: i32, gap: i32) -> i32 {
    tray_left - taskbar_left - width - gap
}

fn light_taskbar() -> bool {
    reg_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "SystemUsesLightTheme") == Some(1)
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_RBUTTONUP => {
            PostThreadMessageW(GetCurrentThreadId(), WM_APP_MENU, 0, 0);
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Yeniden çizim gerektiren dış koşullar; `tick` bunlar değişmişse çizer.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Env {
    taskbar_left: i32,
    taskbar_height: i32,
    tray_left: i32,
    dpi: u32,
    light: bool,
}

/// Şeritte yazılacak tek bir parça (ikon ya da yazı) ve yerleşimi.
struct Part {
    font: HFONT,
    text: Vec<u16>,
    rgb: Rgb,
    x: i32,
    size: SIZE,
}

pub struct Strip {
    hwnd: HWND,
    /// Sağ tık menüsünün sahibi: görünmez, üst düzey kendi penceremiz (menü odak ister).
    owner: HWND,
    taskbar: HWND,
    tray: HWND,
    items: Vec<Shown>,
    drawn: Option<Env>,
}

impl Strip {
    /// Taskbar bulunamazsa ya da yatay değilse `None`.
    pub fn create() -> Option<Strip> {
        unsafe {
            let taskbar = FindWindowW(wide("Shell_TrayWnd").as_ptr(), null());
            if taskbar.is_null() {
                return None;
            }
            let tray = FindWindowExW(taskbar, null_mut(), wide("TrayNotifyWnd").as_ptr(), null());
            let r = window_rect(taskbar);
            if tray.is_null() || r.right - r.left < r.bottom - r.top {
                return None;
            }

            let hinst = GetModuleHandleW(null());
            let class = wide("PilGostergesi");
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: hinst,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassExW(&wc); // Explorer yeniden başlayınca tekrar çağrılır; "zaten var" önemsiz

            let title = wide("Pil Göstergesi");
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, class.as_ptr(), title.as_ptr(),
                WS_CHILD, 0, 0, 0, 0, taskbar, null_mut(), hinst, null(),
            );
            if hwnd.is_null() {
                return None;
            }
            let owner = CreateWindowExW(
                WS_EX_TOOLWINDOW, class.as_ptr(), title.as_ptr(), WS_POPUP,
                0, 0, 0, 0, null_mut(), null_mut(), hinst, null(),
            );
            Some(Strip { hwnd, owner, taskbar, tray, items: Vec::new(), drawn: None })
        }
    }

    /// Explorer yeniden başlarsa taskbar ve onunla birlikte şerit yok olur.
    pub fn alive(&self) -> bool {
        unsafe { IsWindow(self.hwnd) != 0 && IsWindow(self.tray) != 0 }
    }

    pub fn set(&mut self, items: &[Shown]) {
        self.items = items.to_vec();
        self.draw(self.env());
    }

    /// Taskbar konumu/boyutu, simge alanı genişliği, DPI veya tema değiştiyse yeniden çizer.
    pub fn tick(&mut self) {
        let env = self.env();
        if self.drawn != Some(env) {
            self.draw(env);
        }
    }

    fn env(&self) -> Env {
        let taskbar = window_rect(self.taskbar);
        Env {
            taskbar_left: taskbar.left,
            taskbar_height: taskbar.bottom - taskbar.top,
            tray_left: window_rect(self.tray).left,
            dpi: unsafe { GetDpiForWindow(self.taskbar) }.max(96),
            light: light_taskbar(),
        }
    }

    fn draw(&mut self, env: Env) {
        self.drawn = Some(env);
        if self.items.is_empty() {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            return;
        }
        let scale = |v: i32| (v * env.dpi as i32 + 48) / 96;
        let height = env.taskbar_height;

        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            let icon_font = icon_font(dc, scale(ICON_PT));
            let text_font = font(scale(TEXT_PT), "Segoe UI");

            // Yerleşim: [ikon] [yazı]   [ikon] [yazı] …
            let mut parts: Vec<Part> = Vec::new();
            let mut x = scale(EDGE);
            for (i, item) in self.items.iter().enumerate() {
                if i > 0 {
                    x += scale(ITEM_GAP);
                }
                let (icon_rgb, text_rgb) = colors(item.tone, env.light);
                for (font, text, rgb, gap) in [
                    (icon_font, glyph(item.kind), icon_rgb, scale(ICON_GAP)),
                    (text_font, item.text.as_str(), text_rgb, 0),
                ] {
                    let text: Vec<u16> = text.encode_utf16().collect();
                    let size = measure(dc, font, &text);
                    parts.push(Part { font, text, rgb, x, size });
                    x += size.cx + gap;
                }
            }
            let width = x + scale(EDGE);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // eksi: satırlar yukarıdan aşağı
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                },
                ..std::mem::zeroed()
            };
            let mut bits: *mut c_void = null_mut();
            let bitmap = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if !bitmap.is_null() && !bits.is_null() {
                let old_bitmap = SelectObject(dc, bitmap);
                let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
                paint(dc, pixels, height, &parts);

                let x = place_x(env.taskbar_left, env.tray_left, width, scale(TRAY_GAP));
                SetWindowPos(self.hwnd, HWND_TOP, x, 0, width, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let size = SIZE { cx: width, cy: height };
                let origin = POINT { x: 0, y: 0 };
                UpdateLayeredWindow(self.hwnd, null_mut(), null(), &size, dc, &origin, 0, &blend, ULW_ALPHA);

                SelectObject(dc, old_bitmap);
                DeleteObject(bitmap);
            }
            DeleteObject(icon_font);
            DeleteObject(text_font);
            DeleteDC(dc);
        }
    }

    /// Sağ tık menüsünü imlecin yanında açar ve seçilen komutu döndürür.
    pub fn menu(&self, autostart_on: bool) -> Option<Command> {
        unsafe {
            let m = CreatePopupMenu();
            let check = if autostart_on { MF_CHECKED } else { MF_UNCHECKED };
            AppendMenuW(m, MF_STRING, Command::Refresh as usize, wide("Yenile").as_ptr());
            AppendMenuW(m, MF_STRING | check, Command::ToggleAutostart as usize, wide("Açılışta başlat").as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, null());
            AppendMenuW(m, MF_STRING, Command::Quit as usize, wide("Çıkış").as_ptr());
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            // Menünün dışına tıklanınca kapanması için sahibinin önde olması gerekir.
            SetForegroundWindow(self.owner);
            let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN;
            let id = TrackPopupMenu(m, flags, pt.x, pt.y, 0, self.owner, null());
            PostMessageW(self.owner, WM_NULL, 0, 0);
            DestroyMenu(m);
            [Command::Refresh, Command::ToggleAutostart, Command::Quit]
                .into_iter()
                .find(|&c| c as i32 == id)
        }
    }
}

impl Drop for Strip {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
            DestroyWindow(self.owner);
        }
    }
}

/// Parçaları sırayla beyaz çizip kapsamayı kendi renkleriyle `pixels`'e (ön-çarpımlı BGRA) aktarır.
unsafe fn paint(dc: HDC, pixels: &mut [u8], height: i32, parts: &[Part]) {
    let mut out = vec![0u8; pixels.len()];
    SetBkMode(dc, TRANSPARENT as _);
    SetTextColor(dc, 0x00FF_FFFF);
    for Part { font, text, rgb, x, size } in parts {
        pixels.fill(0);
        let old = SelectObject(dc, *font);
        TextOutW(dc, *x, (height - size.cy) / 2, text.as_ptr(), text.len() as i32);
        SelectObject(dc, old);
        GdiFlush();
        for (src, dst) in pixels.as_chunks::<4>().0.iter().zip(out.as_chunks_mut::<4>().0) {
            let a = src[1]; // gri tonlu yumuşatma: her kanal kapsamayı taşır
            if a > dst[3] {
                let mul = |c: u8| (c as u16 * a as u16 / 255) as u8;
                *dst = [mul(rgb[2]), mul(rgb[1]), mul(rgb[0]), a];
            }
        }
    }
    // Tamamen saydam pikseller tıklamayı alttakine geçirir; şeridin her yeri sağ tıklanabilsin.
    for p in out.as_chunks_mut::<4>().0 {
        p[3] = p[3].max(1);
    }
    pixels.copy_from_slice(&out);
}

unsafe fn font(px: i32, face: &str) -> HFONT {
    CreateFontW(
        -px, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32, ANTIALIASED_QUALITY as u32, (DEFAULT_PITCH | FF_DONTCARE) as u32,
        wide(face).as_ptr(),
    )
}

/// Windows 11'de Segoe Fluent Icons; yoksa (Windows 10) aynı kod noktalarına sahip Segoe MDL2 Assets.
unsafe fn icon_font(dc: HDC, px: i32) -> HFONT {
    const FLUENT: &str = "Segoe Fluent Icons";
    let f = font(px, FLUENT);
    let old = SelectObject(dc, f);
    let mut name = [0u16; 32];
    let n = GetTextFaceW(dc, name.len() as i32, name.as_mut_ptr()).max(1) as usize - 1;
    SelectObject(dc, old);
    if String::from_utf16_lossy(&name[..n]) == FLUENT {
        return f;
    }
    DeleteObject(f);
    font(px, "Segoe MDL2 Assets")
}

unsafe fn measure(dc: HDC, font: HFONT, text: &[u16]) -> SIZE {
    let mut size = SIZE { cx: 0, cy: 0 };
    let old = SelectObject(dc, font);
    GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
    SelectObject(dc, old);
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_colors() {
        assert_eq!(colors(Tone::Normal, false), (DARK_TASKBAR_FG, DARK_TASKBAR_FG));
        assert_eq!(colors(Tone::Normal, true), (LIGHT_TASKBAR_FG, LIGHT_TASKBAR_FG));
        assert_eq!(colors(Tone::Low, false), (DARK_TASKBAR_FG, RED));
        assert_eq!(colors(Tone::Charging, true), (LIGHT_TASKBAR_FG, GREEN));
        assert_eq!(colors(Tone::Inactive, false), (GRAY, GRAY));
    }

    #[test]
    fn placed_left_of_tray() {
        // taskbar 0'dan, ^ oku 2229'dan başlıyor; şerit 150 px, 2 px boşluk
        assert_eq!(place_x(0, 2229, 150, 2), 2077);
        // sola uzanan ikinci monitör: negatif başlangıç
        assert_eq!(place_x(-864, 0, 100, 0), 764);
    }
}
