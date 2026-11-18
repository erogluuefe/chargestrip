//! Windows taskbar şeridi: `Shell_TrayWnd`'nin içine, `TrayNotifyWnd`'nin (okun) hemen soluna
//! yerleşen saydam (layered) bir alt pencere. İçerik: Segoe Fluent Icons kulaklık/fare ikonu +
//! Segoe UI yüzde. Katmanlı alt pencere için uygulama bildirimi (manifest) Windows 8+ uyumluluğu
//! bildirmeli; bunu build.rs ekler.

use crate::model::{Rgb, Shown, WHITE};
use crate::poller::Update;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Şeride sağ tıklanınca ana döngüye giden iş parçacığı mesajı.
pub const WM_APP_MENU: u32 = WM_APP + 1;

pub const ID_REFRESH: u32 = 1;
pub const ID_AUTOSTART: u32 = 2;
pub const ID_QUIT: u32 = 3;

const GLYPH_HEADSET: &str = "\u{E7F6}";
const GLYPH_MOUSE: &str = "\u{E962}";
const LIGHT_FG: Rgb = Rgb(0x1A, 0x1A, 0x1A);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// "75" → "75%", "–" olduğu gibi.
pub fn label(text: &str) -> String {
    if text.chars().all(|c| c.is_ascii_digit()) && !text.is_empty() {
        format!("{text}%")
    } else {
        text.to_string()
    }
}

/// Normal (beyaz) renk açık temada koyuya çevrilir; kırmızı/yeşil/gri aynı kalır.
pub fn themed(color: Rgb, light: bool) -> Rgb {
    if light && color == WHITE { LIGHT_FG } else { color }
}

/// Şeridin taskbar içindeki x konumu: okun solunda, `gap` kadar boşlukla.
pub fn place_x(taskbar_left: i32, notify_left: i32, width: i32, gap: i32) -> i32 {
    notify_left - taskbar_left - width - gap
}

fn light_theme() -> bool {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let val = wide("SystemUsesLightTheme");
    let mut data: u32 = 0;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER, key.as_ptr(), val.as_ptr(), RRF_RT_REG_DWORD,
            null_mut(), &mut data as *mut u32 as *mut c_void, &mut size,
        )
    };
    r == 0 && data == 1
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

fn register_class(name: &[u16]) {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(null()),
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: name.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc); // ikinci kez çağrılırsa "zaten var" hatası önemsiz
    }
}

fn rect(hwnd: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

fn find_taskbar() -> Option<(HWND, HWND)> {
    unsafe {
        let tb = FindWindowW(wide("Shell_TrayWnd").as_ptr(), null());
        if tb.is_null() {
            return None;
        }
        let notify = FindWindowExW(tb, null_mut(), wide("TrayNotifyWnd").as_ptr(), null());
        if notify.is_null() {
            return None;
        }
        let r = rect(tb);
        // dikey taskbar desteklenmiyor → tepsi simgelerine dönülür
        if r.right - r.left < r.bottom - r.top {
            return None;
        }
        Some((tb, notify))
    }
}

struct Fonts {
    icon: HFONT,
    text: HFONT,
}

impl Fonts {
    fn new(dc: HDC, dpi: u32) -> Fonts {
        let px = |n: i32| -((n * dpi as i32 + 48) / 96);
        let make = |size: i32, face: &str| unsafe {
            CreateFontW(
                px(size), 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, ANTIALIASED_QUALITY as u32,
                (DEFAULT_PITCH | FF_DONTCARE) as u32, wide(face).as_ptr(),
            )
        };
        // Windows 11: Segoe Fluent Icons; yoksa (Windows 10) Segoe MDL2 Assets — aynı kod noktaları
        let mut icon = make(16, "Segoe Fluent Icons");
        unsafe {
            let old = SelectObject(dc, icon as HGDIOBJ);
            let mut buf = [0u16; 64];
            let n = GetTextFaceW(dc, buf.len() as i32, buf.as_mut_ptr());
            SelectObject(dc, old);
            if String::from_utf16_lossy(&buf[..n.max(1) as usize - 1]) != "Segoe Fluent Icons" {
                DeleteObject(icon as HGDIOBJ);
                icon = make(16, "Segoe MDL2 Assets");
            }
        }
        Fonts { icon, text: make(12, "Segoe UI") }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.icon as HGDIOBJ);
            DeleteObject(self.text as HGDIOBJ);
        }
    }
}

fn extent(dc: HDC, font: HFONT, s: &str) -> SIZE {
    let w: Vec<u16> = s.encode_utf16().collect();
    let mut sz = SIZE { cx: 0, cy: 0 };
    unsafe {
        let old = SelectObject(dc, font as HGDIOBJ);
        GetTextExtentPoint32W(dc, w.as_ptr(), w.len() as i32, &mut sz);
        SelectObject(dc, old);
    }
    sz
}

pub struct Widget {
    hwnd: HWND,
    owner: HWND,
    taskbar: HWND,
    notify: HWND,
    last: Option<Update>,
    // son çizimin anahtarı: bunlardan biri değişince yeniden çizilir/taşınır
    key: (i32, i32, u32, bool, i32),
}

impl Widget {
    pub fn create() -> Option<Widget> {
        let (taskbar, notify) = find_taskbar()?;
        let class = wide("PilGostergesiSerit");
        let owner_class = wide("PilGostergesiSahip");
        register_class(&class);
        register_class(&owner_class);
        unsafe {
            let hinst = GetModuleHandleW(null());
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(), wide("Pil Göstergesi").as_ptr(), WS_CHILD,
                0, 0, 1, 1, taskbar, null_mut(), hinst, null(),
            );
            if hwnd.is_null() {
                return None;
            }
            // Menünün sahibi: görünmez, üst düzey kendi penceremiz
            let owner = CreateWindowExW(
                WS_EX_TOOLWINDOW, owner_class.as_ptr(), wide("Pil Göstergesi").as_ptr(), WS_POPUP,
                0, 0, 0, 0, null_mut(), null_mut(), hinst, null(),
            );
            let mut w = Widget { hwnd, owner, taskbar, notify, last: None, key: (0, 0, 0, false, 0) };
            w.draw();
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Some(w)
        }
    }

    /// Explorer yeniden başlarsa taskbar ve şerit yok olur.
    pub fn alive(&self) -> bool {
        unsafe { IsWindow(self.hwnd) != 0 && IsWindow(self.taskbar) != 0 && IsWindow(self.notify) != 0 }
    }

    pub fn set(&mut self, u: Update) {
        self.last = Some(u);
        self.draw();
    }

    /// Zamanlayıcıdan: konum, boyut, DPI veya tema değiştiyse yeniden çizer.
    pub fn tick(&mut self) {
        let (tb, nt) = (rect(self.taskbar), rect(self.notify));
        let dpi = unsafe { GetDpiForWindow(self.taskbar) };
        let key = (nt.left - tb.left, tb.bottom - tb.top, dpi, light_theme(), tb.right - tb.left);
        if key != self.key {
            self.draw();
        }
    }

    fn items(&self) -> [(&'static str, String, Rgb); 2] {
        let pick = |s: Option<&Shown>| match s {
            Some(s) => (label(&s.text), s.color),
            None => ("–".to_string(), crate::model::GRAY),
        };
        let (ht, hc) = pick(self.last.as_ref().map(|u| &u.headset));
        let (mt, mc) = pick(self.last.as_ref().map(|u| &u.mouse));
        [(GLYPH_HEADSET, ht, hc), (GLYPH_MOUSE, mt, mc)]
    }

    fn draw(&mut self) {
        let (tb, nt) = (rect(self.taskbar), rect(self.notify));
        let dpi = unsafe { GetDpiForWindow(self.taskbar) }.max(96);
        let light = light_theme();
        self.key = (nt.left - tb.left, tb.bottom - tb.top, dpi, light, tb.right - tb.left);
        let s = |n: i32| (n * dpi as i32 + 48) / 96;
        let height = tb.bottom - tb.top;
        let fg = themed(WHITE, light);
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            let fonts = Fonts::new(dc, dpi);

            // Yerleşim: [ikon] boşluk [yazı]   [ikon] boşluk [yazı]
            let mut parts: Vec<(i32, HFONT, String, Rgb, SIZE)> = Vec::new();
            let mut x = s(6);
            for (i, (glyph, text, color)) in self.items().into_iter().enumerate() {
                if i > 0 {
                    x += s(10);
                }
                let gs = extent(dc, fonts.icon, glyph);
                parts.push((x, fonts.icon, glyph.to_string(), fg, gs));
                x += gs.cx + s(4);
                let ts = extent(dc, fonts.text, &text);
                parts.push((x, fonts.text, text, themed(color, light), ts));
                x += ts.cx;
            }
            let width = x + s(6);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // yukarıdan aşağı
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB as u32,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
            };
            let mut bits: *mut c_void = null_mut();
            let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bmp.is_null() || bits.is_null() {
                DeleteDC(dc);
                return;
            }
            let old_bmp = SelectObject(dc, bmp as HGDIOBJ);
            let n = (width * height) as usize;
            let px = std::slice::from_raw_parts_mut(bits as *mut u8, n * 4);
            let mut out = vec![0u8; n * 4];

            // Her parçayı beyaz maske olarak çiz, kapsamayı kendi rengiyle ön-çarpımlı BGRA'ya aktar.
            SetBkMode(dc, TRANSPARENT as _);
            SetTextColor(dc, 0x00FF_FFFF);
            for (x, font, text, color, size) in &parts {
                px.fill(0);
                let w: Vec<u16> = text.encode_utf16().collect();
                let old = SelectObject(dc, *font as HGDIOBJ);
                TextOutW(dc, *x, (height - size.cy) / 2, w.as_ptr(), w.len() as i32);
                SelectObject(dc, old);
                GdiFlush();
                for i in 0..n {
                    let a = px[i * 4 + 1] as u32; // yeşil kanal = kapsama
                    if a > out[i * 4 + 3] as u32 {
                        out[i * 4] = (color.2 as u32 * a / 255) as u8;
                        out[i * 4 + 1] = (color.1 as u32 * a / 255) as u8;
                        out[i * 4 + 2] = (color.0 as u32 * a / 255) as u8;
                        out[i * 4 + 3] = a as u8;
                    }
                }
            }
            // Tamamen saydam pikseller tıklamayı alttakine geçirir; tüm şerit sağ tıklanabilsin.
            for p in out.chunks_mut(4) {
                if p[3] == 0 {
                    p[3] = 1;
                }
            }
            px.copy_from_slice(&out);

            let x = place_x(tb.left, nt.left, width, s(2));
            SetWindowPos(self.hwnd, HWND_TOP, x, 0, width, height, SWP_NOACTIVATE);
            let size = SIZE { cx: width, cy: height };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(self.hwnd, null_mut(), null(), &size, dc, &src, 0, &blend, ULW_ALPHA);

            SelectObject(dc, old_bmp);
            DeleteObject(bmp as HGDIOBJ);
            drop(fonts);
            DeleteDC(dc);
        }
    }

    /// Sağ tık menüsünü imlecin yanında açar, seçilen komutun kimliğini döndürür (0 = iptal).
    pub fn menu(&self, autostart_on: bool) -> u32 {
        unsafe {
            let m = CreatePopupMenu();
            AppendMenuW(m, MF_STRING, ID_REFRESH as usize, wide("Yenile").as_ptr());
            let check = if autostart_on { MF_CHECKED } else { MF_UNCHECKED };
            AppendMenuW(m, MF_STRING | check, ID_AUTOSTART as usize, wide("Açılışta başlat").as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, null());
            AppendMenuW(m, MF_STRING, ID_QUIT as usize, wide("Çıkış").as_ptr());
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(self.owner);
            let cmd = TrackPopupMenu(
                m, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                pt.x, pt.y, 0, self.owner, null(),
            );
            PostMessageW(self.owner, WM_NULL, 0, 0);
            DestroyMenu(m);
            cmd as u32
        }
    }
}

impl Drop for Widget {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
            DestroyWindow(self.owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GRAY, RED};

    #[test]
    fn label_adds_percent_to_numbers_only() {
        assert_eq!(label("75"), "75%");
        assert_eq!(label("100"), "100%");
        assert_eq!(label("–"), "–");
        assert_eq!(label(""), "");
    }

    #[test]
    fn white_becomes_dark_on_light_theme() {
        assert_eq!(themed(WHITE, true), LIGHT_FG);
        assert_eq!(themed(WHITE, false), WHITE);
        assert_eq!(themed(RED, true), RED);
        assert_eq!(themed(GRAY, true), GRAY);
    }

    #[test]
    fn placed_left_of_tray() {
        // taskbar 0'dan, ok 2229'dan başlıyor, şerit 150 geniş, 2 px boşluk
        assert_eq!(place_x(0, 2229, 150, 2), 2077);
        // ikinci monitörde negatif başlangıç
        assert_eq!(place_x(-864, 0, 100, 0), 764);
    }
}
