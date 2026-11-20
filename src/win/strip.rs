//! Taskbar strip: a transparent child window inside a taskbar that shows, for each device, a
//! Segoe Fluent Icons glyph and the percentage in Segoe UI. Being a child of the taskbar, it hides
//! with it in full-screen games. Every taskbar gets one (one per monitor):
//!
//! - **Main taskbar** (`Shell_TrayWnd`): just left of the notification area (`TrayNotifyWnd`, the
//!   ^ arrow); on a vertical taskbar, stacked just above it.
//! - **Other monitors** (`Shell_SecondaryTrayWnd`): they have no notification area to anchor to.
//!   Windows 11 draws its clock at the right end with XAML, so the strip goes to the empty left
//!   end (or right after the taskbar buttons when they are left-aligned). Windows 10 has nothing
//!   at the right end, so it goes there.
//!
//! Drawing: each part is drawn in white into a DIB, the text's anti-aliasing is read back as
//! coverage (alpha) and written in the part's own color as premultiplied BGRA, then shown with
//! `UpdateLayeredWindow`. A layered *child* window requires the app to declare Windows 8+
//! compatibility (src/win/app.manifest).
//!
//! Hovering over a device shows a tooltip with its name and state; each device is its own tool.

use super::{reg_dword, wide};
use crate::model::{Kind, Shown, Tone};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Controls::{
    TOOLTIPS_CLASSW, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_DELTOOLW, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX,
    TTTOOLINFOW,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Thread message posted to the main loop when a strip is clicked (left or right).
pub const WM_APP_MENU: u32 = WM_APP + 1;

/// Command picked from the menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Refresh = 1,
    ToggleAutostart = 2,
    Quit = 3,
    Update = 4,
    CheckUpdate = 5,
    CopyDiagnostics = 6,
}

type Rgb = [u8; 3];
const RED: Rgb = [0xE5, 0x39, 0x35];
const GREEN: Rgb = [0x43, 0xA0, 0x47];
const GRAY: Rgb = [0x9E, 0x9E, 0x9E];
const DARK_TASKBAR_FG: Rgb = [0xFF, 0xFF, 0xFF];
const LIGHT_TASKBAR_FG: Rgb = [0x1A, 0x1A, 0x1A];

/// Sizes at 96 DPI; scaled to the taskbar's DPI when drawing.
const ICON_PT: i32 = 16;
const TEXT_PT: i32 = 12;
const EDGE: i32 = 6; // padding around the strip's content
const ICON_GAP: i32 = 4; // between icon and text
const ITEM_GAP: i32 = 10; // between two devices (horizontal)
const ROW: i32 = 26; // row height per device (vertical taskbar)
const TRAY_GAP: i32 = 2; // between the strip and the ^ arrow / the taskbar buttons

/// Segoe Fluent Icons / Segoe MDL2 Assets code point.
fn glyph(kind: Kind) -> &'static str {
    match kind {
        Kind::Headset => "\u{E7F6}",
        Kind::Mouse => "\u{E962}",
        Kind::Keyboard => "\u{E765}",
        Kind::Gamepad => "\u{E7FC}",
        Kind::Other => "\u{E702}", // Bluetooth symbol
    }
}

/// Dimmed empty battery (Battery0) shown when nothing is connected, so the strip never vanishes
/// and the app, with its menu, stays reachable.
const NO_DEVICE_GLYPH: &str = "\u{E850}";

/// (icon color, text color). A device that is off or asleep is dimmed entirely.
fn colors(tone: Tone, light_taskbar: bool) -> (Rgb, Rgb) {
    let fg = if light_taskbar { LIGHT_TASKBAR_FG } else { DARK_TASKBAR_FG };
    match tone {
        Tone::Normal => (fg, fg),
        Tone::Low => (fg, RED),
        Tone::Charging => (fg, GREEN),
        Tone::Inactive => (GRAY, GRAY),
    }
}

fn light_taskbar() -> bool {
    reg_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "SystemUsesLightTheme") == Some(1)
}

/// Windows 11 taskbar buttons are centered unless "Taskbar alignment" is set to left.
fn buttons_left_aligned() -> bool {
    reg_dword(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "TaskbarAl") == Some(0)
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

fn child(parent: HWND, class: &str) -> HWND {
    unsafe { FindWindowExW(parent, null_mut(), wide(class).as_ptr(), null()) }
}

/// All taskbars, main one first.
pub fn taskbars() -> Vec<HWND> {
    let mut found = Vec::new();
    unsafe {
        let main = FindWindowW(wide("Shell_TrayWnd").as_ptr(), null());
        if !main.is_null() {
            found.push(main);
        }
        let class = wide("Shell_SecondaryTrayWnd");
        let mut prev = null_mut();
        loop {
            prev = FindWindowExW(null_mut(), prev, class.as_ptr(), null());
            if prev.is_null() {
                break;
            }
            found.push(prev);
        }
    }
    found
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONUP | WM_RBUTTONUP => {
            PostThreadMessageW(GetCurrentThreadId(), WM_APP_MENU, 0, 0);
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Registers the window class shared by the strips and the menu owner (repeat calls are harmless).
unsafe fn register_class() -> Vec<u16> {
    let class = wide("Chargestrip");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: GetModuleHandleW(null()),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    RegisterClassExW(&wc);
    class
}

/// Where the strip goes, given its size; in taskbar coordinates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anchor {
    /// Horizontal taskbar: the strip ends at this x.
    End(i32),
    /// Horizontal taskbar: the strip starts at this x.
    Start(i32),
    /// Vertical taskbar: the strip ends at this y.
    Bottom(i32),
}

/// Outside conditions that require a redraw; `tick` redraws when they change.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Env {
    width: i32,
    height: i32,
    anchor: Anchor,
    dpi: u32,
    light: bool,
}

/// One part of the strip (icon or text) and where it goes.
struct Part {
    font: HFONT,
    text: Vec<u16>,
    rgb: Rgb,
    x: i32,
    y: i32,
}

/// Lays out the entries: the (x, y) of each entry's icon and text in turn, each entry's rectangle
/// (for its tooltip) and the strip's size. `sizes[i]` are the measured (icon, text) sizes of entry
/// `i`; `bar` is the taskbar's (width, height).
fn layout(sizes: &[(SIZE, SIZE)], vertical: bool, bar: (i32, i32), scale: impl Fn(i32) -> i32) -> (Vec<(i32, i32)>, Vec<RECT>, SIZE) {
    let (bar_w, bar_h) = bar;
    let mut origins = Vec::new();
    let mut rects = Vec::new();
    if vertical {
        let row = scale(ROW);
        let height = scale(EDGE) * 2 + row * sizes.len() as i32;
        for (i, (icon, text)) in sizes.iter().enumerate() {
            let top = scale(EDGE) + row * i as i32;
            let w = icon.cx + scale(ICON_GAP) + text.cx;
            let x = (bar_w - w).max(0) / 2;
            origins.push((x, top + (row - icon.cy) / 2));
            origins.push((x + icon.cx + scale(ICON_GAP), top + (row - text.cy) / 2));
            rects.push(RECT { left: 0, top, right: bar_w, bottom: top + row });
        }
        (origins, rects, SIZE { cx: bar_w, cy: height })
    } else {
        let half_gap = scale(ITEM_GAP) / 2;
        let mut x = scale(EDGE);
        for (i, (icon, text)) in sizes.iter().enumerate() {
            if i > 0 {
                x += scale(ITEM_GAP);
            }
            let start = x;
            origins.push((x, (bar_h - icon.cy) / 2));
            x += icon.cx + scale(ICON_GAP);
            origins.push((x, (bar_h - text.cy) / 2));
            x += text.cx;
            rects.push(RECT { left: start - half_gap.min(start), top: 0, right: x + half_gap, bottom: bar_h });
        }
        (origins, rects, SIZE { cx: x + scale(EDGE), cy: bar_h })
    }
}

/// The strip's top-left corner inside the taskbar.
fn place(anchor: Anchor, size: SIZE) -> POINT {
    match anchor {
        Anchor::End(x) => POINT { x: x - size.cx, y: 0 },
        Anchor::Start(x) => POINT { x, y: 0 },
        Anchor::Bottom(y) => POINT { x: 0, y: y - size.cy },
    }
}

pub struct Strip {
    hwnd: HWND,
    tip: HWND,
    taskbar: HWND,
    /// Notification area; only the main taskbar has one.
    tray: HWND,
    /// Windows 11 taskbar (XAML content).
    xaml: bool,
    items: Vec<Shown>,
    drawn: Option<Env>,
    /// Number of tooltip tools currently registered.
    tools: usize,
}

impl Strip {
    /// A strip inside the given taskbar; `None` if the window can't be created.
    pub fn create(taskbar: HWND) -> Option<Strip> {
        unsafe {
            let tray = child(taskbar, "TrayNotifyWnd");
            let xaml = !child(taskbar, "Windows.UI.Composition.DesktopWindowContentBridge").is_null();
            let class = register_class();
            let hinst = GetModuleHandleW(null());
            let title = wide("Chargestrip");
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, class.as_ptr(), title.as_ptr(),
                WS_CHILD, 0, 0, 0, 0, taskbar, null_mut(), hinst, null(),
            );
            if hwnd.is_null() {
                return None;
            }
            let tip = CreateWindowExW(
                WS_EX_TOPMOST, TOOLTIPS_CLASSW, null(), WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
                CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, hwnd, null_mut(), hinst, null(),
            );
            if !tip.is_null() {
                SendMessageW(tip, TTM_SETMAXTIPWIDTH, 0, 800);
            }
            Some(Strip { hwnd, tip, taskbar, tray, xaml, items: Vec::new(), drawn: None, tools: 0 })
        }
    }

    pub fn taskbar(&self) -> HWND {
        self.taskbar
    }

    /// When Explorer restarts, the taskbars and the strips with them are gone.
    pub fn alive(&self) -> bool {
        unsafe {
            IsWindow(self.hwnd) != 0 && IsWindow(self.taskbar) != 0 && (self.tray.is_null() || IsWindow(self.tray) != 0)
        }
    }

    pub fn set(&mut self, items: &[Shown]) {
        self.items = items.to_vec();
        self.draw(self.env());
    }

    /// Redraws if the taskbar's position/size, the anchor, the DPI or the theme changed.
    pub fn tick(&mut self) {
        let env = self.env();
        if self.drawn != Some(env) {
            self.draw(env);
        }
    }

    fn env(&self) -> Env {
        let bar = window_rect(self.taskbar);
        let (width, height) = (bar.right - bar.left, bar.bottom - bar.top);
        let dpi = unsafe { GetDpiForWindow(self.taskbar) }.max(96);
        let scale = |v: i32| (v * dpi as i32 + 48) / 96;
        let vertical = height > width;
        let anchor = if !self.tray.is_null() {
            let tray = window_rect(self.tray);
            if vertical {
                Anchor::Bottom(tray.top - bar.top - scale(TRAY_GAP))
            } else {
                Anchor::End(tray.left - bar.left - scale(TRAY_GAP))
            }
        } else if vertical {
            Anchor::Bottom(height - scale(EDGE))
        } else if !self.xaml {
            Anchor::End(width - scale(EDGE)) // Windows 10: nothing at the right end
        } else if buttons_left_aligned() {
            let buttons = child(self.taskbar, "WorkerW");
            let end = if buttons.is_null() { 0 } else { window_rect(buttons).right - bar.left };
            Anchor::Start(end.max(0) + scale(TRAY_GAP))
        } else {
            Anchor::Start(scale(EDGE)) // Windows 11, centered buttons: the left end is free
        };
        Env { width, height, anchor, dpi, light: light_taskbar() }
    }

    fn draw(&mut self, env: Env) {
        self.drawn = Some(env);
        let none_tip = crate::i18n::t().tip_none;
        let entries: Vec<(&str, &str, Tone, &str)> = if self.items.is_empty() {
            vec![(NO_DEVICE_GLYPH, "–", Tone::Inactive, none_tip)]
        } else {
            self.items.iter().map(|s| (glyph(s.kind), s.text.as_str(), s.tone, s.tip.as_str())).collect()
        };
        let scale = |v: i32| (v * env.dpi as i32 + 48) / 96;
        let vertical = env.height > env.width;

        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            let icon_font = icon_font(dc, scale(ICON_PT));
            let text_font = font(scale(TEXT_PT), "Segoe UI");

            let texts: Vec<(Vec<u16>, Vec<u16>)> = entries
                .iter()
                .map(|(icon, label, ..)| (icon.encode_utf16().collect(), label.encode_utf16().collect()))
                .collect();
            let sizes: Vec<(SIZE, SIZE)> =
                texts.iter().map(|(icon, label)| (measure(dc, icon_font, icon), measure(dc, text_font, label))).collect();
            let (origins, rects, size) = layout(&sizes, vertical, (env.width, env.height), scale);

            let mut parts = Vec::new();
            for (i, ((icon, label), &(_, _, tone, _))) in texts.into_iter().zip(&entries).enumerate() {
                let (icon_rgb, text_rgb) = colors(tone, env.light);
                let (ix, iy) = origins[2 * i];
                let (tx, ty) = origins[2 * i + 1];
                parts.push(Part { font: icon_font, text: icon, rgb: icon_rgb, x: ix, y: iy });
                parts.push(Part { font: text_font, text: label, rgb: text_rgb, x: tx, y: ty });
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size.cx,
                    biHeight: -size.cy, // negative: rows top-down
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
                let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (size.cx * size.cy * 4) as usize);
                paint(dc, pixels, &parts);

                let at = place(env.anchor, size);
                SetWindowPos(self.hwnd, HWND_TOP, at.x, at.y, size.cx, size.cy, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let origin = POINT { x: 0, y: 0 };
                UpdateLayeredWindow(self.hwnd, null_mut(), null(), &size, dc, &origin, 0, &blend, ULW_ALPHA);

                SelectObject(dc, old_bitmap);
                DeleteObject(bitmap);
            }
            DeleteObject(icon_font);
            DeleteObject(text_font);
            DeleteDC(dc);

            let tips: Vec<String> = entries.iter().map(|e| e.3.to_string()).collect();
            self.update_tooltips(&rects, &tips);
        }
    }

    /// One tooltip tool per entry, covering its part of the strip.
    unsafe fn update_tooltips(&mut self, rects: &[RECT], tips: &[String]) {
        if self.tip.is_null() {
            return;
        }
        let hwnd = self.hwnd;
        let tool = |id: usize, rect: RECT, text: *mut u16| TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_SUBCLASS,
            hwnd,
            uId: id,
            rect,
            lpszText: text,
            ..Default::default()
        };
        for id in 0..self.tools {
            let info = tool(id, RECT { left: 0, top: 0, right: 0, bottom: 0 }, null_mut());
            SendMessageW(self.tip, TTM_DELTOOLW, 0, &info as *const _ as LPARAM);
        }
        for (id, (rect, text)) in rects.iter().zip(tips).enumerate() {
            let mut text = wide(text); // the control keeps its own copy
            let info = tool(id, *rect, text.as_mut_ptr());
            SendMessageW(self.tip, TTM_ADDTOOLW, 0, &info as *const _ as LPARAM);
        }
        self.tools = rects.len();
    }
}

impl Drop for Strip {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.tip);
            DestroyWindow(self.hwnd);
        }
    }
}

/// Our own invisible top-level window that owns the menu and the message boxes (a menu needs a
/// window in the foreground).
pub struct Owner(HWND);

impl Owner {
    pub fn create() -> Owner {
        unsafe {
            let class = register_class();
            let title = wide("Chargestrip");
            Owner(CreateWindowExW(
                WS_EX_TOOLWINDOW, class.as_ptr(), title.as_ptr(), WS_POPUP,
                0, 0, 0, 0, null_mut(), null_mut(), GetModuleHandleW(null()), null(),
            ))
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.0
    }

    /// Opens the menu next to the cursor and returns the picked command. `update`: the number of a
    /// published new version, if any; adds "Update" at the top of the menu.
    pub fn menu(&self, autostart_on: bool, update: Option<&str>) -> Option<Command> {
        unsafe {
            let m = CreatePopupMenu();
            let t = crate::i18n::t();
            let check = if autostart_on { MF_CHECKED } else { MF_UNCHECKED };
            if let Some(version) = update {
                let label = wide(&crate::i18n::fill(t.update_to, version));
                AppendMenuW(m, MF_STRING, Command::Update as usize, label.as_ptr());
                AppendMenuW(m, MF_SEPARATOR, 0, null());
            }
            AppendMenuW(m, MF_STRING, Command::Refresh as usize, wide(t.refresh).as_ptr());
            AppendMenuW(m, MF_STRING | check, Command::ToggleAutostart as usize, wide(t.autostart).as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, null());
            if update.is_none() {
                AppendMenuW(m, MF_STRING, Command::CheckUpdate as usize, wide(t.check_updates).as_ptr());
            }
            AppendMenuW(m, MF_STRING, Command::CopyDiagnostics as usize, wide(t.copy_diagnostics).as_ptr());
            let version = wide(&format!("Chargestrip {}", crate::update::CURRENT));
            AppendMenuW(m, MF_STRING | MF_GRAYED, 0, version.as_ptr()); // information only
            AppendMenuW(m, MF_STRING, Command::Quit as usize, wide(t.quit).as_ptr());
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            // The owner must be in the foreground for the menu to close when clicking outside it.
            SetForegroundWindow(self.0);
            let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN;
            let id = TrackPopupMenu(m, flags, pt.x, pt.y, 0, self.0, null());
            PostMessageW(self.0, WM_NULL, 0, 0);
            DestroyMenu(m);
            [
                Command::Refresh,
                Command::ToggleAutostart,
                Command::Quit,
                Command::Update,
                Command::CheckUpdate,
                Command::CopyDiagnostics,
            ]
            .into_iter()
            .find(|&c| c as i32 == id)
        }
    }

    /// Reports the result of something the user asked for in a small message box. With `ask`, shows
    /// Yes/No and returns whether "Yes" was chosen; otherwise there is only an OK button.
    pub fn message(&self, text: &str, ask: bool) -> bool {
        let buttons = if ask { MB_YESNO | MB_ICONQUESTION } else { MB_OK | MB_ICONINFORMATION };
        let (text, title) = (wide(text), wide("Chargestrip"));
        unsafe {
            SetForegroundWindow(self.0);
            MessageBoxW(self.0, text.as_ptr(), title.as_ptr(), buttons | MB_SETFOREGROUND) == IDYES
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

/// Draws the parts one by one in white and writes their coverage, in their own colors, to `pixels`
/// (premultiplied BGRA).
unsafe fn paint(dc: HDC, pixels: &mut [u8], parts: &[Part]) {
    let mut out = vec![0u8; pixels.len()];
    SetBkMode(dc, TRANSPARENT as _);
    SetTextColor(dc, 0x00FF_FFFF);
    for Part { font, text, rgb, x, y } in parts {
        pixels.fill(0);
        let old = SelectObject(dc, *font);
        TextOutW(dc, *x, *y, text.as_ptr(), text.len() as i32);
        SelectObject(dc, old);
        GdiFlush();
        for (src, dst) in pixels.as_chunks::<4>().0.iter().zip(out.as_chunks_mut::<4>().0) {
            let a = src[1]; // grayscale anti-aliasing: every channel carries the coverage
            if a > dst[3] {
                let mul = |c: u8| (c as u16 * a as u16 / 255) as u8;
                *dst = [mul(rgb[2]), mul(rgb[1]), mul(rgb[0]), a];
            }
        }
    }
    // Fully transparent pixels pass clicks through; keep every pixel of the strip clickable.
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

/// Segoe Fluent Icons on Windows 11; otherwise (Windows 10) Segoe MDL2 Assets, which has the same code points.
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

    fn sz(cx: i32, cy: i32) -> SIZE {
        SIZE { cx, cy }
    }

    #[test]
    fn tone_colors() {
        assert_eq!(colors(Tone::Normal, false), (DARK_TASKBAR_FG, DARK_TASKBAR_FG));
        assert_eq!(colors(Tone::Normal, true), (LIGHT_TASKBAR_FG, LIGHT_TASKBAR_FG));
        assert_eq!(colors(Tone::Low, false), (DARK_TASKBAR_FG, RED));
        assert_eq!(colors(Tone::Charging, true), (LIGHT_TASKBAR_FG, GREEN));
        assert_eq!(colors(Tone::Inactive, false), (GRAY, GRAY));
    }

    #[test]
    fn horizontal_layout() {
        // two devices at 96 DPI: icon 16x16, texts 30x16 and 26x16, in a 48 px tall taskbar
        let sizes = [(sz(16, 16), sz(30, 16)), (sz(16, 16), sz(26, 16))];
        let (origins, rects, size) = layout(&sizes, false, (2560, 48), |v| v);
        assert_eq!(origins, vec![(6, 16), (26, 16), (66, 16), (86, 16)]);
        assert_eq!(size.cx, 6 + 16 + 4 + 30 + 10 + 16 + 4 + 26 + 6);
        assert_eq!(size.cy, 48);
        assert_eq!((rects[0].left, rects[0].right, rects[1].left, rects[1].right), (1, 61, 61, 117));
    }

    #[test]
    fn vertical_layout_stacks_rows() {
        let sizes = [(sz(16, 16), sz(30, 16)), (sz(16, 16), sz(26, 16))];
        let (origins, rects, size) = layout(&sizes, true, (62, 1080), |v| v);
        assert_eq!((size.cx, size.cy), (62, 6 + 26 * 2 + 6));
        assert_eq!(origins[0], ((62 - 50) / 2, 6 + 5)); // row 1, centered
        assert_eq!((rects[1].top, rects[1].bottom), (32, 58));
    }

    #[test]
    fn placement() {
        let at = |a, cx, cy| {
            let p = place(a, sz(cx, cy));
            (p.x, p.y)
        };
        assert_eq!(at(Anchor::End(2227), 150, 48), (2077, 0)); // left of the ^ arrow
        assert_eq!(at(Anchor::Start(6), 150, 48), (6, 0));
        assert_eq!(at(Anchor::Bottom(1000), 62, 64), (0, 936));
    }
}
