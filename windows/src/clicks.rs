//! Ctrl-clicks on the wallpaper, for throwing snowballs into its picture: a low-level mouse hook on a thread of its own.
//! It hands each one to the snow's loop and keeps it from the desktop, which would otherwise select the icons under it;
//! every other click goes on untouched. The hook must answer at once, or Windows takes it away.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{HANDLE, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::SetEvent;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GA_ROOT, GetAncestor, GetMessageW, HC_ACTION, MSG, MSLLHOOKSTRUCT, SetWindowsHookExW, TranslateMessage,
    WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WindowFromPoint,
};

use crate::log;
use crate::tracker::class_name;

/// Whether a snowball fight is on anywhere: the hook takes no click otherwise.
static FIGHTING: AtomicBool = AtomicBool::new(false);
/// The click taken, whose button's release is kept from the desktop too.
static HELD: AtomicBool = AtomicBool::new(false);
/// The event that wakes the snow's loop.
static WAKE: AtomicIsize = AtomicIsize::new(0);
/// Clicks taken, in physical pixels on the shared desktop, for the snow's loop to throw.
static TAKEN: Mutex<Vec<(i32, i32)>> = Mutex::new(Vec::new());

/// Starts watching for Ctrl-clicks, telling the snow's loop through `wake`.
pub fn start(wake: isize) {
    WAKE.store(wake, Ordering::Relaxed);
    let _ = std::thread::Builder::new().name("clicks".into()).spawn(|| unsafe {
        let Ok(instance) = GetModuleHandleW(None) else { return };
        if let Err(error) = SetWindowsHookExW(WH_MOUSE_LL, Some(hook), Some(instance.into()), 0) {
            log!("no Ctrl-clicks: {error}");
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
}

/// Whether a fight is on: only then are Ctrl-clicks on the wallpaper taken.
pub fn set_fighting(on: bool) {
    FIGHTING.store(on, Ordering::Relaxed);
}

/// The Ctrl-clicks taken since this was last asked.
pub fn take() -> Vec<(i32, i32)> {
    TAKEN.lock().map(|mut taken| std::mem::take(&mut *taken)).unwrap_or_default()
}

/// Whether the window under a point is the desktop: its icons, or the wallpaper behind them.
fn desktop_at(point: POINT) -> bool {
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
        if code == HC_ACTION as i32 && FIGHTING.load(Ordering::Relaxed) {
            let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            match wparam.0 as u32 {
                WM_LBUTTONDOWN if GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 && desktop_at(info.pt) => {
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
