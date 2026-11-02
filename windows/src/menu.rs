//! The tray menu on Windows 11: panels of Acrylic, the material of the system's own menus and flyouts, with the entries
//! drawn on it by Direct2D and submenus opening beside them. It runs like `TrackPopupMenuEx`: `track` returns the id
//! chosen once the menu has closed.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::Once;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory,
    ID2D1Factory, ID2D1RenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DWMWINDOWATTRIBUTE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, ClientToScreen, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, EndPaint, GetMonitorInfoW, HDC, HGDIOBJ, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, PAINTSTRUCT,
    SRCCOPY, SelectObject,
};
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, Result, s, w};
use windows_numerics::Vector2;

use crate::log;
use crate::tracker;

const CLASS: PCWSTR = w!("SnowhushMenu");
const TIMER_SUBMENU: usize = 1;
/// Looks for the pointer leaving the menu, which sends no messages once it is over other windows.
const TIMER_POINTER: usize = 2;
/// The first build of Windows 11 22H2, which has the system's backdrops.
const BACKDROP_BUILD: u32 = 22621;
/// Sizes in DIPs, after the menus of Windows 11.
const PAD: f32 = 4.0;
const ROW: f32 = 28.0;
const SEPARATOR: f32 = 7.0;
const ROW_RADIUS: f32 = 4.0;
/// The centre of the marks and the start of the labels, from the panel's left edge.
const MARK: f32 = 20.0;
const LABEL: f32 = 36.0;
/// Room after the longest label, where a submenu's chevron goes.
const TRAILING: f32 = 40.0;
const MIN_WIDTH: f32 = 160.0;
const TEXT_SIZE: f32 = 14.0;
const GLYPH_SIZE: f32 = 12.0;
/// How far a submenu reaches back over the panel it opens from.
const OVERLAP: f32 = 4.0;
/// The tint over the blur, as 0xAABBGGRR: light enough to see through, yet dark or pale enough to read on.
const TINT_DARK: u32 = 0x732C_2C2C;
const TINT_LIGHT: u32 = 0x80F3_F3F3;
const CHECK: &str = "\u{E73E}";
const CHEVRON: &str = "\u{E76C}";

/// What the menu lists.
#[derive(Clone)]
pub enum Entry {
    Item {
        id: u32,
        label: String,
        checked: bool,
        enabled: bool,
        /// One of a group of which just one is on, marked with a dot.
        radio: bool,
    },
    Submenu {
        label: String,
        entries: Vec<Entry>,
    },
    Separator,
}

impl Entry {
    pub fn item(id: u32, label: &str, checked: bool, enabled: bool) -> Entry {
        Entry::Item { id, label: label.to_string(), checked, enabled, radio: false }
    }

    pub fn choice(id: u32, label: &str, chosen: bool) -> Entry {
        Entry::Item { id, label: label.to_string(), checked: chosen, enabled: true, radio: true }
    }

    fn label(&self) -> Option<&str> {
        match self {
            Entry::Item { label, .. } | Entry::Submenu { label, .. } => Some(label),
            Entry::Separator => None,
        }
    }

    fn selectable(&self) -> bool {
        match self {
            Entry::Item { enabled, .. } => *enabled,
            Entry::Submenu { .. } => true,
            Entry::Separator => false,
        }
    }

    fn is_submenu(&self) -> bool {
        matches!(self, Entry::Submenu { .. })
    }
}

/// Whether the menu can be drawn: the system's Acrylic needs Windows 11 22H2, and with a screen reader or high
/// contrast on, the system's own menu serves better.
pub fn available() -> bool {
    tracker::windows_build() >= BACKDROP_BUILD && !screen_reader() && !high_contrast()
}

fn screen_reader() -> bool {
    let mut on = BOOL(0);
    let read = unsafe { SystemParametersInfoW(SPI_GETSCREENREADER, 0, Some((&raw mut on).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    read.is_ok() && on.as_bool()
}

fn high_contrast() -> bool {
    let mut contrast = HIGHCONTRASTW { cbSize: size_of::<HIGHCONTRASTW>() as u32, ..Default::default() };
    let read = unsafe {
        SystemParametersInfoW(SPI_GETHIGHCONTRAST, contrast.cbSize, Some((&raw mut contrast).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0))
    };
    read.is_ok() && contrast.dwFlags.contains(HCF_HIGHCONTRASTON)
}

/// The colours of Windows 11's menus on light and dark Acrylic.
#[derive(Clone, Copy)]
struct Palette {
    ink: D2D1_COLOR_F,
    dim: D2D1_COLOR_F,
    hover: D2D1_COLOR_F,
    line: D2D1_COLOR_F,
}

impl Palette {
    fn new(light: bool) -> Palette {
        let tone = if light { 0.0 } else { 1.0 };
        let color = |a: f32| D2D1_COLOR_F { r: tone, g: tone, b: tone, a };
        if light {
            Palette { ink: color(0.894), dim: color(0.361), hover: color(0.037), line: color(0.058) }
        } else {
            Palette { ink: color(1.0), dim: color(0.363), hover: color(0.061), line: color(0.084) }
        }
    }
}

struct Panel {
    window: HWND,
    entries: Vec<Entry>,
    /// Where each entry's row starts, in DIPs from the panel's top.
    tops: Vec<f32>,
    width: f32,
    height: f32,
    /// The panel on the screen, in physical pixels.
    frame: RECT,
    hover: Option<usize>,
    /// The entry of the panel before it that opened it.
    from: usize,
    pixels: Vec<u8>,
}

impl Panel {
    fn size(&self) -> (i32, i32) {
        (self.frame.right - self.frame.left, self.frame.bottom - self.frame.top)
    }
}

struct Menu {
    /// The menu, then each submenu open, each opened from an entry of the one before it.
    panels: Vec<Panel>,
    scale: f32,
    work: RECT,
    light: bool,
    palette: Palette,
    /// The panel and entry the pointer last came to rest on, while the submenu timer runs.
    resting: Option<(usize, Option<usize>)>,
    delay: u32,
    d2d: ID2D1Factory,
    imaging: IWICImagingFactory,
    write: IDWriteFactory,
    text: IDWriteTextFormat,
    glyphs: IDWriteTextFormat,
}

thread_local! {
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
    static CLOSED: Cell<bool> = const { Cell::new(false) };
    static CHOSEN: Cell<u32> = const { Cell::new(0) };
}

/// Shows the menu at `x`, `y` in screen pixels, the way the system places a menu there, runs it until it closes and
/// returns the id of the item chosen, or 0. Fails if it can't be shown, so that the system's menu can take over.
pub fn track(x: i32, y: i32, entries: Vec<Entry>, light: bool) -> Result<u32> {
    if MENU.with_borrow(Option::is_some) {
        return Ok(0);
    }
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(procedure),
            hInstance: GetModuleHandleW(None).map(Into::into).unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
    });
    CLOSED.set(false);
    CHOSEN.set(0);

    let mut menu = Menu::new(POINT { x, y }, light)?;
    let mut panel = menu.panel(entries, 0)?;
    let size = SIZE { cx: (panel.width * menu.scale).ceil() as i32, cy: (panel.height * menu.scale).ceil() as i32 };
    unsafe { CalculatePopupWindowPosition(&POINT { x, y }, &size, (TPM_LEFTALIGN | TPM_TOPALIGN | TPM_WORKAREA).0, None, &mut panel.frame)? };
    panel.window = backdrop_window(panel.frame, light, true)?;
    let root = panel.window;
    menu.panels.push(panel);
    menu.render(0);
    MENU.set(Some(menu));

    unsafe {
        show(root, true);
        let _ = SetForegroundWindow(root);
        SetCapture(root);
        SetTimer(Some(root), TIMER_POINTER, 50, None);

        while !CLOSED.get() {
            let mut message = MSG::default();
            let status = GetMessageW(&mut message, None, 0, 0);
            if status.0 <= 0 {
                if status.0 == 0 {
                    PostQuitMessage(message.wParam.0 as i32);
                }
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        let _ = KillTimer(Some(root), TIMER_SUBMENU);
        let _ = KillTimer(Some(root), TIMER_POINTER);
        let _ = ReleaseCapture();
        let windows: Vec<HWND> = MENU.take().map(|menu| menu.panels.iter().map(|panel| panel.window).collect()).unwrap_or_default();
        for window in windows.into_iter().rev() {
            let _ = DestroyWindow(window);
        }
    }
    Ok(CHOSEN.get())
}

/// A window for a panel with the system's Acrylic behind it, rounded and shadowed like the system's menus.
fn backdrop_window(frame: RECT, light: bool, root: bool) -> Result<HWND> {
    unsafe {
        let style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | if root { WINDOW_EX_STYLE(0) } else { WS_EX_NOACTIVATE };
        let window = CreateWindowExW(
            style,
            CLASS,
            PCWSTR::null(),
            WS_POPUP,
            frame.left,
            frame.top,
            frame.right - frame.left,
            frame.bottom - frame.top,
            None,
            None,
            None,
            None,
        )?;
        let dark = BOOL::from(!light);
        let set = |attribute: DWMWINDOWATTRIBUTE, value: &i32| {
            DwmSetWindowAttribute(window, attribute, (value as *const i32).cast(), size_of::<i32>() as u32)
        };
        let whole = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
        let styled =
            set(DWMWA_USE_IMMERSIVE_DARK_MODE, &dark.0)
                .and_then(|()| set(DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND.0))
                .and_then(|()| DwmExtendFrameIntoClientArea(window, &whole))
                .and_then(|()| {
                    if transparency() && blur_behind(window, light) { Ok(()) } else { set(DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_TRANSIENTWINDOW.0) }
                });
        if let Err(error) = styled {
            let _ = DestroyWindow(window);
            return Err(error);
        }
        Ok(window)
    }
}

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    color: u32,
    animation: u32,
}

#[repr(C)]
struct CompositionData {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}

/// Blurs what is behind a window under a tint of the menu's own, through user32's undocumented accent policy, which
/// the taskbar uses too: the documented Acrylic tints so heavily that next to nothing shows through.
fn blur_behind(window: HWND, light: bool) -> bool {
    const WCA_ACCENT_POLICY: u32 = 19;
    const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;
    type Set = unsafe extern "system" fn(HWND, *mut CompositionData) -> BOOL;
    unsafe {
        let Ok(user) = GetModuleHandleW(w!("user32.dll")) else { return false };
        let Some(procedure) = GetProcAddress(user, s!("SetWindowCompositionAttribute")) else { return false };
        let set: Set = std::mem::transmute(procedure);
        let color = if light { TINT_LIGHT } else { TINT_DARK };
        let mut policy = AccentPolicy { state: ACCENT_ENABLE_ACRYLICBLURBEHIND, flags: 0, color, animation: 0 };
        let mut data = CompositionData { attribute: WCA_ACCENT_POLICY, data: (&raw mut policy).cast(), size: size_of::<AccentPolicy>() };
        set(window, &mut data).as_bool()
    }
}

/// Whether "Transparency effects" is on in the settings; without them the menu takes the system's plain backdrop.
fn transparency() -> bool {
    let mut value = 1u32;
    let mut size = size_of::<u32>() as u32;
    let _ = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    value != 0
}

fn show(window: HWND, root: bool) {
    unsafe {
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | if root { SET_WINDOW_POS_FLAGS(0) } else { SWP_NOACTIVATE };
        let _ = SetWindowPos(window, Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
        // The backdrop shows as on an active window, also on a submenu, which never takes the foreground.
        SendMessageW(window, WM_NCACTIVATE, Some(WPARAM(1)), None);
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_NCACTIVATE => DefWindowProcW(window, message, WPARAM(1), lparam),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(window, &mut paint);
                MENU.with(|menu| {
                    if let Ok(menu) = menu.try_borrow()
                        && let Some(panel) = menu.as_ref().and_then(|menu| menu.panels.iter().find(|panel| panel.window == window))
                    {
                        blit(dc, panel.size(), &panel.pixels);
                    }
                });
                let _ = EndPaint(window, &paint);
                LRESULT(0)
            }
            WM_ACTIVATE if wparam.0 & 0xffff == WA_INACTIVE as usize => {
                CLOSED.set(true);
                LRESULT(0)
            }
            WM_CANCELMODE => {
                CLOSED.set(true);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let point = screen_point(window, lparam);
                act(|menu| menu.pointer(point));
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                let point = screen_point(window, lparam);
                act(|menu| {
                    if menu.panel_at(point).is_none() {
                        CLOSED.set(true);
                    }
                    false
                });
                LRESULT(0)
            }
            WM_LBUTTONUP | WM_RBUTTONUP => {
                let point = screen_point(window, lparam);
                act(|menu| {
                    let Some(p) = menu.panel_at(point) else { return false };
                    match menu.entry_at(p, point) {
                        Some(i) => menu.activate(p, i, false),
                        None => false,
                    }
                });
                LRESULT(0)
            }
            WM_KEYDOWN => {
                act(|menu| menu.press(wparam.0));
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_POINTER => {
                let mut point = POINT::default();
                let _ = GetCursorPos(&mut point);
                act(|menu| menu.panel_at(point).is_none() && menu.pointer(point));
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_SUBMENU => {
                let _ = KillTimer(Some(window), TIMER_SUBMENU);
                act(|menu| menu.rest());
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

/// A mouse message's point, on the screen.
fn screen_point(window: HWND, lparam: LPARAM) -> POINT {
    let mut point = POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 };
    unsafe {
        let _ = ClientToScreen(window, &mut point);
    }
    point
}

/// Changes the menu and, if that says its look changed, draws it again. The messages that windows send while it
/// changes find the menu busy and get the default handling.
fn act(change: impl FnOnce(&mut Menu) -> bool) {
    MENU.with(|menu| {
        let Ok(mut menu) = menu.try_borrow_mut() else { return };
        let Some(menu) = menu.as_mut() else { return };
        if change(menu) {
            for p in 0..menu.panels.len() {
                menu.render(p);
            }
        }
    });
}

impl Menu {
    fn new(anchor: POINT, light: bool) -> Result<Menu> {
        unsafe {
            let monitor = MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let (mut dpi, mut _y) = (96, 96);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut _y);
            let mut delay = 400u32;
            let _ = SystemParametersInfoW(SPI_GETMENUSHOWDELAY, 0, Some((&raw mut delay).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));

            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = |family: PCWSTR, size: f32| {
                write.CreateTextFormat(
                    family,
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-us"),
                )
            };
            let text = format(w!("Segoe UI Variable Text"), TEXT_SIZE)?;
            text.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let glyphs = format(w!("Segoe Fluent Icons"), GLYPH_SIZE)?;
            glyphs.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            glyphs.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            Ok(Menu {
                panels: Vec::new(),
                scale: dpi.max(96) as f32 / 96.0,
                work: info.rcWork,
                light,
                palette: Palette::new(light),
                resting: None,
                delay: delay.max(1),
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                imaging: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
                write,
                text,
                glyphs,
            })
        }
    }

    /// A panel for `entries`, sized to fit them; its window and place are left to the caller.
    fn panel(&self, entries: Vec<Entry>, from: usize) -> Result<Panel> {
        let mut widest: f32 = 0.0;
        for label in entries.iter().filter_map(Entry::label) {
            let mut metrics = DWRITE_TEXT_METRICS::default();
            unsafe { self.layout(label, 10_000.0)?.GetMetrics(&mut metrics)? };
            widest = widest.max(metrics.widthIncludingTrailingWhitespace);
        }
        let mut tops = Vec::with_capacity(entries.len());
        let mut y = PAD;
        for entry in &entries {
            tops.push(y);
            y += if matches!(entry, Entry::Separator) { SEPARATOR } else { ROW };
        }
        Ok(Panel {
            window: HWND::default(),
            entries,
            tops,
            width: (LABEL + widest + TRAILING).ceil().max(MIN_WIDTH),
            height: y + PAD,
            frame: RECT::default(),
            hover: None,
            from,
            pixels: Vec::new(),
        })
    }

    fn layout(&self, text: &str, width: f32) -> Result<IDWriteTextLayout> {
        let units: Vec<u16> = text.encode_utf16().collect();
        unsafe { self.write.CreateTextLayout(&units, &self.text, width, ROW) }
    }

    /// The deepest panel under a point on the screen.
    fn panel_at(&self, point: POINT) -> Option<usize> {
        self.panels.iter().rposition(|panel| {
            let frame = panel.frame;
            point.x >= frame.left && point.x < frame.right && point.y >= frame.top && point.y < frame.bottom
        })
    }

    /// The entry of a panel under a point on the screen, if it can be chosen.
    fn entry_at(&self, p: usize, point: POINT) -> Option<usize> {
        let panel = &self.panels[p];
        let x = (point.x - panel.frame.left) as f32 / self.scale;
        let y = (point.y - panel.frame.top) as f32 / self.scale;
        if x < PAD || x >= panel.width - PAD {
            return None;
        }
        let i = panel.tops.iter().rposition(|&top| y >= top)?;
        let height = if matches!(panel.entries[i], Entry::Separator) { SEPARATOR } else { ROW };
        (y < panel.tops[i] + height && panel.entries[i].selectable()).then_some(i)
    }

    fn set_hover(&mut self, p: usize, hover: Option<usize>) -> bool {
        let changed = self.panels[p].hover != hover;
        self.panels[p].hover = hover;
        changed
    }

    /// Follows the pointer: lights what it is over, and once it rests, closes the submenus it left and opens the
    /// one it is on.
    fn pointer(&mut self, point: POINT) -> bool {
        let root = self.panels[0].window;
        let last = self.panels.len() - 1;
        let Some(p) = self.panel_at(point) else {
            // Off the menu, only the entries that opened a submenu stay lit.
            self.wait(root, None);
            return self.set_hover(last, None);
        };
        let entry = self.entry_at(p, point);
        let mut changed = self.set_hover(p, entry);
        for q in 0..p {
            let from = self.panels[q + 1].from;
            changed |= self.set_hover(q, Some(from));
        }
        for q in p + 1..self.panels.len() {
            changed |= self.set_hover(q, None);
        }
        let opened = self.panels.get(p + 1).map(|panel| panel.from);
        let keep = if entry.is_some() && entry == opened { p + 2 } else { p + 1 };
        let opens = entry.is_some_and(|i| self.panels[p].entries[i].is_submenu()) && entry != opened;
        self.wait(root, (self.panels.len() > keep || opens).then_some((p, entry)));
        changed
    }

    /// Starts the submenu timer for where the pointer is resting, unless it already runs for it, or stops it.
    fn wait(&mut self, root: HWND, resting: Option<(usize, Option<usize>)>) {
        if resting == self.resting {
            return;
        }
        self.resting = resting;
        unsafe {
            if resting.is_some() {
                SetTimer(Some(root), TIMER_SUBMENU, self.delay, None);
            } else {
                let _ = KillTimer(Some(root), TIMER_SUBMENU);
            }
        }
    }

    /// The pointer has rested: closes the submenus it has left and opens the one it is on.
    fn rest(&mut self) -> bool {
        let Some((p, entry)) = self.resting.take() else { return false };
        if p >= self.panels.len() {
            return false;
        }
        let stays = entry.is_some() && self.panels.get(p + 1).map(|panel| panel.from) == entry;
        let mut changed = self.close_after(if stays { p + 1 } else { p });
        if let Some(i) = entry
            && !stays
            && self.panels[p].entries[i].is_submenu()
        {
            changed |= self.open(p, i, false);
        }
        changed
    }

    /// Closes the panels after `p`.
    fn close_after(&mut self, p: usize) -> bool {
        let mut changed = false;
        while self.panels.len() > p + 1 {
            if let Some(panel) = self.panels.pop() {
                unsafe {
                    let _ = DestroyWindow(panel.window);
                }
                changed = true;
            }
        }
        changed
    }

    /// Opens the submenu of entry `i` of panel `p` beside it, its first entry lit if the keyboard opened it.
    fn open(&mut self, p: usize, i: usize, keyboard: bool) -> bool {
        match self.place(p, i, keyboard) {
            Ok(()) => {
                self.panels[p].hover = Some(i);
                true
            }
            Err(error) => {
                log!("submenu failed: {error}");
                false
            }
        }
    }

    fn place(&mut self, p: usize, i: usize, keyboard: bool) -> Result<()> {
        let Entry::Submenu { entries, .. } = self.panels[p].entries[i].clone() else { return Ok(()) };
        let mut panel = self.panel(entries, i)?;
        let parent = &self.panels[p];
        let size = ((panel.width * self.scale).ceil() as i32, (panel.height * self.scale).ceil() as i32);
        let overlap = (OVERLAP * self.scale).round() as i32;
        let mut left = parent.frame.right - overlap;
        if left + size.0 > self.work.right {
            left = parent.frame.left - size.0 + overlap;
        }
        let top = parent.frame.top + ((parent.tops[i] - PAD) * self.scale).round() as i32;
        let top = top.min(self.work.bottom - size.1).max(self.work.top);
        panel.frame = RECT { left, top, right: left + size.0, bottom: top + size.1 };
        panel.window = backdrop_window(panel.frame, self.light, false)?;
        if keyboard {
            panel.hover = panel.entries.iter().position(Entry::selectable);
        }
        let window = panel.window;
        self.panels.push(panel);
        self.render(p + 1);
        show(window, false);
        Ok(())
    }

    /// Chooses an item or opens a submenu.
    fn activate(&mut self, p: usize, i: usize, keyboard: bool) -> bool {
        match &self.panels[p].entries[i] {
            Entry::Item { id, enabled: true, .. } => {
                CHOSEN.set(*id);
                CLOSED.set(true);
                false
            }
            Entry::Submenu { .. } => {
                let root = self.panels[0].window;
                self.wait(root, None);
                if self.panels.get(p + 1).is_some_and(|panel| panel.from == i) {
                    let first = self.panels[p + 1].entries.iter().position(Entry::selectable);
                    return keyboard && self.set_hover(p + 1, first);
                }
                self.close_after(p);
                self.open(p, i, keyboard)
            }
            _ => false,
        }
    }

    /// Handles a key in the deepest panel open.
    fn press(&mut self, key: usize) -> bool {
        let is = |code: u16| key == code as usize;
        let last = self.panels.len() - 1;
        if is(VK_ESCAPE.0) || is(VK_LEFT.0) {
            if last > 0 {
                return self.close_after(last - 1);
            }
            if is(VK_ESCAPE.0) {
                CLOSED.set(true);
            }
            false
        } else if is(VK_DOWN.0) || is(VK_UP.0) {
            self.step(last, is(VK_DOWN.0))
        } else if is(VK_RIGHT.0) {
            match self.panels[last].hover {
                Some(i) if self.panels[last].entries[i].is_submenu() => self.activate(last, i, true),
                _ => false,
            }
        } else if is(VK_RETURN.0) || is(VK_SPACE.0) {
            match self.panels[last].hover {
                Some(i) => self.activate(last, i, true),
                None => false,
            }
        } else {
            false
        }
    }

    /// Moves the light in a panel to the next entry that can be chosen, wrapping around.
    fn step(&mut self, p: usize, forward: bool) -> bool {
        let panel = &self.panels[p];
        let list: Vec<usize> = (0..panel.entries.len()).filter(|&i| panel.entries[i].selectable()).collect();
        if list.is_empty() {
            return false;
        }
        let current = panel.hover.and_then(|hover| list.iter().position(|&i| i == hover));
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => list.len() - 1,
            (Some(at), true) => (at + 1) % list.len(),
            (Some(at), false) => (at + list.len() - 1) % list.len(),
        };
        self.set_hover(p, Some(list[next]))
    }

    /// Draws a panel again and has its window show it.
    fn render(&mut self, p: usize) {
        match self.paint(&self.panels[p]) {
            Ok(pixels) => {
                self.panels[p].pixels = pixels;
                unsafe {
                    let _ = InvalidateRect(Some(self.panels[p].window), None, false);
                }
            }
            Err(error) => log!("menu paint failed: {error}"),
        }
    }

    fn paint(&self, panel: &Panel) -> Result<Vec<u8>> {
        let size = panel.size();
        unsafe {
            let bitmap = self.imaging.CreateBitmap(size.0 as u32, size.1 as u32, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0 * self.scale,
                dpiY: 96.0 * self.scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &properties)?;
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F::default()));
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let drawn = self.draw(&target, panel);
            target.EndDraw(None, None)?;
            drawn?;
            drop(target);
            let mut pixels = vec![0u8; (size.0 * size.1 * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), (size.0 * 4) as u32, &mut pixels)?;
            Ok(pixels)
        }
    }

    fn draw(&self, target: &ID2D1RenderTarget, panel: &Panel) -> Result<()> {
        let palette = self.palette;
        unsafe {
            let ink = target.CreateSolidColorBrush(&palette.ink, None)?;
            let dim = target.CreateSolidColorBrush(&palette.dim, None)?;
            let hover = target.CreateSolidColorBrush(&palette.hover, None)?;
            let line = target.CreateSolidColorBrush(&palette.line, None)?;
            for (i, entry) in panel.entries.iter().enumerate() {
                let top = panel.tops[i];
                let (label, checked, radio, enabled) = match entry {
                    Entry::Separator => {
                        let y = top + SEPARATOR / 2.0;
                        target.DrawLine(Vector2 { X: PAD, Y: y }, Vector2 { X: panel.width - PAD, Y: y }, &line, 1.0, None);
                        continue;
                    }
                    Entry::Item { label, checked, enabled, radio, .. } => (label.as_str(), *checked, *radio, *enabled),
                    Entry::Submenu { label, .. } => (label.as_str(), false, false, true),
                };
                if enabled && panel.hover == Some(i) {
                    let rect = D2D_RECT_F { left: PAD, top, right: panel.width - PAD, bottom: top + ROW };
                    target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect, radiusX: ROW_RADIUS, radiusY: ROW_RADIUS }, &hover);
                }
                let brush = if enabled { &ink } else { &dim };
                if checked && radio {
                    let dot = D2D1_ELLIPSE { point: Vector2 { X: MARK, Y: top + ROW / 2.0 }, radiusX: 3.0, radiusY: 3.0 };
                    target.FillEllipse(&dot, brush);
                } else if checked {
                    self.glyph(target, CHECK, MARK, top, brush);
                }
                if entry.is_submenu() {
                    self.glyph(target, CHEVRON, panel.width - PAD - 14.0, top, brush);
                }
                let layout = self.layout(label, panel.width - LABEL - TRAILING / 2.0)?;
                target.DrawTextLayout(Vector2 { X: LABEL, Y: top }, &layout, brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
        }
        Ok(())
    }

    /// One of the system's icons, centred on `x` in the row starting at `top`.
    fn glyph(&self, target: &ID2D1RenderTarget, glyph: &str, x: f32, top: f32, brush: &ID2D1SolidColorBrush) {
        let units: Vec<u16> = glyph.encode_utf16().collect();
        unsafe {
            if let Ok(layout) = self.write.CreateTextLayout(&units, &self.glyphs, 16.0, ROW) {
                target.DrawTextLayout(Vector2 { X: x - 8.0, Y: top }, &layout, brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
        }
    }
}

/// Copies premultiplied pixels into a window's client area; the backdrop shows through where they are clear.
fn blit(dc: HDC, size: (i32, i32), pixels: &[u8]) {
    if pixels.len() < (size.0 * size.1 * 4) as usize {
        return;
    }
    unsafe {
        let memory = CreateCompatibleDC(Some(dc));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0,
                biHeight: -size.1,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        if let Ok(bitmap) = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            let _ = BitBlt(dc, 0, 0, size.0, size.1, Some(memory), 0, 0, SRCCOPY);
            SelectObject(memory, previous);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
        let _ = DeleteDC(memory);
    }
}
