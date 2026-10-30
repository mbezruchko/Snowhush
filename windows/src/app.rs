use std::cell::RefCell;
use std::mem::size_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification};
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
use crate::prefs::{self, Settings};
use crate::snow::{self, Command, Links};
use crate::update::{self, Release};
use crate::welcome::{self, Handlers};
use snowhush_core::simulation::Snowfall;

const CLASS: PCWSTR = w!("SnowhushController");
const WM_TRAY: u32 = WM_APP + 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const ID_SNOWFALL: u32 = 100;
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
const TIMER_WELCOME: usize = 1;
const TIMER_UPDATES: usize = 2;
const TIMER_HALLOWEEN: usize = 3;
const WM_UPDATE: u32 = WM_APP + 2;
/// The shell's message for a click on the tray notification.
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;
const DAY: u32 = 24 * 60 * 60 * 1000;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct App {
    settings: Settings,
    commands: Sender<Command>,
    wake: HANDLE,
    snow: Option<JoinHandle<()>>,
    icon: HICON,
    /// Whether the tray shows the jack-o'-lantern.
    halloween: bool,
    locked: bool,
    display_off: bool,
    away: bool,
    /// The download of the update the tray last announced.
    update: Option<String>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
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
        let halloween = settings.halloween && halloween::season();
        let icon = tray_icon(halloween);
        add_tray_icon(window, icon);
        let _ = WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);
        let _ = RegisterPowerSettingNotification(HANDLE(window.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);
        if !settings.welcomed {
            SetTimer(Some(window), TIMER_WELCOME, 1200, None);
        }
        SetTimer(Some(window), TIMER_UPDATES, 30_000, None);
        SetTimer(Some(window), TIMER_HALLOWEEN, 60_000, None);
        APP.set(Some(App { settings, commands, wake, snow, icon, halloween, locked: false, display_off: false, away: false, update: None }));

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
            refresh_halloween(window);
            LRESULT(0)
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
    let Some(settings) = APP.with_borrow(|app| app.as_ref().map(|app| app.settings)) else { return };
    let login = prefs::opens_at_login();
    let chosen = unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let Ok(snowfall) = CreatePopupMenu() else { return };
        for (i, option) in Snowfall::ALL.iter().enumerate() {
            let title: Vec<u16> = option.title().encode_utf16().chain([0]).collect();
            let _ = AppendMenuW(snowfall, MF_STRING, (ID_SNOWFALL + i as u32) as usize, PCWSTR(title.as_ptr()));
        }
        let current = Snowfall::ALL.iter().position(|&s| s == settings.snowfall).unwrap_or(0) as u32;
        let _ = CheckMenuRadioItem(snowfall, ID_SNOWFALL, ID_SNOWFALL + 4, ID_SNOWFALL + current, MF_BYCOMMAND.0);
        let checked = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_POPUP, snowfall.0 as usize, w!("Snowfall"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING | checked(settings.accumulate), ID_ACCUMULATE as usize, w!("Accumulate"));
        let melt = MF_STRING | checked(settings.melt) | if settings.accumulate { MF_ENABLED } else { MF_GRAYED };
        let _ = AppendMenuW(menu, melt, ID_MELT as usize, w!("Melt Snow"));
        let windows = MF_STRING | checked(settings.snow_on_windows) | if settings.behind_windows { MF_GRAYED } else { MF_ENABLED };
        let _ = AppendMenuW(menu, windows, ID_WINDOWS as usize, w!("Snow on Windows"));
        let _ = AppendMenuW(menu, MF_STRING, ID_CLEAR as usize, w!("Clear Snow"));
        if halloween::season() {
            let _ = AppendMenuW(menu, MF_STRING | checked(settings.halloween), ID_HALLOWEEN as usize, w!("Halloween"));
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING | checked(settings.behind_windows), ID_BEHIND as usize, w!("Behind Windows"));
        let _ = AppendMenuW(menu, MF_STRING | checked(settings.show_in_screenshots), ID_SCREENSHOTS as usize, w!("Show in Screenshots"));
        let _ = AppendMenuW(menu, MF_STRING | checked(login), ID_LOGIN as usize, w!("Open at Login"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_UPDATES as usize, w!("Check for Updates…"));
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT as usize, w!("Quit"));

        // A tray menu only closes on an outside click while its owner is in the foreground.
        let _ = SetForegroundWindow(window);
        let chosen = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY).0, x, y, window, None).0 as u32;
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        chosen
    };
    choose(window, chosen, login);
}

fn choose(window: HWND, chosen: u32, login: bool) {
    match chosen {
        0 => {}
        ID_QUIT => quit(window),
        ID_UPDATES => check_for_updates(window, true),
        ID_CLEAR => APP.with_borrow(|app| {
            if let Some(app) = app {
                send(app, Command::ClearSnow(!prefs::animations_enabled()));
            }
        }),
        ID_LOGIN => {
            let status = prefs::set_opens_at_login(!login);
            log!("open at login {}: {status:?}", !login);
        }
        ID_HALLOWEEN => {
            APP.with_borrow_mut(|app| {
                let Some(app) = app else { return };
                app.settings.halloween = !app.settings.halloween;
                app.settings.save();
                send(app, Command::Halloween(app.settings.halloween));
            });
            refresh_halloween(window);
        }
        _ => APP.with_borrow_mut(|app| {
            let Some(app) = app else { return };
            let settings = &mut app.settings;
            let command = match chosen {
                ID_ACCUMULATE => {
                    settings.accumulate = !settings.accumulate;
                    Command::Accumulate(settings.accumulate)
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
                ID_SCREENSHOTS => {
                    settings.show_in_screenshots = !settings.show_in_screenshots;
                    Command::ShowInScreenshots(settings.show_in_screenshots)
                }
                _ => {
                    let Some(&snowfall) = Snowfall::ALL.get(chosen.wrapping_sub(ID_SNOWFALL) as usize) else { return };
                    settings.snowfall = snowfall;
                    Command::Snowfall(snowfall)
                }
            };
            settings.save();
            send(app, command);
        }),
    }
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
            let text = format!("Snowhush {} is available. You have {current}.\n\nDownload it now?", release.version);
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
                notify(window, &format!("Snowhush {} is available", release.version), "Click to download it.");
            }
        }
        Ok(None) if asked => {
            message(window, &format!("You're up to date.\n\nSnowhush {current} is the latest version."), MB_OK | MB_ICONINFORMATION);
        }
        Ok(None) => {}
        Err(error) => {
            log!("update check failed: {error}");
            if asked {
                message(window, &format!("Couldn't check for updates.\n\n{error}"), MB_OK | MB_ICONWARNING);
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

/// Puts a jack-o'-lantern in the tray while it is Halloween and wanted, and the app icon back after.
fn refresh_halloween(window: HWND) {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        let on = app.settings.halloween && halloween::season();
        if on == app.halloween {
            return;
        }
        app.halloween = on;
        let icon = tray_icon(on);
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

/// The tray's icon at its size for the display's scale: the jack-o'-lantern with `pumpkin`, or when it can't be had,
/// the app icon from the executable's resources.
fn tray_icon(pumpkin: bool) -> HICON {
    unsafe {
        let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem());
        if pumpkin && let Some(icon) = halloween::pumpkin(size) {
            return icon;
        }
        let Ok(instance) = GetModuleHandleW(None) else { return HICON::default() };
        LoadImageW(Some(instance.into()), PCWSTR(1 as *const u16), IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
            .map_or(HICON::default(), |icon| HICON(icon.0))
    }
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
