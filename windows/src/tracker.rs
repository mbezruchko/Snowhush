use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUNDSMALL,
    DwmGetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::Shell::{
    IVirtualDesktopManager, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState, VirtualDesktopManager,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PWSTR, w};

use snowhush_core::geometry::Rect;
use snowhush_core::simulation::{COLUMN_WIDTH, WindowSurface};

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
    static NOTICED: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
}

struct Window {
    id: isize,
    class: String,
    rect: RECT,
    radius: f32,
    cover_only: bool,
    topmost: bool,
    zoomed: bool,
    toast: bool,
    sweeps: bool,
    hold: bool,
    slide: f32,
    /// The toast card in a toast's window, which is all of the window that shows.
    card: RECT,
    travel: f32,
}

struct Arrival {
    at: i64,
    left: i32,
    right: i32,
    moved: bool,
    model: bool,
    travel: f32,
    samples: u8,
    started: bool,
    /// One full scrape of the toast band has already run.
    swept: bool,
}

/// The rubber band a drag across the desktop's empty space draws to select icons. It has no window of its own, being
/// drawn into the desktop's icon view, so it is followed from the mouse: from where the button went down to the pointer.
#[derive(Default)]
struct Marquee {
    /// Where the button went down, and whether on the desktop's icon view.
    pressed: Option<(POINT, bool)>,
    rect: Option<RECT>,
}

/// A surface id no window has: the rubber band's.
const MARQUEE: isize = isize::MIN + 1;

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
    marquee: Marquee,
    arrivals: HashMap<isize, Arrival>,
    shells: HashMap<u32, bool>,
    animating: bool,
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
            marquee: Marquee::default(),
            arrivals: HashMap::new(),
            shells: HashMap::new(),
            animating: false,
            listings: 0,
            cost: 0,
        }
    }

    /// Re-reads the window list if anything may have changed; returns whether it did.
    pub fn refresh(&mut self, clock: &Clock, now: i64) -> bool {
        let events = EVENTS.get();
        let due = events.changed || clock.seconds(now - self.listed_at) >= 0.5;
        if !due && !self.animating {
            return false;
        }
        EVENTS.set(Events { changed: false, ..events });
        if due {
            let began = Clock::now();
            self.windows = self.list();
            self.busy = SHELL_BUSY.load(Ordering::Relaxed);
            self.cost += Clock::now() - began;
            self.listings += 1;
            self.listed_at = now;
            self.generation += 1;
        }
        self.note_toasts(clock, now);
        true
    }

    /// Follows a rubber band being dragged out on the desktop, from the primary button and the pointer; returns whether
    /// it changed. Dragging an icon instead shows the shell's drag image, and draws no band.
    pub fn follow_marquee(&mut self) -> bool {
        let primary = if unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0 { VK_RBUTTON } else { VK_LBUTTON };
        let held = unsafe { GetAsyncKeyState(primary.0 as i32) } as u16 & 0x8000 != 0;
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) }.is_err() {
            return false;
        }
        let before = self.marquee.rect;
        match self.marquee.pressed {
            _ if !held => self.marquee = Marquee::default(),
            None => self.marquee.pressed = Some((cursor, on_desktop(cursor))),
            Some((anchor, true)) => {
                let slack = unsafe { (GetSystemMetrics(SM_CXDRAG), GetSystemMetrics(SM_CYDRAG)) };
                let dragged = (cursor.x - anchor.x).abs() > slack.0 || (cursor.y - anchor.y).abs() > slack.1;
                let icon = unsafe { FindWindowW(w!("SysDragImage"), None) }.is_ok_and(|image| unsafe { IsWindowVisible(image) }.as_bool());
                self.marquee.rect = (dragged && !icon).then(|| RECT {
                    left: anchor.x.min(cursor.x),
                    top: anchor.y.min(cursor.y),
                    right: anchor.x.max(cursor.x),
                    bottom: anchor.y.max(cursor.y),
                });
            }
            Some((_, false)) => {}
        }
        let changed = self.marquee.rect != before;
        if changed {
            self.generation += 1;
        }
        changed
    }

    /// Forgets the rubber band, as when it is no longer followed.
    pub fn drop_marquee(&mut self) {
        if std::mem::take(&mut self.marquee).rect.is_some() {
            self.generation += 1;
        }
    }

    pub fn animating(&self) -> bool {
        self.animating
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
            // A toast carries no snow; only its card hides what lies behind it.
            if window.toast {
                blockers.extend(local_frame(&window.card, area, size));
                continue;
            }
            if room_above && on_screen && !fills_screen && !window.cover_only && frame.width >= 80.0 && frame.height >= 40.0 {
                let radius = window.radius.min(frame.width / 2.0).min(frame.height / 2.0);
                let region = Rect { x: frame.x, y: frame.max_y() - radius, width: frame.width, height: 40.0 + radius };
                surfaces.push(WindowSurface {
                    id: window.id,
                    frame,
                    blockers: blockers.iter().filter(|b| intersects(b, &region)).copied().collect(),
                    cover: blockers.iter().filter(|b| intersects(b, &frame)).copied().collect(),
                    corner_radius: radius,
                    corner_exponent: 2.0,
                });
            }
            // Alt+Tab's switcher sits in a clear window the size of the display; it doesn't hide the snow on windows.
            if !(window.cover_only && fills_screen) {
                blockers.push(frame);
            }
        }
        // The rubber band lies on the desktop, behind every window, and hides nothing: it is see-through. However thin,
        // its top edge holds snow, as wide as the simulation keeps a ledge.
        if let Some(frame) = self.marquee.rect.as_ref().and_then(|rect| local_frame(rect, area, size))
            && frame.width >= 8.0 * COLUMN_WIDTH
            && frame.max_y() < height
        {
            let region = Rect { x: frame.x, y: frame.max_y(), width: frame.width, height: 40.0 };
            surfaces.push(WindowSurface {
                id: MARQUEE,
                frame,
                blockers: blockers.iter().filter(|b| intersects(b, &region)).copied().collect(),
                cover: blockers.iter().filter(|b| intersects(b, &frame)).copied().collect(),
                corner_radius: 0.0,
                corner_exponent: 2.0,
            });
        }
        surfaces
    }

    /// The cards of the toasts sliding in on one display, in its overlay coordinates.
    pub fn toast_sweeps(&self, area: &RECT, size: (u32, u32)) -> Vec<(isize, Rect)> {
        let sx = size.0 as f32 / (area.right - area.left).max(1) as f32;
        self.windows
            .iter()
            .filter(|window| window.toast && window.sweeps)
            .filter_map(|window| {
                let card = local_frame(&window.card, area, size)?;
                // Windows puts the toast's window where it ends up at once, so its slide in is modelled.
                Some((window.id, Rect { x: card.x + window.slide * sx, ..card }))
            })
            .collect()
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
        NOTICED.with(|noticed| {
            let mut noticed = noticed.borrow_mut();
            noticed.retain(|window| unsafe { IsWindow(Some(*window)).as_bool() });
            for window in noticed.iter().rev() {
                if !handles.contains(window) {
                    handles.insert(0, *window);
                }
            }
        });
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
            let mut rect = frame(window)?;
            if class == POPUP_BRIDGE {
                rect = popup_panel(window, rect);
            }
            if rect.right - rect.left < 2 || rect.bottom - rect.top < 2 {
                return None;
            }
            let id = window.0 as isize;
            if self.seen.len() > 4096 {
                self.seen.clear();
            }
            let toast = self.toast_banner(window, &class, process, &rect) && near_right(window, &rect);
            if self.seen.insert(id) {
                log!(
                    "window {class} of {}: {}x{} at {},{}{}",
                    process_name(process),
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    rect.left,
                    rect.top,
                    if toast { " toast" } else { "" }
                );
            }
            let zoomed = IsZoomed(window).as_bool();
            let radius = if self.rounded && !zoomed { corner_radius(window) } else { 0.0 };
            let scale = if toast { GetDpiForWindow(window).max(96) as f32 / 96.0 } else { 1.0 };
            Some(Window {
                id,
                cover_only: COVER_ONLY.contains(&class.as_str()),
                class,
                rect,
                radius,
                topmost: extended & WS_EX_TOPMOST.0 != 0,
                zoomed,
                toast,
                sweeps: false,
                hold: false,
                slide: 0.0,
                card: if toast { toast_card(&rect, scale) } else { rect },
                travel: if toast { (rect.right - rect.left) as f32 + TOAST_MARGIN.0 * scale } else { 0.0 },
            })
        }
    }

    fn toast_banner(&mut self, window: HWND, class: &str, process: u32, rect: &RECT) -> bool {
        if class != "Windows.UI.Core.CoreWindow" || !self.shell(process) {
            return false;
        }
        let dpi = unsafe { GetDpiForWindow(window) }.max(96) as i32;
        let width = (rect.right - rect.left) * 96 / dpi;
        let height = (rect.bottom - rect.top) * 96 / dpi;
        (200..=600).contains(&width) && (48..=440).contains(&height)
    }

    fn shell(&mut self, process: u32) -> bool {
        if let Some(&known) = self.shells.get(&process) {
            return known;
        }
        let name = process_name(process);
        let shell = name.eq_ignore_ascii_case("ShellExperienceHost.exe") || name.eq_ignore_ascii_case("ShellHost.exe");
        if self.shells.len() > 64 {
            self.shells.clear();
        }
        self.shells.insert(process, shell);
        shell
    }

    fn note_toasts(&mut self, clock: &Clock, now: i64) {
        let animate = motion_enabled();
        self.animating = false;
        let mut alive = HashSet::new();
        for i in 0..self.windows.len() {
            if !self.windows[i].toast {
                self.windows[i].sweeps = false;
                self.windows[i].hold = false;
                self.windows[i].slide = 0.0;
                continue;
            }
            let id = self.windows[i].id;
            let rect = self.windows[i].rect;
            let travel = self.windows[i].travel;
            alive.insert(id);
            let (hold, slide, sweeps) = {
                let arrival = self.arrivals.entry(id).or_insert(Arrival {
                    at: now,
                    left: rect.left,
                    right: rect.right,
                    moved: false,
                    model: false,
                    travel,
                    samples: 0,
                    started: false,
                    swept: false,
                });
                if arrival.samples == 0 {
                    // Windows usually parks the toast at its final rect; always model a slide when motion is on
                    // so the shove spreads instead of dumping one cliff against the left edge.
                    arrival.model = animate;
                    arrival.travel = travel;
                    arrival.left = rect.left;
                    arrival.right = rect.right;
                }
                arrival.samples = arrival.samples.saturating_add(1);
                let shifted = (rect.left - arrival.left).abs() > 2 || (rect.right - arrival.right).abs() > 2;
                if shifted {
                    arrival.moved = true;
                    arrival.left = rect.left;
                    arrival.right = rect.right;
                }
                if arrival.model && !arrival.moved {
                    if arrival.samples < 2 {
                        (true, 0.0, false)
                    } else {
                        if !arrival.started {
                            arrival.started = true;
                            arrival.at = now;
                        }
                        let offset = arrival.travel * (-(clock.seconds(now - arrival.at) as f32) / 0.064).exp();
                        let slide = if offset < 0.5 { 0.0 } else { offset };
                        if slide <= 0.5 {
                            arrival.swept = true;
                        }
                        (false, slide, slide > 0.5)
                    }
                } else {
                    // Toast already sitting still: scrape the whole band once so no drift stays behind it.
                    let pending = !arrival.swept;
                    if pending {
                        arrival.swept = true;
                    }
                    (false, 0.0, pending || shifted)
                }
            };
            self.windows[i].hold = hold;
            self.windows[i].slide = slide;
            self.windows[i].sweeps = sweeps;
            if hold || slide > 0.0 || sweeps {
                self.animating = true;
            }
        }
        self.arrivals.retain(|id, _| alive.contains(id));
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
            events.changed = true;
            if class_name(window) == "Windows.UI.Core.CoreWindow" {
                NOTICED.with(|noticed| {
                    let mut noticed = noticed.borrow_mut();
                    if !noticed.contains(&window) {
                        noticed.push(window);
                    }
                });
            }
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

static SHELL_WATCHED: AtomicBool = AtomicBool::new(true);
static SHELL_WATCHER: OnceLock<std::thread::Thread> = OnceLock::new();

/// Asking the shell can stall for a fifth of a second while it is busy, sliding a hidden taskbar in or out for one,
/// so it is asked on a thread of its own rather than between frames.
fn watch_shell() {
    static START: Once = Once::new();
    START.call_once(|| {
        let _ = std::thread::Builder::new().name("shell state".into()).spawn(|| {
            let _ = SHELL_WATCHER.set(std::thread::current());
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            loop {
                if !SHELL_WATCHED.load(Ordering::Relaxed) {
                    std::thread::park();
                    continue;
                }
                let busy = unsafe { SHQueryUserNotificationState() }
                    .is_ok_and(|state| matches!(state, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE));
                SHELL_BUSY.store(busy, Ordering::Relaxed);
                std::thread::park_timeout(Duration::from_millis(250));
            }
        });
    });
}

/// Keeps asking the shell only while the snow is drawn: locked or asleep, there is no full-screen app to keep out of.
pub fn watch_shell_state(on: bool) {
    SHELL_WATCHED.store(on, Ordering::Relaxed);
    if on && let Some(watcher) = SHELL_WATCHER.get() {
        watcher.unpark();
    }
}

pub fn cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe { DwmGetWindowAttribute(window, DWMWA_CLOAKED, (&raw mut cloaked).cast(), 4) }.is_ok() && cloaked != 0
}

/// Explorer's menus on the Windows App SDK: their window takes in a clear margin around the panel for its shadow, and
/// neither the window nor any child of it tells where the panel is.
const POPUP_BRIDGE: &str = "Microsoft.UI.Content.PopupWindowSiteBridge";

/// The panel inside such a menu's window, its margin measured at 100% (left, top, right, bottom) and scaled to the window's.
fn popup_panel(window: HWND, rect: RECT) -> RECT {
    const MARGIN: [f32; 4] = [10.0, 3.0, 10.0, 10.0];
    let scale = unsafe { GetDpiForWindow(window) } as f32 / 96.0;
    let inset = |side: usize| (MARGIN[side] * scale).round() as i32;
    RECT { left: rect.left + inset(0), top: rect.top + inset(1), right: rect.right - inset(2), bottom: rect.bottom - inset(3) }
}

/// Whether the desktop's icon view is under this point, its empty space or an icon.
fn on_desktop(point: POINT) -> bool {
    unsafe {
        let view = WindowFromPoint(point);
        if view.is_invalid() || class_name(view) != "SysListView32" {
            return false;
        }
        let Ok(shell) = GetParent(view) else { return false };
        let Ok(host) = GetParent(shell) else { return false };
        class_name(shell) == "SHELLDLL_DefView" && matches!(class_name(host).as_str(), "Progman" | "WorkerW")
    }
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

pub fn windows_build() -> u32 {
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
pub fn local_frame(r: &RECT, area: &RECT, size: (u32, u32)) -> Option<Rect> {
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

/// The room ShellExperienceHost's window leaves around the toast card in it, at the sides, above and below, in pixels
/// at 96 dpi: measured on Windows 11.
const TOAST_MARGIN: (f32, f32, f32) = (16.0, 30.0, 15.0);

fn toast_card(rect: &RECT, scale: f32) -> RECT {
    let (side, above, below) = TOAST_MARGIN;
    let side = (side * scale).round() as i32;
    RECT {
        left: rect.left + side,
        top: rect.top + (above * scale).round() as i32,
        right: rect.right - side,
        bottom: rect.bottom - (below * scale).round() as i32,
    }
}

fn near_right(window: HWND, rect: &RECT) -> bool {
    unsafe {
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return true;
        }
        let work = info.rcWork;
        (work.right - rect.right).abs() <= 48
    }
}

fn motion_enabled() -> bool {
    unsafe {
        let mut enabled = BOOL(0);
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some((&raw mut enabled).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok()
            && enabled.as_bool()
    }
}
