use std::cell::Cell;
use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUNDSMALL,
    DwmGetWindowAttribute,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Shell::{
    IVirtualDesktopManager, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState, VirtualDesktopManager,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PWSTR, w};

use snowhush_core::geometry::Rect;
use snowhush_core::simulation::WindowSurface;

use crate::log;
use crate::screen::Clock;

/// Shell windows that are never solid surfaces: the desktop, the taskbar, shadows and tooltips.
const IGNORED: [&str; 10] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "tooltips_class32",
    "SysShadow",
    "EdgeUiInputTopWndClass",
    "EdgeUiInputWndClass",
    "ApplicationManager_ImmersiveShellWindow",
    "Internet Explorer_Hidden",
];
/// Switchers, thumbnails and the shell's flyouts (hidden tray icons, context menus, Start and the like) hide snow behind
/// them but don't collect any. A flyout's rectangle also takes in a clear margin for its shadow, well outside what shows.
pub const COVER_ONLY: [&str; 8] = [
    "XamlExplorerHostIslandWindow",
    "MultitaskingViewFrame",
    "TaskListThumbnailWnd",
    "ForegroundStaging",
    "TopLevelWindowForOverflowXamlIsland",
    "NotifyIconOverflowWindow",
    "Xaml_WindowedPopupClass",
    "Windows.UI.Core.CoreWindow",
];

#[derive(Clone, Copy, Default)]
struct Events {
    changed: bool,
    moving: bool,
    started_moving: bool,
}

/// The shell's own sense that a full-screen app or presentation is in front, kept up to date by `watch_shell`.
static SHELL_BUSY: AtomicBool = AtomicBool::new(false);

thread_local! {
    static EVENTS: Cell<Events> = const { Cell::new(Events { changed: true, moving: false, started_moving: false }) };
}

struct Window {
    id: isize,
    class: String,
    /// The visible frame in physical pixels, without the invisible resize border.
    rect: RECT,
    radius: f32,
    cover_only: bool,
    topmost: bool,
    zoomed: bool,
}

/// The windows on screen, front to back, re-read when the system reports a window moving, showing, hiding,
/// minimizing or changing desktops, and twice a second in case a report went missing.
pub struct Tracker {
    hooks: Vec<HWINEVENTHOOK>,
    desktops: Option<IVirtualDesktopManager>,
    rounded: bool,
    own_process: u32,
    windows: Vec<Window>,
    listed_at: i64,
    /// The shell's own sense that a full-screen app or presentation is in front.
    busy: bool,
    pub generation: u64,
    seen: HashSet<isize>,
    pub listings: u32,
    pub cost: i64,
}

impl Tracker {
    pub fn new() -> Tracker {
        let ranges = [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ];
        let hooks = ranges
            .iter()
            .map(|&(first, last)| unsafe {
                SetWinEventHook(first, last, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)
            })
            .filter(|hook| !hook.is_invalid())
            .collect();
        watch_shell();
        Tracker {
            hooks,
            desktops: unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) }.ok(),
            rounded: windows_build() >= 22000,
            own_process: std::process::id(),
            windows: Vec::new(),
            listed_at: 0,
            busy: false,
            generation: 0,
            seen: HashSet::new(),
            listings: 0,
            cost: 0,
        }
    }

    /// Re-reads the window list if anything may have changed; returns whether it did.
    pub fn refresh(&mut self, clock: &Clock, now: i64) -> bool {
        let events = EVENTS.get();
        if !events.changed && clock.seconds(now - self.listed_at) < 0.5 {
            return false;
        }
        EVENTS.set(Events { changed: false, ..events });
        let began = Clock::now();
        self.windows = self.list();
        self.busy = SHELL_BUSY.load(Ordering::Relaxed);
        self.cost += Clock::now() - began;
        self.listings += 1;
        self.listed_at = now;
        self.generation += 1;
        true
    }

    /// Whether a window is being dragged or resized right now.
    pub fn moving(&self) -> bool {
        EVENTS.get().moving
    }

    /// Whether a drag or resize started since the last call.
    pub fn take_started_moving(&self) -> bool {
        let events = EVENTS.get();
        EVENTS.set(Events { started_moving: false, ..events });
        events.started_moving
    }

    /// The windows on one display that snow can settle on, in its overlay coordinates.
    /// `area` is the display's work area in physical pixels, `size` the overlay's size in DIPs.
    pub fn surfaces(&self, area: &RECT, size: (u32, u32)) -> Vec<WindowSurface> {
        let (width, height) = (size.0 as f32, size.1 as f32);
        let sy = height / (area.bottom - area.top).max(1) as f32;
        let mut blockers: Vec<Rect> = Vec::new();
        let mut surfaces = Vec::new();
        for window in &self.windows {
            let Some(frame) = local_frame(&window.rect, area, size) else { continue };
            let r = &window.rect;
            let room_above = (r.top - area.top) as f32 * sy >= 6.0 && frame.max_y() < height;
            let on_screen = frame.max_x() > 0.0 && frame.min_x() < width && frame.max_y() > 0.0;
            let fills_screen = frame.width * frame.height >= width * height * 0.9;
            if room_above && on_screen && !fills_screen && !window.cover_only && frame.width >= 80.0 && frame.height >= 40.0 {
                let radius = window.radius.min(frame.width / 2.0).min(frame.height / 2.0);
                let region = Rect { x: frame.x, y: frame.max_y() - radius, width: frame.width, height: 40.0 + radius };
                surfaces.push(WindowSurface {
                    id: window.id,
                    frame,
                    blockers: blockers.iter().filter(|b| intersects(b, &region)).copied().collect(),
                    corner_radius: radius,
                    corner_exponent: 2.0,
                });
            }
            // Alt+Tab's switcher sits in a clear window the size of the display; it doesn't hide the snow on windows.
            if !(window.cover_only && fills_screen) {
                blockers.push(frame);
            }
        }
        surfaces
    }

    /// Every app window on one display, maximized and full-screen ones too, left, bottom, right, top in its overlay
    /// coordinates: a pumpkin hides from any that covers it. Open menus and the shell's switchers and flyouts don't count.
    pub fn covers(&self, area: &RECT, size: (u32, u32)) -> Vec<[f32; 4]> {
        self.windows
            .iter()
            .filter(|window| !window.cover_only && window.class != "#32768")
            .filter_map(|window| local_frame(&window.rect, area, size))
            .map(|frame| [frame.min_x(), frame.min_y(), frame.max_x(), frame.max_y()])
            .collect()
    }

    /// The full-screen window covering the display with these bounds, if any: the frontmost window on it,
    /// looking past small always-on-top ones and the shell's switchers and flyouts, fills it entirely. A maximized
    /// window counts only when the shell also sees a full-screen app, since it fills the display too wherever the
    /// taskbar hides itself.
    pub fn covering(&self, bounds: &RECT) -> Option<&str> {
        let foreground = unsafe { GetForegroundWindow() }.0 as isize;
        for window in &self.windows {
            let r = &window.rect;
            if window.cover_only || r.right <= bounds.left || r.left >= bounds.right || r.bottom <= bounds.top || r.top >= bounds.bottom {
                continue;
            }
            let fills = r.left <= bounds.left && r.top <= bounds.top && r.right >= bounds.right && r.bottom >= bounds.bottom;
            if fills && (!window.zoomed || (self.busy && window.id == foreground)) {
                return Some(&window.class);
            }
            if !window.topmost {
                return None;
            }
        }
        None
    }

    /// Whether a listed window covers most of this display's work area, in physical pixels, as when maximized or full screen.
    pub fn fills(&self, id: isize, area: &RECT) -> bool {
        self.windows.iter().any(|window| {
            let r = &window.rect;
            let width = (r.right.min(area.right) - r.left.max(area.left)).max(0) as i64;
            let height = (r.bottom.min(area.bottom) - r.top.max(area.top)).max(0) as i64;
            window.id == id && width * height * 10 >= (area.right - area.left) as i64 * (area.bottom - area.top) as i64 * 9
        })
    }

    /// Whether a window that left the list moved to another virtual desktop, rather than closing or hiding.
    pub fn elsewhere(&self, id: isize) -> bool {
        let window = HWND(id as *mut c_void);
        unsafe {
            IsWindow(Some(window)).as_bool()
                && cloaked(window)
                && self.desktops.as_ref().is_some_and(|desktops| desktops.IsWindowOnCurrentVirtualDesktop(window).is_ok_and(|on| !on.as_bool()))
        }
    }

    fn list(&mut self) -> Vec<Window> {
        unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
            unsafe { (*(list.0 as *mut Vec<HWND>)).push(window) };
            true.into()
        }
        let mut handles: Vec<HWND> = Vec::with_capacity(512);
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&raw mut handles as isize));
        }
        handles.into_iter().filter_map(|window| self.describe(window)).collect()
    }

    fn describe(&mut self, window: HWND) -> Option<Window> {
        unsafe {
            if !IsWindowVisible(window).as_bool() || IsIconic(window).as_bool() {
                return None;
            }
            let mut process = 0;
            GetWindowThreadProcessId(window, Some(&mut process));
            if process == self.own_process || cloaked(window) {
                return None;
            }
            let extended = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
            // Click-through windows are overlays like this one, not something snow could rest on.
            if extended & WS_EX_TRANSPARENT.0 != 0 {
                return None;
            }
            // A layered window drawn with per-pixel alpha, or cut out by a colour key, isn't the shape of its rectangle:
            // desktop widgets like Rainmeter skins are mostly empty space. The attributes can't be read for per-pixel ones.
            if extended & WS_EX_LAYERED.0 != 0 {
                let mut alpha = 255u8;
                let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
                if GetLayeredWindowAttributes(window, None, Some(&mut alpha), Some(&mut flags)).is_err()
                    || flags.contains(LWA_COLORKEY)
                    || (flags.contains(LWA_ALPHA) && alpha < 77)
                {
                    return None;
                }
            }
            let class = class_name(window);
            if IGNORED.contains(&class.as_str()) {
                return None;
            }
            let rect = frame(window)?;
            if rect.right - rect.left < 2 || rect.bottom - rect.top < 2 {
                return None;
            }
            let id = window.0 as isize;
            if self.seen.len() > 4096 {
                self.seen.clear();
            }
            if self.seen.insert(id) {
                log!(
                    "window {class} of {}: {}x{} at {},{}",
                    process_name(process),
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    rect.left,
                    rect.top
                );
            }
            let zoomed = IsZoomed(window).as_bool();
            let radius = if self.rounded && !zoomed { corner_radius(window) } else { 0.0 };
            Some(Window {
                id,
                cover_only: COVER_ONLY.contains(&class.as_str()),
                class,
                rect,
                radius,
                topmost: extended & WS_EX_TOPMOST.0 != 0,
                zoomed,
            })
        }
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        for hook in &self.hooks {
            unsafe {
                let _ = UnhookWinEvent(*hook);
            }
        }
    }
}

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, event: u32, window: HWND, object: i32, child: i32, _: u32, _: u32) {
    let mut events = EVENTS.get();
    match event {
        EVENT_SYSTEM_MOVESIZESTART => {
            events.moving = true;
            events.started_moving = true;
            events.changed = true;
        }
        EVENT_SYSTEM_MOVESIZEEND => {
            events.moving = false;
            events.changed = true;
        }
        // A taskbar set to hide itself moves on every frame of its slide, and the list leaves it out anyway.
        _ if object == OBJID_WINDOW.0 && child == 0 && is_top_level(window, event) && !IGNORED.contains(&class_name(window).as_str()) => {
            events.changed = true
        }
        _ => return,
    }
    EVENTS.set(events);
}

/// Location changes come for every child control and caret; only a top-level window moving matters.
/// A window gone by the time the report arrives counts, since it may have been one with snow on it.
fn is_top_level(window: HWND, event: u32) -> bool {
    unsafe {
        if !IsWindow(Some(window)).as_bool() {
            return event != EVENT_OBJECT_LOCATIONCHANGE;
        }
        GetAncestor(window, GA_ROOT) == window
    }
}

/// Asking the shell can stall for a fifth of a second while it is busy, sliding a hidden taskbar in or out for one,
/// so it is asked on a thread of its own rather than between frames.
fn watch_shell() {
    static START: Once = Once::new();
    START.call_once(|| {
        let _ = std::thread::Builder::new().name("shell state".into()).spawn(|| {
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            loop {
                let busy = unsafe { SHQueryUserNotificationState() }
                    .is_ok_and(|state| matches!(state, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE));
                SHELL_BUSY.store(busy, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    });
}

pub fn cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe { DwmGetWindowAttribute(window, DWMWA_CLOAKED, (&raw mut cloaked).cast(), 4) }.is_ok() && cloaked != 0
}

fn frame(window: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    unsafe {
        if DwmGetWindowAttribute(window, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut rect).cast(), size_of::<RECT>() as u32).is_ok() {
            return Some(rect);
        }
        GetWindowRect(window, &mut rect).ok().map(|_| rect)
    }
}

/// Windows 11 rounds window corners to 8 DIPs unless the app asks otherwise; maximized windows stay square.
fn corner_radius(window: HWND) -> f32 {
    let mut preference = DWM_WINDOW_CORNER_PREFERENCE(0);
    let _ = unsafe { DwmGetWindowAttribute(window, DWMWA_WINDOW_CORNER_PREFERENCE, (&raw mut preference).cast(), 4) };
    match preference {
        DWMWCP_DONOTROUND => 0.0,
        DWMWCP_ROUNDSMALL => 4.0,
        _ => 8.0,
    }
}

pub fn class_name(window: HWND) -> String {
    let mut buffer = [0u16; 128];
    let length = unsafe { GetClassNameW(window, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..length])
}

pub fn process_name(process: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) else { return format!("pid {process}") };
        let mut buffer = [0u16; 512];
        let mut length = buffer.len() as u32;
        let name = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length).ok().map(|_| {
            let path = String::from_utf16_lossy(&buffer[..length as usize]);
            path.rsplit('\\').next().unwrap_or_default().to_string()
        });
        let _ = CloseHandle(handle);
        name.unwrap_or_else(|| format!("pid {process}"))
    }
}

fn windows_build() -> u32 {
    let mut buffer = [0u16; 32];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return 0;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(0);
    String::from_utf16_lossy(&buffer[..length]).parse().unwrap_or(0)
}

/// A window's frame in the overlay coordinates of the display with work area `area` and an overlay `size` DIPs across,
/// if any of it is on that display.
fn local_frame(r: &RECT, area: &RECT, size: (u32, u32)) -> Option<Rect> {
    if r.right <= area.left || r.left >= area.right || r.bottom <= area.top || r.top >= area.bottom {
        return None;
    }
    let sx = size.0 as f32 / (area.right - area.left).max(1) as f32;
    let sy = size.1 as f32 / (area.bottom - area.top).max(1) as f32;
    Some(Rect {
        x: (r.left - area.left) as f32 * sx,
        y: (area.bottom - r.bottom) as f32 * sy,
        width: (r.right - r.left) as f32 * sx,
        height: (r.bottom - r.top) as f32 * sy,
    })
}

fn intersects(a: &Rect, b: &Rect) -> bool {
    a.min_x() < b.max_x() && b.min_x() < a.max_x() && a.min_y() < b.max_y() && b.min_y() < a.max_y()
}
