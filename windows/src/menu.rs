//! The tray menu on Windows 11: panels of Acrylic, the material of the system's own menus and flyouts, with the entries
//! drawn on it by Direct2D and submenus opening beside them. It runs like `TrackPopupMenuEx`: `track` returns the id
//! chosen once the menu has closed.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::Once;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Factory, ID2D1RenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_TEXT_METRICS, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
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
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, Result, s, w};
use windows_numerics::{Matrix3x2, Vector2};

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
const HEADER: f32 = 24.0;
const ROW_RADIUS: f32 = 4.0;
/// The centre of the marks and the start of the labels, from the panel's left edge.
const MARK: f32 = 20.0;
const LABEL: f32 = 36.0;
/// Room after the longest label, where a submenu's chevron goes.
const TRAILING: f32 = 40.0;
const MIN_WIDTH: f32 = 160.0;
/// The inset of a panel with no checkmarks or radio dots to show, the same on both sides.
const INSET: f32 = 12.0;
/// A slider's row: the track with its thumb, the caption under it.
const SLIDER: f32 = 46.0;
const SLIDER_BARE: f32 = 32.0;
const THUMB: f32 = 10.0;
const TRACK: f32 = 4.0;
/// A panel with a slider is at least this wide, for its steps to lie far enough apart.
const SLIDER_WIDTH: f32 = 240.0;
/// A row of icons, each over its label, and the least width each takes.
const ICONS: f32 = 56.0;
const TILE_WIDTH: f32 = 72.0;
const ICON: f32 = 20.0;
/// An icon summing up a folded row, and the room between two.
const SUMMARY_ICON: f32 = 15.0;
const SUMMARY_GAP: f32 = 5.0;
const TEXT_SIZE: f32 = 14.0;
const GLYPH_SIZE: f32 = 12.0;
const HEADER_SIZE: f32 = 12.0;
/// How far a submenu reaches back over the panel it opens from.
const OVERLAP: f32 = 4.0;
/// The tint over the blur, as 0xAABBGGRR: light enough to see through, yet dark or pale enough to read on.
const TINT_DARK: u32 = 0x732C_2C2C;
const TINT_LIGHT: u32 = 0x80F3_F3F3;
const CHECK: &str = "\u{E73E}";
const CHEVRON: &str = "\u{E76C}";
const CHEVRON_DOWN: &str = "\u{E70D}";
const HEART: &str = "\u{EB52}";
/// The site's sponsor heart, and how far the label after it moves over.
const HEART_COLOR: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.92, g: 0.51, b: 0.51, a: 1.0 };
const HEART_SPACE: f32 = 22.0;

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
        /// Switches an option and leaves the menu open, so several can be changed in one go.
        keep_open: bool,
        /// Drawn with the sponsor heart ahead of its label.
        heart: bool,
    },
    /// The title of the group of entries under it.
    Header(String),
    /// One of `stops` values along a track, `at` the one chosen. Moving it goes to the menu's `toggle` as `id + at`,
    /// leaving the menu open.
    /// Empty while a `Disclosure` above has it folded away; with no caption, the row above shows its value.
    Slider {
        id: u32,
        at: u32,
        stops: u32,
        caption: String,
        shown: bool,
    },
    Submenu {
        label: String,
        entries: Vec<Entry>,
    },
    /// Folds the row under it out or away, leaving the menu open; folded, `summary` trails it, and unfolded too with
    /// `keeps_summary`, for a value the row sets. With `icons`, those trail it instead: less room than their names.
    Disclosure {
        id: u32,
        label: String,
        summary: String,
        icons: Vec<Icon>,
        open: bool,
        keeps_summary: bool,
    },
    /// Icons that each switch something on or off, leaving the menu open: the row a `Disclosure` folds, empty while it
    /// is folded.
    Icons {
        tiles: Vec<Tile>,
        shown: bool,
    },
    Separator,
}

/// One icon of a row, lit while what it switches is on.
#[derive(Clone)]
pub struct Tile {
    pub id: u32,
    pub icon: Icon,
    pub label: String,
    pub on: bool,
}

#[derive(Clone, Copy)]
pub enum Icon {
    Ground,
    Windows,
}

impl Entry {
    pub fn item(id: u32, label: &str, checked: bool, enabled: bool) -> Entry {
        Entry::Item { id, label: label.to_string(), checked, enabled, radio: false, keep_open: false, heart: false }
    }

    /// An item with the sponsor heart ahead of its label.
    pub fn loved(id: u32, label: &str) -> Entry {
        Entry::Item { id, label: label.to_string(), checked: false, enabled: true, radio: false, keep_open: false, heart: true }
    }

    pub fn toggle(id: u32, label: &str, checked: bool, enabled: bool) -> Entry {
        Entry::Item { id, label: label.to_string(), checked, enabled, radio: false, keep_open: true, heart: false }
    }

    pub fn header(label: &str) -> Entry {
        Entry::Header(label.to_string())
    }

    pub fn choice(id: u32, label: &str, chosen: bool) -> Entry {
        Entry::Item { id, label: label.to_string(), checked: chosen, enabled: true, radio: true, keep_open: false, heart: false }
    }

    pub fn slider(id: u32, at: u32, stops: u32, caption: &str) -> Entry {
        Entry::Slider { id, at: at.min(stops - 1), stops, caption: caption.to_string(), shown: true }
    }

    /// A slider with no caption, under a `Disclosure` that shows its value and folds it away.
    pub fn folded_slider(id: u32, at: u32, stops: u32, shown: bool) -> Entry {
        Entry::Slider { id, at: at.min(stops - 1), stops, caption: String::new(), shown }
    }

    fn label(&self) -> Option<&str> {
        match self {
            Entry::Item { label, .. } | Entry::Submenu { label, .. } | Entry::Header(label) | Entry::Disclosure { label, .. } => Some(label),
            Entry::Separator | Entry::Slider { .. } | Entry::Icons { .. } => None,
        }
    }

    fn selectable(&self) -> bool {
        match self {
            Entry::Item { enabled, .. } => *enabled,
            Entry::Submenu { .. } | Entry::Disclosure { .. } => true,
            Entry::Icons { shown, .. } | Entry::Slider { shown, .. } => *shown,
            Entry::Separator | Entry::Header(_) => false,
        }
    }

    fn height(&self) -> f32 {
        match self {
            Entry::Separator => SEPARATOR,
            Entry::Header(_) => HEADER,
            Entry::Slider { shown: false, .. } => 0.0,
            Entry::Slider { caption, .. } if caption.is_empty() => SLIDER_BARE,
            Entry::Slider { .. } => SLIDER,
            Entry::Icons { shown, .. } => {
                if *shown {
                    ICONS
                } else {
                    0.0
                }
            }
            _ => ROW,
        }
    }

    fn is_submenu(&self) -> bool {
        matches!(self, Entry::Submenu { .. })
    }

    fn is_slider(&self) -> bool {
        matches!(self, Entry::Slider { .. })
    }

    fn tiles(&self) -> usize {
        match self {
            Entry::Icons { tiles, .. } => tiles.len(),
            _ => 0,
        }
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
    /// A slider's caption, its track, its thumb and the thumb's rim, after the system's own sliders.
    soft: D2D1_COLOR_F,
    track: D2D1_COLOR_F,
    thumb: D2D1_COLOR_F,
    rim: D2D1_COLOR_F,
    accent: D2D1_COLOR_F,
}

impl Palette {
    fn new(light: bool) -> Palette {
        let tone = if light { 0.0 } else { 1.0 };
        let color = |a: f32| D2D1_COLOR_F { r: tone, g: tone, b: tone, a };
        let accent = accent(light);
        if light {
            Palette {
                ink: color(0.894),
                dim: color(0.361),
                hover: color(0.037),
                line: color(0.058),
                soft: color(0.606),
                track: color(0.447),
                thumb: D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
                rim: color(0.06),
                accent,
            }
        } else {
            Palette {
                ink: color(1.0),
                dim: color(0.363),
                hover: color(0.061),
                line: color(0.084),
                soft: color(0.786),
                track: color(0.545),
                thumb: D2D1_COLOR_F { r: 0.271, g: 0.271, b: 0.271, a: 1.0 },
                rim: color(0.094),
                accent,
            }
        }
    }
}

/// The user's accent as the system's sliders fill with it: a lighter shade on dark menus, a darker one on light.
fn accent(light: bool) -> D2D1_COLOR_F {
    let mut shades = [0u8; 32];
    let mut size = shades.len() as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent"),
            w!("AccentPalette"),
            RRF_RT_REG_BINARY,
            None,
            Some(shades.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    // Eight shades, lightest first, the accent itself fourth: dark menus take the second, light ones the fifth.
    let (r, g, b) = if read.0 == 0 && size as usize >= shades.len() {
        let at = if light { 16 } else { 4 };
        (shades[at], shades[at + 1], shades[at + 2])
    } else if light {
        (0x00, 0x5f, 0xb8)
    } else {
        (0x4c, 0xc2, 0xff)
    };
    D2D1_COLOR_F { r: f32::from(r) / 255.0, g: f32::from(g) / 255.0, b: f32::from(b) / 255.0, a: 1.0 }
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
    /// The icon lit in a row of them, while that row is.
    tile: usize,
    /// The entry of the panel before it that opened it.
    from: usize,
    /// Where labels start, in DIPs from the panel's left edge.
    label: f32,
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
    /// Where the menu was asked to show, which places it again when it grows or shrinks.
    anchor: POINT,
    scale: f32,
    work: RECT,
    light: bool,
    palette: Palette,
    /// The panel and entry the pointer last came to rest on, while the submenu timer runs.
    resting: Option<(usize, Option<usize>)>,
    /// The panel and slider being dragged.
    sliding: Option<(usize, usize)>,
    delay: u32,
    d2d: ID2D1Factory,
    imaging: IWICImagingFactory,
    write: IDWriteFactory,
    text: IDWriteTextFormat,
    heading: IDWriteTextFormat,
    caption: IDWriteTextFormat,
    glyphs: IDWriteTextFormat,
}

thread_local! {
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
    static CLOSED: Cell<bool> = const { Cell::new(false) };
    static CHOSEN: Cell<u32> = const { Cell::new(0) };
    static TOGGLED: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Shows the menu at `x`, `y` in screen pixels, the way the system places a menu there, runs it until it closes and
/// returns the id of the item chosen, or 0. Fails if it can't be shown, so that the system's menu can take over.
/// An item that keeps the menu open goes to `toggle` instead, which returns the entries as they now are.
pub fn track(x: i32, y: i32, entries: Vec<Entry>, light: bool, toggle: &mut dyn FnMut(u32) -> Vec<Entry>) -> Result<u32> {
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
    TOGGLED.set(None);

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
            if let Some(id) = TOGGLED.take() {
                let entries = toggle(id);
                act(|menu| menu.refresh(entries));
            }
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
        // The menu holds the mouse, so nothing else sets the cursor while it is open: on the first open after launch,
        // the busy cursor Windows shows for a starting app would stay over it.
        ready_cursor();
    }
}

fn ready_cursor() {
    unsafe {
        if let Ok(arrow) = LoadCursorW(None, IDC_ARROW) {
            let _ = SetCursor(Some(arrow));
        }
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_NCACTIVATE => DefWindowProcW(window, message, WPARAM(1), lparam),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            // Keep the arrow while the pointer is over the menu; otherwise AppStarting from the first open sticks.
            WM_SETCURSOR => {
                ready_cursor();
                LRESULT(1)
            }
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
                act(|menu| if menu.sliding.is_some() { menu.drag(point) } else { menu.pointer(point) });
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                let point = screen_point(window, lparam);
                let left = message == WM_LBUTTONDOWN;
                act(|menu| {
                    let Some(p) = menu.panel_at(point) else {
                        CLOSED.set(true);
                        return false;
                    };
                    match menu.entry_at(p, point) {
                        Some(i) if left && menu.panels[p].entries[i].is_slider() => {
                            menu.sliding = Some((p, i));
                            menu.drag(point);
                            true
                        }
                        _ => false,
                    }
                });
                LRESULT(0)
            }
            WM_LBUTTONUP | WM_RBUTTONUP => {
                let point = screen_point(window, lparam);
                act(|menu| {
                    if menu.sliding.take().is_some() {
                        return true;
                    }
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
            WM_MOUSEWHEEL => {
                // Unlike the other mouse messages, the wheel's carries a point on the screen.
                let point = POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 };
                let up = ((wparam.0 >> 16) & 0xffff) as i16 > 0;
                act(|menu| {
                    let Some(p) = menu.panel_at(point) else { return false };
                    match menu.entry_at(p, point) {
                        Some(i) => menu.nudge(p, i, if up { 1 } else { -1 }),
                        None => false,
                    }
                });
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_POINTER => {
                let mut point = POINT::default();
                let _ = GetCursorPos(&mut point);
                act(|menu| menu.sliding.is_none() && menu.panel_at(point).is_none() && menu.pointer(point));
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
            let format = |family: PCWSTR, size: f32, weight: DWRITE_FONT_WEIGHT| {
                write.CreateTextFormat(family, None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))
            };
            let text = format(w!("Segoe UI Variable Text"), TEXT_SIZE, DWRITE_FONT_WEIGHT_NORMAL)?;
            text.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let heading = format(w!("Segoe UI Variable Text"), HEADER_SIZE, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            heading.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let caption = format(w!("Segoe UI Variable Text"), HEADER_SIZE, DWRITE_FONT_WEIGHT_NORMAL)?;
            caption.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            caption.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            let glyphs = format(w!("Segoe Fluent Icons"), GLYPH_SIZE, DWRITE_FONT_WEIGHT_NORMAL)?;
            glyphs.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            glyphs.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            Ok(Menu {
                panels: Vec::new(),
                anchor,
                scale: dpi.max(96) as f32 / 96.0,
                work: info.rcWork,
                light,
                palette: Palette::new(light),
                resting: None,
                sliding: None,
                delay: delay.max(1),
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                imaging: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
                write,
                text,
                heading,
                caption,
                glyphs,
            })
        }
    }

    /// A panel for `entries`, sized to fit them; its window and place are left to the caller.
    fn panel(&self, entries: Vec<Entry>, from: usize) -> Result<Panel> {
        let mut widest: f32 = 0.0;
        for entry in &entries {
            let Some(label) = entry.label() else { continue };
            let mut metrics = DWRITE_TEXT_METRICS::default();
            unsafe { self.layout(label, 10_000.0)?.GetMetrics(&mut metrics)? };
            let heart = if matches!(entry, Entry::Item { heart: true, .. }) { HEART_SPACE } else { 0.0 };
            let summary = match entry {
                Entry::Disclosure { icons, .. } if !icons.is_empty() => icons.len() as f32 * (SUMMARY_ICON + SUMMARY_GAP) + 16.0,
                Entry::Disclosure { summary, .. } => {
                    let mut trailing = DWRITE_TEXT_METRICS::default();
                    unsafe { self.layout(summary, 10_000.0)?.GetMetrics(&mut trailing)? };
                    trailing.widthIncludingTrailingWhitespace + 16.0
                }
                _ => 0.0,
            };
            widest = widest.max(metrics.widthIncludingTrailingWhitespace + heart + summary);
        }
        let tiles = entries.iter().map(Entry::tiles).max().unwrap_or(0) as f32 * TILE_WIDTH + 2.0 * PAD;
        let mut tops = Vec::with_capacity(entries.len());
        let mut y = PAD;
        for entry in &entries {
            tops.push(y);
            y += entry.height();
        }
        let marks = entries.iter().any(|entry| matches!(entry, Entry::Item { checked, radio, keep_open, .. } if *checked || *radio || *keep_open));
        let label = if marks { LABEL } else { INSET };
        let least = if entries.iter().any(Entry::is_slider) { SLIDER_WIDTH } else { MIN_WIDTH };
        Ok(Panel {
            window: HWND::default(),
            entries,
            tops,
            label,
            width: (label + widest + TRAILING).ceil().max(least).max(tiles),
            height: y + PAD,
            frame: RECT::default(),
            hover: None,
            tile: 0,
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
        (y < panel.tops[i] + panel.entries[i].height() && panel.entries[i].selectable()).then_some(i)
    }

    /// The icon under a point on the screen, if entry `i` of panel `p` is a row of them.
    fn tile_at(&self, p: usize, i: usize, point: POINT) -> Option<usize> {
        let panel = &self.panels[p];
        let count = panel.entries[i].tiles();
        if count == 0 {
            return None;
        }
        let x = (point.x - panel.frame.left) as f32 / self.scale;
        let width = (panel.width - 2.0 * PAD) / count as f32;
        Some((((x - PAD) / width).max(0.0) as usize).min(count - 1))
    }

    fn set_hover(&mut self, p: usize, hover: Option<usize>) -> bool {
        let changed = self.panels[p].hover != hover;
        self.panels[p].hover = hover;
        changed
    }

    /// Moves the slider being dragged to the stop nearest the pointer.
    fn drag(&mut self, point: POINT) -> bool {
        let Some((p, i)) = self.sliding else { return false };
        let panel = &self.panels[p];
        let &Entry::Slider { stops, .. } = &panel.entries[i] else { return false };
        let (left, right) = travel(panel);
        let x = (point.x - panel.frame.left) as f32 / self.scale;
        let along = ((x - left) / (right - left)).clamp(0.0, 1.0);
        self.set_slider(p, i, (along * (stops - 1) as f32).round() as u32)
    }

    /// Moves a slider a stop up or down, if entry `i` of panel `p` is one.
    fn nudge(&mut self, p: usize, i: usize, step: i32) -> bool {
        let &Entry::Slider { at, stops, .. } = &self.panels[p].entries[i] else { return false };
        self.set_slider(p, i, (at as i32 + step).clamp(0, stops as i32 - 1) as u32)
    }

    fn set_slider(&mut self, p: usize, i: usize, next: u32) -> bool {
        let Entry::Slider { id, at, .. } = &mut self.panels[p].entries[i] else { return false };
        if *at == next {
            return false;
        }
        *at = next;
        TOGGLED.set(Some(*id + next));
        true
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
        if let Some(i) = entry
            && let Some(tile) = self.tile_at(p, i, point)
            && tile != self.panels[p].tile
        {
            self.panels[p].tile = tile;
            changed = true;
        }
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
            Entry::Item { id, enabled: true, keep_open: true, .. } => {
                TOGGLED.set(Some(*id));
                false
            }
            Entry::Item { id, enabled: true, .. } => {
                CHOSEN.set(*id);
                CLOSED.set(true);
                false
            }
            Entry::Disclosure { id, .. } => {
                TOGGLED.set(Some(*id));
                false
            }
            Entry::Icons { tiles, shown: true } => {
                if let Some(tile) = tiles.get(self.panels[p].tile) {
                    TOGGLED.set(Some(tile.id));
                }
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

    /// Shows the entries as they are after an option was switched, in each panel still open. Should a panel's list have
    /// changed shape, the menu closes, as it would after any other choice.
    fn refresh(&mut self, entries: Vec<Entry>) -> bool {
        let mut list = entries;
        for p in 0..self.panels.len() {
            if list.len() != self.panels[p].entries.len() {
                CLOSED.set(true);
                return false;
            }
            let grown = list.iter().zip(&self.panels[p].entries).any(|(new, old)| new.height() != old.height());
            self.panels[p].entries = list;
            if grown {
                return self.reshape(p);
            }
            let Some(Entry::Submenu { entries, .. }) = self.panels.get(p + 1).map(|next| self.panels[p].entries[next.from].clone()) else { break };
            list = entries;
        }
        true
    }

    /// Fits panel `p` to its entries again, as after a row in it folded out or away, the menu placed again where it was
    /// asked to show and a submenu where it opened; the submenus it had open close.
    fn reshape(&mut self, p: usize) -> bool {
        self.close_after(p);
        let scale = self.scale;
        let panel = &mut self.panels[p];
        let mut y = PAD;
        for (top, entry) in panel.tops.iter_mut().zip(&panel.entries) {
            *top = y;
            y += entry.height();
        }
        panel.height = y + PAD;
        if panel.hover.is_some_and(|i| !panel.entries[i].selectable()) {
            panel.hover = None;
        }
        let size = SIZE { cx: (panel.width * scale).ceil() as i32, cy: (panel.height * scale).ceil() as i32 };
        let mut frame = RECT { left: panel.frame.left, top: panel.frame.top, right: panel.frame.left + size.cx, bottom: panel.frame.top + size.cy };
        if p == 0 {
            let flags = (TPM_LEFTALIGN | TPM_TOPALIGN | TPM_WORKAREA).0;
            if unsafe { CalculatePopupWindowPosition(&self.anchor, &size, flags, None, &mut frame) }.is_err() {
                return false;
            }
        }
        panel.frame = frame;
        unsafe {
            let _ = SetWindowPos(panel.window, None, frame.left, frame.top, size.cx, size.cy, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        true
    }

    /// Handles a key in the deepest panel open.
    fn press(&mut self, key: usize) -> bool {
        let is = |code: u16| key == code as usize;
        let last = self.panels.len() - 1;
        if (is(VK_LEFT.0) || is(VK_RIGHT.0))
            && let Some(i) = self.panels[last].hover
            && self.panels[last].entries[i].is_slider()
        {
            return self.nudge(last, i, if is(VK_RIGHT.0) { 1 } else { -1 });
        }
        if (is(VK_LEFT.0) || is(VK_RIGHT.0))
            && let Some(i) = self.panels[last].hover
            && self.panels[last].entries[i].tiles() > 0
        {
            let count = self.panels[last].entries[i].tiles();
            let panel = &mut self.panels[last];
            panel.tile = if is(VK_RIGHT.0) { (panel.tile + 1) % count } else { (panel.tile + count - 1) % count };
            return true;
        }
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
        match self.paint(p) {
            Ok(pixels) => {
                self.panels[p].pixels = pixels;
                unsafe {
                    let _ = InvalidateRect(Some(self.panels[p].window), None, false);
                }
            }
            Err(error) => log!("menu paint failed: {error}"),
        }
    }

    fn paint(&self, p: usize) -> Result<Vec<u8>> {
        let size = self.panels[p].size();
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
            let drawn = self.draw(&target, p);
            target.EndDraw(None, None)?;
            drawn?;
            drop(target);
            let mut pixels = vec![0u8; (size.0 * size.1 * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), (size.0 * 4) as u32, &mut pixels)?;
            Ok(pixels)
        }
    }

    fn draw(&self, target: &ID2D1RenderTarget, p: usize) -> Result<()> {
        let panel = &self.panels[p];
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
                    Entry::Header(label) => {
                        let units: Vec<u16> = label.encode_utf16().collect();
                        let layout = self.write.CreateTextLayout(&units, &self.heading, panel.width - panel.label, HEADER)?;
                        target.DrawTextLayout(Vector2 { X: panel.label, Y: top }, &layout, &dim, D2D1_DRAW_TEXT_OPTIONS_NONE);
                        continue;
                    }
                    &Entry::Slider { at, stops, ref caption, shown, .. } => {
                        if !shown {
                            continue;
                        }
                        let state = if self.sliding == Some((p, i)) {
                            Thumb::Pressed
                        } else if panel.hover == Some(i) {
                            Thumb::Hovered
                        } else {
                            Thumb::Resting
                        };
                        self.slider(target, panel, top, at as f32 / (stops - 1).max(1) as f32, caption, state)?;
                        continue;
                    }
                    Entry::Icons { tiles, shown } => {
                        if *shown {
                            self.icons(target, panel, i, tiles)?;
                        }
                        continue;
                    }
                    Entry::Disclosure { label, summary, icons, open, keeps_summary, .. } => {
                        if panel.hover == Some(i) {
                            let rect = D2D_RECT_F { left: PAD, top, right: panel.width - PAD, bottom: top + ROW };
                            target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect, radiusX: ROW_RADIUS, radiusY: ROW_RADIUS }, &hover);
                        }
                        self.glyph(target, if *open { CHEVRON_DOWN } else { CHEVRON }, panel.width - PAD - 14.0, top, &ink);
                        let layout = self.layout(label, panel.width - panel.label - TRAILING / 2.0)?;
                        target.DrawTextLayout(Vector2 { X: panel.label, Y: top }, &layout, &ink, D2D1_DRAW_TEXT_OPTIONS_NONE);
                        if !*open && !icons.is_empty() {
                            let soft = target.CreateSolidColorBrush(&palette.soft, None)?;
                            let scale = SUMMARY_ICON / ICON;
                            let mut right = panel.width - PAD - 28.0;
                            for &icon in icons.iter().rev() {
                                let x = right - SUMMARY_ICON / 2.0;
                                let y = top + ROW / 2.0;
                                target.SetTransform(&Matrix3x2 {
                                    M11: scale,
                                    M12: 0.0,
                                    M21: 0.0,
                                    M22: scale,
                                    M31: x * (1.0 - scale),
                                    M32: y * (1.0 - scale),
                                });
                                self.icon(target, icon, x, y - ICON / 2.0, &soft)?;
                                right -= SUMMARY_ICON + SUMMARY_GAP;
                            }
                            target.SetTransform(&Matrix3x2::identity());
                        } else if !*open || *keeps_summary {
                            let soft = target.CreateSolidColorBrush(&palette.soft, None)?;
                            let layout = self.layout(summary, panel.width - panel.label - PAD - 28.0)?;
                            layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING)?;
                            target.DrawTextLayout(Vector2 { X: panel.label, Y: top }, &layout, &soft, D2D1_DRAW_TEXT_OPTIONS_NONE);
                        }
                        continue;
                    }
                    Entry::Item { label, checked, enabled, radio, .. } => (label.as_str(), *checked, *radio, *enabled),
                    Entry::Submenu { label, .. } => (label.as_str(), false, false, true),
                };
                let heart = matches!(entry, Entry::Item { heart: true, .. });
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
                let shift = if heart {
                    let red = target.CreateSolidColorBrush(&HEART_COLOR, None)?;
                    self.glyph(target, HEART, panel.label + 8.0, top, &red);
                    HEART_SPACE
                } else {
                    0.0
                };
                let layout = self.layout(label, panel.width - panel.label - shift - TRAILING / 2.0)?;
                target.DrawTextLayout(Vector2 { X: panel.label + shift, Y: top }, &layout, brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
        }
        Ok(())
    }

    /// A row of icons, entry `i` of the panel: bright while on, faint while off, each over its label.
    fn icons(&self, target: &ID2D1RenderTarget, panel: &Panel, i: usize, tiles: &[Tile]) -> Result<()> {
        let palette = self.palette;
        let top = panel.tops[i];
        let width = (panel.width - 2.0 * PAD) / tiles.len() as f32;
        unsafe {
            let ink = target.CreateSolidColorBrush(&palette.ink, None)?;
            let dim = target.CreateSolidColorBrush(&palette.dim, None)?;
            let hover = target.CreateSolidColorBrush(&palette.hover, None)?;
            for (k, tile) in tiles.iter().enumerate() {
                let left = PAD + width * k as f32;
                if panel.hover == Some(i) && panel.tile == k {
                    let rect = D2D_RECT_F { left: left + 2.0, top: top + 2.0, right: left + width - 2.0, bottom: top + ICONS - 2.0 };
                    target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect, radiusX: ROW_RADIUS, radiusY: ROW_RADIUS }, &hover);
                }
                let center = left + width / 2.0;
                self.icon(target, tile.icon, center, top + 8.0, if tile.on { &ink } else { &dim })?;
                let units: Vec<u16> = tile.label.encode_utf16().collect();
                let layout = self.write.CreateTextLayout(&units, &self.caption, width, 16.0)?;
                target.DrawTextLayout(
                    Vector2 { X: left, Y: top + ICON + 12.0 },
                    &layout,
                    if tile.on { &ink } else { &dim },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                );
            }
        }
        Ok(())
    }

    /// An icon `ICON` across, centred on `x`, from `y` down, drawn in strokes like the system's line icons.
    fn icon(&self, target: &ID2D1RenderTarget, icon: Icon, x: f32, y: f32, brush: &ID2D1SolidColorBrush) -> Result<()> {
        let half = ICON / 2.0;
        let stroke = 1.5;
        unsafe {
            match icon {
                Icon::Windows => {
                    let rect = D2D_RECT_F { left: x - half, top: y + 2.0, right: x + half, bottom: y + 18.0 };
                    target.DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect, radiusX: 2.5, radiusY: 2.5 }, brush, stroke, None);
                    target.DrawLine(Vector2 { X: x - half, Y: y + 6.5 }, Vector2 { X: x + half, Y: y + 6.5 }, brush, stroke, None);
                }
                Icon::Ground => {
                    let base = y + 16.0;
                    let mound = self.d2d.CreatePathGeometry()?;
                    let sink = mound.Open()?;
                    sink.BeginFigure(Vector2 { X: x - 8.0, Y: base }, D2D1_FIGURE_BEGIN_FILLED);
                    sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: Vector2 { X: x + 8.0, Y: base },
                        size: D2D_SIZE_F { width: 8.0, height: 6.0 },
                        rotationAngle: 0.0,
                        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                        arcSize: D2D1_ARC_SIZE_SMALL,
                    });
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    sink.Close()?;
                    let opacity = brush.GetOpacity();
                    brush.SetOpacity(opacity * 0.3);
                    target.FillGeometry(&mound, brush, None);
                    brush.SetOpacity(opacity);
                    target.DrawGeometry(&mound, brush, stroke, None);
                    target.DrawLine(Vector2 { X: x - half, Y: base }, Vector2 { X: x + half, Y: base }, brush, stroke, None);
                }
            }
        }
        Ok(())
    }

    /// A slider in the row starting at `top`, its thumb `along` the track from 0 to 1, drawn like the system's.
    fn slider(&self, target: &ID2D1RenderTarget, panel: &Panel, top: f32, along: f32, caption: &str, thumb: Thumb) -> Result<()> {
        let palette = self.palette;
        let (left, right) = travel(panel);
        let y = top + 5.0 + THUMB;
        let x = left + (right - left) * along;
        let bar = |from: f32, to: f32| D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: from, top: y - TRACK / 2.0, right: to, bottom: y + TRACK / 2.0 },
            radiusX: TRACK / 2.0,
            radiusY: TRACK / 2.0,
        };
        let dot = match thumb {
            Thumb::Resting => 6.0,
            Thumb::Hovered => 7.0,
            Thumb::Pressed => 5.0,
        };
        let circle = |radius: f32| D2D1_ELLIPSE { point: Vector2 { X: x, Y: y }, radiusX: radius, radiusY: radius };
        unsafe {
            let track = target.CreateSolidColorBrush(&palette.track, None)?;
            let accent = target.CreateSolidColorBrush(&palette.accent, None)?;
            let fill = target.CreateSolidColorBrush(&palette.thumb, None)?;
            let rim = target.CreateSolidColorBrush(&palette.rim, None)?;
            let soft = target.CreateSolidColorBrush(&palette.soft, None)?;
            target.FillRoundedRectangle(&bar(left - THUMB, right + THUMB), &track);
            target.FillRoundedRectangle(&bar(left - THUMB, x), &accent);
            target.FillEllipse(&circle(THUMB), &fill);
            target.DrawEllipse(&circle(THUMB - 0.5), &rim, 1.0, None);
            target.FillEllipse(&circle(dot), &accent);
            if !caption.is_empty() {
                let units: Vec<u16> = caption.encode_utf16().collect();
                let layout = self.write.CreateTextLayout(&units, &self.caption, right - left + 2.0 * THUMB, 16.0)?;
                target.DrawTextLayout(Vector2 { X: left - THUMB, Y: top + 27.0 }, &layout, &soft, D2D1_DRAW_TEXT_OPTIONS_NONE);
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

#[derive(Clone, Copy)]
enum Thumb {
    Resting,
    Hovered,
    Pressed,
}

/// Where a slider's thumb goes in a panel, from the first stop to the last, in DIPs from its left edge.
fn travel(panel: &Panel) -> (f32, f32) {
    (panel.label + THUMB, panel.width - panel.label - THUMB)
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
