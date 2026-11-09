use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use crate::halloween;
use crate::log;
use crate::menu::{self, Entry, Icon, Tile};
use crate::prefs::{self, Settings};
use crate::snow::{self, Command, Links};
use crate::update::{self, Release};
use crate::welcome::{self, Handlers};
use snowhush_core::simulation::Mode;
use snowhush_core::text::{self, tr};

const CLASS: PCWSTR = w!("SnowhushController");
const WM_TRAY: u32 = WM_APP + 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
/// Intensity from 0 up in steps of 5%: `ID_SNOWFALL` + the step.
const ID_SNOWFALL: u32 = 100;
const INTENSITY_STEPS: u32 = 21;
/// The system's menu can't hold a slider: its Intensity submenu offers these, in percent.
const INTENSITY_STOPS: [u32; 5] = [0, 25, 50, 75, 100];
const ID_ACCUMULATE: u32 = 200;
const ID_MELT: u32 = 201;
const ID_SCREENSHOTS: u32 = 202;
const ID_LOGIN: u32 = 203;
const ID_WINDOWS: u32 = 204;
const ID_BEHIND: u32 = 205;
const ID_QUIT: u32 = 299;
const ID_UPDATES: u32 = 206;
const ID_CLEAR: u32 = 207;
const ID_HALLOWEEN: u32 = 208;
const ID_TASKBAR: u32 = 209;
const ID_SNOW: u32 = 210;
const ID_AUTUMN: u32 = 211;
const ID_FULL_SCREEN: u32 = 212;
const ID_RAIN: u32 = 213;
const ID_MAX_PERFORMANCE: u32 = 215;
const ID_DEEP: u32 = 216;
const ID_BLOSSOM: u32 = 217;
const ID_SPONSOR: u32 = 218;
const ID_MIXED: u32 = 219;
const ID_GROUND: u32 = 220;
const ID_OPACITY_FOLD: u32 = 221;
/// Opacity from 10% up in steps of 5%: `ID_OPACITY` + the step.
const ID_OPACITY: u32 = 400;
const OPACITY_STEPS: u32 = 19;
/// The system's menu can't hold a slider: its Opacity submenu offers these, in percent.
const OPACITY_STOPS: [u32; 5] = [100, 80, 60, 40, 20];

const SPONSORS: &str = "https://github.com/sponsors/mbezruchko";
const TIMER_WELCOME: usize = 1;
const TIMER_UPDATES: usize = 2;
const TIMER_HALLOWEEN: usize = 3;
const WM_UPDATE: u32 = WM_APP + 2;
/// The shell's message for a click on the tray notification.
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;
const DAY: u32 = 24 * 60 * 60 * 1000;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// A glyph drawn like the Mac's menu bar ones, at the sizes the tray draws it at.
macro_rules! glyph {
    ($name:literal) => {
        [
            (16, include_bytes!(concat!("../assets/tray/", $name, "-16.png")).as_slice()),
            (20, include_bytes!(concat!("../assets/tray/", $name, "-20.png")).as_slice()),
            (24, include_bytes!(concat!("../assets/tray/", $name, "-24.png")).as_slice()),
            (32, include_bytes!(concat!("../assets/tray/", $name, "-32.png")).as_slice()),
            (40, include_bytes!(concat!("../assets/tray/", $name, "-40.png")).as_slice()),
            (48, include_bytes!(concat!("../assets/tray/", $name, "-48.png")).as_slice()),
            (64, include_bytes!(concat!("../assets/tray/", $name, "-64.png")).as_slice()),
        ]
    };
}

type Glyph = [(i32, &'static [u8]); 7];
const SNOWFLAKE_WHITE: Glyph = glyph!("snowflake-white");
const SNOWFLAKE_BLACK: Glyph = glyph!("snowflake-black");
const LEAF_WHITE: Glyph = glyph!("leaf-white");
const LEAF_BLACK: Glyph = glyph!("leaf-black");
const PUMPKIN_WHITE: Glyph = glyph!("pumpkin-white");
const PUMPKIN_BLACK: Glyph = glyph!("pumpkin-black");
const RAIN_WHITE: Glyph = glyph!("rain-white");
const RAIN_BLACK: Glyph = glyph!("rain-black");
const BLOSSOM_WHITE: Glyph = glyph!("blossom-white");
const BLOSSOM_BLACK: Glyph = glyph!("blossom-black");

/// What the tray shows: a snowflake, a jack-o'-lantern while it is Halloween and wanted, a leaf while leaves fall, a
/// rain cloud while it rains, or a cherry blossom while its petals fall.
#[derive(Clone, Copy, PartialEq)]
enum Emblem {
    Snowflake,
    Pumpkin,
    Leaf,
    Rain,
    Blossom,
}

impl Emblem {
    fn of(settings: &Settings) -> Emblem {
        if halloween::shows(settings) {
            Emblem::Pumpkin
        } else {
            match settings.mode {
                Mode::Snow => Emblem::Snowflake,
                Mode::Autumn => Emblem::Leaf,
                Mode::Rain => Emblem::Rain,
                Mode::Blossom => Emblem::Blossom,
            }
        }
    }

    /// Black on a light taskbar, white on a dark one, as the system's own tray icons.
    fn glyph(self, light: bool) -> &'static Glyph {
        match (self, light) {
            (Emblem::Snowflake, false) => &SNOWFLAKE_WHITE,
            (Emblem::Snowflake, true) => &SNOWFLAKE_BLACK,
            (Emblem::Leaf, false) => &LEAF_WHITE,
            (Emblem::Leaf, true) => &LEAF_BLACK,
            (Emblem::Pumpkin, false) => &PUMPKIN_WHITE,
            (Emblem::Pumpkin, true) => &PUMPKIN_BLACK,
            (Emblem::Rain, false) => &RAIN_WHITE,
            (Emblem::Rain, true) => &RAIN_BLACK,
            (Emblem::Blossom, false) => &BLOSSOM_WHITE,
            (Emblem::Blossom, true) => &BLOSSOM_BLACK,
        }
    }
}

/// Whether the taskbar is light: the setting for "your default Windows mode".
fn light_taskbar() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast::<c_void>()),
            Some(&mut size),
        )
    };
    status.is_ok() && value == 1
}

struct App {
    settings: Settings,
    commands: Sender<Command>,
    wake: HANDLE,
    snow: Option<JoinHandle<()>>,
    icon: HICON,
    emblem: Emblem,
    light: bool,
    locked: bool,
    display_off: bool,
    away: bool,
    /// The download of the update the tray last announced.
    update: Option<String>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// Whether Accumulate shows its row of icons, for as long as the app runs.
    static ACCUMULATE_OPEN: Cell<bool> = const { Cell::new(false) };
    /// Whether Opacity shows its slider, for as long as the app runs.
    static OPACITY_OPEN: Cell<bool> = const { Cell::new(false) };
}

/// The tray icon and its menu, and the system events that pause the snow, on the main thread;
/// the snow itself runs on its own thread so menus and other modal loops never stall it.
pub fn run() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        let window = CreateWindowExW(WINDOW_EX_STYLE(0), CLASS, w!("Snowhush"), WS_OVERLAPPED, 0, 0, 0, 0, None, None, Some(instance.into()), None)?;
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

        let settings = Settings::load();
        let (commands, receiver) = mpsc::channel();
        let wake = CreateEventW(None, false, false, None)?;
        let links = Links { wake: wake.0 as isize, controller: window.0 as isize };
        let snow = thread::Builder::new().name("snow".into()).spawn(move || snow::run(receiver, links, settings)).ok();
        let emblem = Emblem::of(&settings);
        let light = light_taskbar();
        let icon = tray_icon(emblem, light);
        add_tray_icon(window, icon);
        let _ = WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);
        let _ = RegisterPowerSettingNotification(HANDLE(window.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);
        if !settings.welcomed {
            SetTimer(Some(window), TIMER_WELCOME, 1200, None);
        }
        SetTimer(Some(window), TIMER_UPDATES, 30_000, None);
        SetTimer(Some(window), TIMER_HALLOWEEN, 60_000, None);
        APP.set(Some(App { settings, commands, wake, snow, icon, emblem, light, locked: false, display_off: false, away: false, update: None }));

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TRAY => {
            let event = (lparam.0 & 0xffff) as u32;
            if event == WM_CONTEXTMENU || event == NIN_SELECT || event == NIN_KEYSELECT {
                let x = (wparam.0 & 0xffff) as i16 as i32;
                let y = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
                show_menu(window, x, y);
            } else if event == NIN_BALLOONUSERCLICK
                && let Some(url) = APP.with_borrow(|app| app.as_ref().and_then(|app| app.update.clone()))
            {
                update::open(&url);
            }
            LRESULT(0)
        }
        WM_WTSSESSION_CHANGE => {
            update(|app| match wparam.0 as u32 {
                WTS_SESSION_LOCK => app.locked = true,
                WTS_SESSION_UNLOCK => app.locked = false,
                WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => app.away = true,
                WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT => app.away = false,
                _ => {}
            });
            LRESULT(0)
        }
        WM_POWERBROADCAST if wparam.0 as u32 == PBT_POWERSETTINGCHANGE => {
            let setting = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
            if setting.PowerSetting == GUID_CONSOLE_DISPLAY_STATE {
                let off = setting.Data[0] == 0;
                update(|app| app.display_off = off);
            }
            LRESULT(1)
        }
        WM_CLOSE => {
            quit(window);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_UPDATES => {
            unsafe { SetTimer(Some(window), TIMER_UPDATES, DAY, None) };
            check_for_updates(window, false);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_HALLOWEEN => {
            refresh_icon(window);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            refresh_icon(window);
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_UPDATE => {
            let result = unsafe { Box::from_raw(lparam.0 as *mut std::result::Result<Option<Release>, String>) };
            updated(window, *result, wparam.0 != 0);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_WELCOME => {
            unsafe {
                let _ = KillTimer(Some(window), TIMER_WELCOME);
            }
            welcome::show(Handlers {
                scatter: Box::new(|pieces| {
                    APP.with_borrow(|app| {
                        if let Some(app) = app {
                            send(app, Command::Scatter(pieces));
                        }
                    })
                }),
                finished: Box::new(|| {
                    APP.with_borrow_mut(|app| {
                        let Some(app) = app else { return };
                        app.settings.welcomed = true;
                        app.settings.save();
                    })
                }),
                skipped: Box::new(|| {
                    APP.with_borrow(|app| {
                        if let Some(app) = app {
                            send(app, Command::ReleaseSnowfall);
                        }
                    })
                }),
            });
            LRESULT(0)
        }
        _ if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            APP.with_borrow(|app| {
                if let Some(app) = app {
                    add_tray_icon(window, app.icon);
                    send(app, Command::ShellRestarted);
                }
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Changes what pauses the snow and tells the snow thread whether it may run.
fn update(change: impl FnOnce(&mut App)) {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        change(app);
        let active = !app.locked && !app.display_off && !app.away;
        send(app, Command::Active(active));
    });
}

fn send(app: &App, command: Command) {
    let _ = app.commands.send(command);
    unsafe {
        let _ = SetEvent(app.wake);
    }
}

fn show_menu(window: HWND, x: i32, y: i32) {
    let current = || APP.with_borrow(|app| app.as_ref().map(|app| app.settings));
    let Some(settings) = current() else { return };
    let mut toggle = |id: u32| {
        choose(window, id, prefs::opens_at_login());
        entries(&current().unwrap_or(settings), prefs::opens_at_login(), true)
    };
    let native = || native_menu(window, x, y, &entries(&settings, prefs::opens_at_login(), false));
    let chosen = if menu::available() {
        menu::track(x, y, entries(&settings, prefs::opens_at_login(), true), light_taskbar(), &mut toggle).unwrap_or_else(|error| {
            log!("menu failed, using the system's: {error}");
            native()
        })
    } else {
        native()
    };
    choose(window, chosen, prefs::opens_at_login());
}

/// What the tray menu lists, with what is on or off: sliders in the drawn menu, submenus in the system's.
fn entries(settings: &Settings, login: bool, sliders: bool) -> Vec<Entry> {
    let picked = settings.modes();
    let falling: Vec<Mode> = Mode::ALL.iter().copied().filter(|mode| picked.contains(mode)).collect();
    // Mixed, the kinds are checkboxes and picking one leaves the menu open for the other; alone, one of a set.
    let kind = |id: u32, label: &str, mode: Mode| {
        if settings.mixed { Entry::toggle(id, label, picked.contains(&mode), true) } else { Entry::choice(id, label, settings.mode == mode) }
    };
    let modes = vec![
        Entry::toggle(ID_MIXED, tr("mixed"), settings.mixed, true),
        Entry::Separator,
        kind(ID_SNOW, tr("snow"), Mode::Snow),
        kind(ID_AUTUMN, tr("autumn"), Mode::Autumn),
        kind(ID_RAIN, tr("rain"), Mode::Rain),
        kind(ID_BLOSSOM, tr("blossom"), Mode::Blossom),
        Entry::Separator,
        Entry::item(ID_HALLOWEEN, tr("halloween"), halloween::wanted(settings.halloween), falling == [Mode::Snow]),
    ];
    let mut behavior = vec![Entry::header(tr("behavior"))];
    // Rain doesn't lie anywhere: nothing to keep, melt or clear.
    let heaps: Vec<Mode> = falling.iter().copied().filter(|&mode| mode != Mode::Rain).collect();
    if !heaps.is_empty() {
        let melt = tr(if heaps.contains(&Mode::Snow) {
            "melt.snow"
        } else if heaps == [Mode::Blossom] {
            "melt.blossom"
        } else {
            "melt.autumn"
        });
        behavior.push(Entry::toggle(ID_MELT, melt, settings.melt, settings.accumulates()));
    }
    let mut options = behavior;
    options.extend([
        Entry::toggle(ID_DEEP, tr("deep"), settings.deep_interaction, settings.snow_on_windows),
        Entry::Separator,
        Entry::header(tr("display")),
        Entry::toggle(ID_BEHIND, tr("behind"), settings.behind_windows, true),
        Entry::toggle(ID_FULL_SCREEN, tr("full_screen"), settings.over_full_screen, !settings.behind_windows),
        Entry::toggle(ID_TASKBAR, tr("taskbar"), settings.behind_taskbar, true),
        Entry::toggle(ID_SCREENSHOTS, tr("screenshots"), settings.show_in_screenshots, true),
        Entry::toggle(ID_MAX_PERFORMANCE, tr("performance"), settings.max_performance, true),
        Entry::Separator,
        Entry::header(tr("general")),
        Entry::toggle(ID_LOGIN, tr("login"), login, true),
        Entry::item(ID_UPDATES, tr("updates"), false, true),
    ]);
    let mut entries = if sliders {
        vec![
            Entry::header(tr("intensity")),
            Entry::slider(ID_SNOWFALL, settings.intensity.clamp(0, 100) / 5, INTENSITY_STEPS, &percent(settings.intensity)),
            // Opacity is set once and left, so it folds away like Accumulate, its value on the row.
            Entry::Disclosure {
                id: ID_OPACITY_FOLD,
                label: tr("opacity").to_string(),
                summary: format!("{}%", settings.opacity),
                open: OPACITY_OPEN.get(),
                keeps_summary: true,
            },
            Entry::folded_slider(ID_OPACITY, (settings.opacity.clamp(10, 100) - 10) / 5, OPACITY_STEPS, OPACITY_OPEN.get()),
            Entry::Separator,
        ]
    } else {
        let intensities =
            INTENSITY_STOPS.iter().map(|&stop| Entry::choice(ID_SNOWFALL + stop / 5, &percent(stop), stop == settings.intensity)).collect();
        let opacities =
            OPACITY_STOPS.iter().map(|&stop| Entry::choice(ID_OPACITY + (stop - 10) / 5, &format!("{stop}%"), stop == settings.opacity)).collect();
        vec![
            Entry::Submenu { label: tr("intensity").to_string(), entries: intensities },
            Entry::Submenu { label: tr("opacity").to_string(), entries: opacities },
        ]
    };
    entries.extend(accumulate(settings, !heaps.is_empty(), sliders));
    entries.push(Entry::Submenu { label: tr("mode").to_string(), entries: modes });
    if !heaps.is_empty() {
        entries.push(Entry::item(ID_CLEAR, text::kinds("clear", &heaps), false, true));
    }
    entries.extend([
        Entry::Separator,
        Entry::Submenu { label: tr("settings").to_string(), entries: options },
        Entry::loved(ID_SPONSOR, tr("support")),
        Entry::item(ID_QUIT, tr("quit"), false, true),
    ]);
    entries
}

/// Where snow settles: a row of icons Accumulate folds out in the drawn menu, a submenu of checkmarks in the system's.
/// Top to bottom, as they lie on the screen. Rain splashes on the ground whatever it says, so with only rain falling
/// there is just the windows.
fn accumulate(settings: &Settings, heaps: bool, drawn: bool) -> Vec<Entry> {
    let mut surfaces = vec![(ID_WINDOWS, Icon::Windows, tr("surface.windows"), settings.snow_on_windows)];
    if heaps {
        surfaces.push((ID_GROUND, Icon::Ground, tr("surface.ground"), settings.ground));
    }
    if !drawn {
        let entries = surfaces.iter().map(|&(id, _, label, on)| Entry::item(id, label, on, true)).collect();
        return vec![Entry::Submenu { label: tr("accumulate").to_string(), entries }];
    }
    let on: Vec<&str> = surfaces.iter().filter(|surface| surface.3).map(|surface| surface.2).collect();
    let summary = if on.len() == surfaces.len() {
        tr("all").to_string()
    } else if on.is_empty() {
        tr("off").to_string()
    } else {
        on.join(", ")
    };
    let open = ACCUMULATE_OPEN.get();
    let tiles = surfaces.into_iter().map(|(id, icon, label, on)| Tile { id, icon, label: label.to_string(), on }).collect();
    vec![
        Entry::Disclosure { id: ID_ACCUMULATE, label: tr("accumulate").to_string(), summary, open, keeps_summary: false },
        Entry::Icons { tiles, shown: open },
    ]
}

/// An intensity as its caption: a percentage, or Off.
fn percent(intensity: u32) -> String {
    if intensity == 0 { tr("off").to_string() } else { format!("{intensity}%") }
}

/// The system's own menu, for when the drawn one can't be shown.
fn native_menu(window: HWND, x: i32, y: i32, entries: &[Entry]) -> u32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        append(menu, entries);
        // A tray menu only closes on an outside click while its owner is in the foreground.
        let _ = SetForegroundWindow(window);
        let chosen = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY).0, x, y, window, None).0 as u32;
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        chosen
    }
}

unsafe fn append(menu: HMENU, entries: &[Entry]) {
    let text = |label: &str| -> Vec<u16> { label.encode_utf16().chain([0]).collect() };
    for entry in entries {
        unsafe {
            match entry {
                Entry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                Entry::Header(label) => {
                    let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, PCWSTR(text(label).as_ptr()));
                }
                Entry::Item { id, label, checked, enabled, radio, .. } => {
                    let flags = MF_STRING | if *checked { MF_CHECKED } else { MF_UNCHECKED } | if *enabled { MF_ENABLED } else { MF_GRAYED };
                    let _ = AppendMenuW(menu, flags, *id as usize, PCWSTR(text(label).as_ptr()));
                    if *radio && *checked {
                        let _ = CheckMenuRadioItem(menu, *id, *id, *id, MF_BYCOMMAND.0);
                    }
                }
                Entry::Submenu { label, entries } => {
                    let Ok(submenu) = CreatePopupMenu() else { continue };
                    append(submenu, entries);
                    let _ = AppendMenuW(menu, MF_POPUP, submenu.0 as usize, PCWSTR(text(label).as_ptr()));
                }
                Entry::Slider { .. } | Entry::Disclosure { .. } | Entry::Icons { .. } => {}
            }
        }
    }
}

fn choose(window: HWND, chosen: u32, login: bool) {
    match chosen {
        0 => {}
        ID_QUIT => quit(window),
        ID_UPDATES => check_for_updates(window, true),
        ID_SPONSOR => update::open(SPONSORS),
        ID_CLEAR => APP.with_borrow(|app| {
            if let Some(app) = app {
                send(app, Command::ClearSnow(!prefs::animations_enabled()));
            }
        }),
        ID_ACCUMULATE => ACCUMULATE_OPEN.set(!ACCUMULATE_OPEN.get()),
        ID_OPACITY_FOLD => OPACITY_OPEN.set(!OPACITY_OPEN.get()),
        ID_LOGIN => {
            let status = prefs::set_opens_at_login(!login);
            log!("open at login {}: {status:?}", !login);
        }
        ID_HALLOWEEN => {
            APP.with_borrow_mut(|app| {
                let Some(app) = app else { return };
                app.settings.halloween = app.settings.halloween.toggled(halloween::today());
                app.settings.save();
                send(app, Command::Halloween(app.settings.halloween));
            });
            refresh_icon(window);
        }
        ID_MIXED => {
            APP.with_borrow_mut(|app| {
                let Some(app) = app else { return };
                app.settings.toggle_mixed();
                switch_modes(app);
            });
            refresh_icon(window);
        }
        ID_SNOW | ID_AUTUMN | ID_RAIN | ID_BLOSSOM => {
            let mode = match chosen {
                ID_AUTUMN => Mode::Autumn,
                ID_RAIN => Mode::Rain,
                ID_BLOSSOM => Mode::Blossom,
                _ => Mode::Snow,
            };
            APP.with_borrow_mut(|app| {
                let Some(app) = app else { return };
                if app.settings.pick(mode) {
                    switch_modes(app);
                }
            });
            refresh_icon(window);
        }
        _ => APP.with_borrow_mut(|app| {
            let Some(app) = app else { return };
            let settings = &mut app.settings;
            let command = match chosen {
                ID_GROUND => {
                    settings.ground = !settings.ground;
                    Command::Ground(settings.ground)
                }
                ID_MELT => {
                    settings.melt = !settings.melt;
                    Command::Melt(settings.melt)
                }
                ID_WINDOWS => {
                    settings.snow_on_windows = !settings.snow_on_windows;
                    Command::SnowOnWindows(settings.snow_on_windows)
                }
                ID_BEHIND => {
                    settings.behind_windows = !settings.behind_windows;
                    Command::BehindWindows(settings.behind_windows)
                }
                ID_FULL_SCREEN => {
                    settings.over_full_screen = !settings.over_full_screen;
                    Command::OverFullScreen(settings.over_full_screen)
                }
                ID_TASKBAR => {
                    settings.behind_taskbar = !settings.behind_taskbar;
                    Command::BehindTaskbar(settings.behind_taskbar)
                }
                ID_SCREENSHOTS => {
                    settings.show_in_screenshots = !settings.show_in_screenshots;
                    Command::ShowInScreenshots(settings.show_in_screenshots)
                }
                ID_DEEP => {
                    settings.deep_interaction = !settings.deep_interaction;
                    Command::DeepInteraction(settings.deep_interaction)
                }
                ID_MAX_PERFORMANCE => {
                    settings.max_performance = !settings.max_performance;
                    Command::Performance(settings.max_performance)
                }
                id if (ID_OPACITY..ID_OPACITY + OPACITY_STEPS).contains(&id) => {
                    settings.opacity = 10 + 5 * (id - ID_OPACITY);
                    Command::FallingOpacity(settings.opacity as f32 / 100.0)
                }
                id if (ID_SNOWFALL..ID_SNOWFALL + INTENSITY_STEPS).contains(&id) => {
                    settings.intensity = 5 * (id - ID_SNOWFALL);
                    Command::Intensity(settings.intensity as f32 / 100.0)
                }
                _ => return,
            };
            settings.save();
            send(app, command);
        }),
    }
}

fn switch_modes(app: &App) {
    app.settings.save();
    let settings = &app.settings;
    send(app, Command::SwitchMode { mode: settings.mode, mixed: settings.mixed, mix: settings.mix, calm: !prefs::animations_enabled() });
}

/// Checks on another thread and reports back to the window with `WM_UPDATE`.
fn check_for_updates(window: HWND, asked: bool) {
    let target = window.0 as isize;
    thread::spawn(move || {
        let result = Box::new(update::check());
        unsafe {
            let _ = PostMessageW(Some(HWND(target as _)), WM_UPDATE, WPARAM(asked as usize), LPARAM(Box::into_raw(result) as isize));
        }
    });
}

/// A check someone asked for always answers; the daily one only speaks up, once, about a new version.
fn updated(window: HWND, result: std::result::Result<Option<Release>, String>, asked: bool) {
    let current = env!("CARGO_PKG_VERSION");
    match result {
        Ok(Some(release)) if asked => {
            let text = text::fill("update.ask", &[("version", &release.version), ("current", current)]);
            if message(window, &text, MB_YESNO | MB_ICONINFORMATION) == IDYES {
                update::open(&release.url);
            }
        }
        Ok(Some(release)) => {
            let fresh = APP.with_borrow_mut(|app| {
                let app = app.as_mut()?;
                (app.update.as_deref() != Some(&release.url)).then(|| app.update = Some(release.url.clone()))
            });
            if fresh.is_some() {
                notify(window, &text::fill("update.available", &[("version", &release.version)]), tr("update.click"));
            }
        }
        Ok(None) if asked => {
            message(window, &text::fill("update.latest", &[("current", current)]), MB_OK | MB_ICONINFORMATION);
        }
        Ok(None) => {}
        Err(error) => {
            log!("update check failed: {error}");
            if asked {
                message(window, &format!("{}\n\n{error}", tr("update.failed")), MB_OK | MB_ICONWARNING);
            }
        }
    }
}

fn message(window: HWND, text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe { MessageBoxW(Some(window), PCWSTR(text.as_ptr()), w!("Snowhush"), style | MB_SETFOREGROUND | MB_TOPMOST) }
}

/// A notification from the tray icon.
fn notify(window: HWND, title: &str, text: &str) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_INFO,
        ..Default::default()
    };
    for (slot, unit) in data.szInfoTitle.iter_mut().zip(title.encode_utf16().take(63)) {
        *slot = unit;
    }
    for (slot, unit) in data.szInfo.iter_mut().zip(text.encode_utf16().take(255)) {
        *slot = unit;
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

fn quit(window: HWND) {
    remove_tray_icon(window);
    if let Some(mut app) = APP.with_borrow_mut(Option::take) {
        send(&app, Command::Quit);
        if let Some(snow) = app.snow.take() {
            let _ = snow.join();
        }
        unsafe {
            let _ = DestroyIcon(app.icon);
        }
    }
    unsafe { PostQuitMessage(0) };
}

/// Puts a jack-o'-lantern in the tray while it is Halloween and wanted, a leaf while leaves fall, and the snowflake
/// back after, recoloured whenever the taskbar turns light or dark.
fn refresh_icon(window: HWND) {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        let emblem = Emblem::of(&app.settings);
        let light = light_taskbar();
        if emblem == app.emblem && light == app.light {
            return;
        }
        app.emblem = emblem;
        app.light = light;
        let icon = tray_icon(emblem, light);
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: 1,
            uFlags: NIF_ICON,
            hIcon: icon,
            ..Default::default()
        };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            let _ = DestroyIcon(app.icon);
        }
        app.icon = icon;
    });
}

/// The tray's icon at its size for the display's scale, or when its picture can't be had, the app icon from the
/// executable's resources.
fn tray_icon(emblem: Emblem, light: bool) -> HICON {
    unsafe {
        let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem());
        if let Some(icon) = picture_icon(emblem.glyph(light), size) {
            return icon;
        }
        let Ok(instance) = GetModuleHandleW(None) else { return HICON::default() };
        LoadImageW(Some(instance.into()), PCWSTR(1 as *const u16), IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
            .map_or(HICON::default(), |icon| HICON(icon.0))
    }
}

/// The picture of `pictures` closest above `size` pixels across, or the largest, as an icon that size.
fn picture_icon(pictures: &[(i32, &[u8])], size: i32) -> Option<HICON> {
    let (_, picture) = pictures.iter().find(|(side, _)| *side >= size).or(pictures.last())?;
    // Icon resources have kept the version Windows 3 gave them; a PNG is taken as one.
    unsafe { CreateIconFromResourceEx(picture, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR) }.ok()
}

fn add_tray_icon(window: HWND, icon: HICON) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (slot, unit) in data.szTip.iter_mut().zip("Snowhush".encode_utf16()) {
        *slot = unit;
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        let _ = Shell_NotifyIconW(NIM_ADD, &data);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
    }
}

fn remove_tray_icon(window: HWND) {
    let data = NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: window, uID: 1, ..Default::default() };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    }
}
