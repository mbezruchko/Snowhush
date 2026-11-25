//! Ctrl-clicks on the wallpaper, for throwing snowballs into its picture: a low-level mouse hook on a thread of its own,
//! set only while a fight is on and taken away when it ends. It hands each Ctrl-click on the desktop of a display with a
//! fight to the snow's loop and keeps it from the desktop, which would otherwise select the icons under it; every other
//! click goes on untouched. The hook must answer at once, or Windows takes it away.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{HANDLE, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentThreadId, SetEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GA_ROOT, GetAncestor, GetMessageW, HC_ACTION, MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WindowFromPoint,
};

use crate::log;
use crate::tracker::class_name;

/// The displays a fight is on, in physical pixels on the shared desktop: the hook takes clicks over these alone.
static FIGHTING: Mutex<Vec<RECT>> = Mutex::new(Vec::new());
/// The click taken, whose button's release is kept from the desktop too.
static HELD: AtomicBool = AtomicBool::new(false);
/// The event that wakes the snow's loop.
static WAKE: AtomicIsize = AtomicIsize::new(0);
/// The hook's thread while it is set, to be told to take it away.
static THREAD: AtomicU32 = AtomicU32::new(0);
/// Clicks taken, in physical pixels on the shared desktop, for the snow's loop to throw.
static TAKEN: Mutex<Vec<(i32, i32)>> = Mutex::new(Vec::new());

/// The displays a fight is on now, telling the snow's loop through `wake`: the hook is set while there are any and taken
/// away once there are none.
pub fn follow(fighting: Vec<RECT>, wake: isize) {
    let on = !fighting.is_empty();
    if let Ok(mut areas) = FIGHTING.lock() {
        *areas = fighting;
    }
    WAKE.store(wake, Ordering::Relaxed);
    let thread = THREAD.load(Ordering::Relaxed);
    if on && thread == 0 {
        start();
    } else if !on && thread != 0 {
        THREAD.store(0, Ordering::Relaxed);
        unsafe {
            let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

fn start() {
    THREAD.store(u32::MAX, Ordering::Relaxed);
    let _ = std::thread::Builder::new().name("clicks".into()).spawn(|| unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::Relaxed);
        let Ok(instance) = GetModuleHandleW(None) else { return };
        let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(hook), Some(instance.into()), 0) {
            Ok(hook) => hook,
            Err(error) => {
                log!("no Ctrl-clicks: {error}");
                THREAD.store(0, Ordering::Relaxed);
                return;
            }
        };
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = UnhookWindowsHookEx(hook);
    });
}

/// The Ctrl-clicks taken since this was last asked.
pub fn take() -> Vec<(i32, i32)> {
    TAKEN.lock().map(|mut taken| std::mem::take(&mut *taken)).unwrap_or_default()
}

/// Whether a point is on the desktop of a display a fight is on: its icons, or the wallpaper behind them.
fn fighting_desktop_at(point: POINT) -> bool {
    let over =
        FIGHTING.lock().is_ok_and(|areas| areas.iter().any(|a| point.x >= a.left && point.x < a.right && point.y >= a.top && point.y < a.bottom));
    if !over {
        return false;
    }
    unsafe {
        let window = WindowFromPoint(point);
        if window.is_invalid() {
            return false;
        }
        let root = GetAncestor(window, GA_ROOT);
        [window, root].into_iter().any(|window| matches!(class_name(window).as_str(), "SysListView32" | "SHELLDLL_DefView" | "WorkerW" | "Progman"))
    }
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            match wparam.0 as u32 {
                WM_LBUTTONDOWN if GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 && fighting_desktop_at(info.pt) => {
                    if let Ok(mut taken) = TAKEN.lock() {
                        taken.push((info.pt.x, info.pt.y));
                    }
                    let _ = SetEvent(HANDLE(WAKE.load(Ordering::Relaxed) as _));
                    HELD.store(true, Ordering::Relaxed);
                    return LRESULT(1);
                }
                WM_LBUTTONUP if HELD.swap(false, Ordering::Relaxed) => return LRESULT(1),
                _ => {}
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}
