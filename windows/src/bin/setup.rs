//! Snowhush-Setup.exe: the app with a one-card installer in its own style.
//! `--silent` (or `/S`) installs without showing anything, for package managers.

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::path::PathBuf;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use snowhush::card::{self, Card, Content, Hits};
use snowhush::install;

/// The app this installer carries, built before it.
const APP: &[u8] = include_bytes!(env!("SNOWHUSH_APP"));
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CLASS: PCWSTR = w!("SnowhushSetup");
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 380.0;

enum Stage {
    Ready,
    Installing,
    Done(PathBuf),
    Failed(String),
}

struct Setup {
    window: HWND,
    card: Card,
    hits: Hits,
    stage: Stage,
    installed: Option<String>,
    pressed: bool,
    holding: bool,
}

thread_local! {
    static SETUP: RefCell<Option<Setup>> = const { RefCell::new(None) };
}

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    if std::env::args().skip(1).any(|argument| argument.eq_ignore_ascii_case("--silent") || argument.eq_ignore_ascii_case("/s")) {
        std::process::exit(if install::install(APP, VERSION).is_ok() { 0 } else { 1 });
    }
    if open().is_err() {
        // Without the card, fall back to installing and opening the app, which is what the card would lead to.
        if let Ok(exe) = install::install(APP, VERSION) {
            install::launch(&exe);
        }
        return;
    }
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn open() -> Result<()> {
    card::register(CLASS, Some(procedure))?;
    let (scale, origin) = card::placement(WIDTH, HEIGHT);
    let card = Card::new(WIDTH, HEIGHT, scale)?;
    let installed = install::installed_version();
    let (pixels, hits) = paint(&card, &Stage::Ready, installed.as_deref(), false)?;
    unsafe {
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_APPWINDOW,
            CLASS,
            w!("Snowhush Setup"),
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
            origin.x,
            origin.y,
            card.size.0,
            card.size.1,
            None,
            None,
            None,
            None,
        )?;
        card::present(window, card.size, &pixels, Some(origin));
        SETUP.set(Some(Setup { window, card, hits, stage: Stage::Ready, installed, pressed: false, holding: false }));
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
    }
    Ok(())
}

/// The card for a stage of the installation.
fn paint(card: &Card, stage: &Stage, installed: Option<&str>, pressed: bool) -> Result<(Vec<u8>, Hits)> {
    let installing = matches!(stage, Stage::Installing);
    let (title, body, button) = match stage {
        Stage::Ready | Stage::Installing => {
            let (body, button) = match installed {
                None => (format!("Version {VERSION}. Installs just for you, without administrator rights."), "INSTALL"),
                Some(old) if snowhush_core::version::newer(VERSION, old) => {
                    (format!("Updates Snowhush {old} to {VERSION}. Your settings stay as they are."), "UPDATE")
                }
                Some(old) => (format!("Snowhush {old} is already here. Installing {VERSION} again repairs it."), "REINSTALL"),
            };
            ("SNOWHUSH", body, if installing { "INSTALLING…" } else { button })
        }
        Stage::Done(_) => ("LET IT SETTLE.", "Snowhush is installed. You'll find it in the Start menu.".to_string(), "OPEN SNOWHUSH"),
        Stage::Failed(error) => ("SNOWHUSH", format!("Couldn't install: {error}."), "TRY AGAIN"),
    };
    card.paint(&Content { icon: 96.0, title, title_size: 40.0, body: &body, button, pressed: pressed || installing, closable: !installing })
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => {
            let mut origin = RECT::default();
            unsafe {
                let _ = GetWindowRect(window, &mut origin);
            }
            let point = card::point(lparam);
            let inside = SETUP.with_borrow(|setup| {
                setup.as_ref().map(|setup| {
                    let at = setup.card.dips(POINT { x: point.x - origin.left, y: point.y - origin.top });
                    card::contains(&setup.hits.button, at) || setup.hits.close.is_some_and(|close| card::contains(&close, at))
                })
            });
            // The card drags the window anywhere but on its controls.
            LRESULT(if inside == Some(false) { HTCAPTION as isize } else { HTCLIENT as isize })
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
            unsafe {
                if message == WM_LBUTTONDOWN {
                    SetCapture(window);
                } else if message == WM_LBUTTONUP {
                    let _ = ReleaseCapture();
                }
            }
            let at = card::point(lparam);
            let outcome = SETUP.with_borrow_mut(|setup| setup.as_mut().map(|setup| setup.press(message, at)));
            if let Some((action, repaint)) = outcome {
                if let Some(pixels) = repaint {
                    show(&pixels);
                }
                match action {
                    Some(Action::Button) => act(),
                    Some(Action::Close) => close(),
                    None => {}
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_RETURN.0 as usize => {
            act();
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
            close();
            LRESULT(0)
        }
        WM_CLOSE => {
            close();
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

enum Action {
    Button,
    Close,
}

impl Setup {
    /// Tracks a press on the button or the close mark: what was clicked, and the card redrawn if the button's look changed.
    fn press(&mut self, message: u32, point: POINT) -> (Option<Action>, Option<Vec<u8>>) {
        let at = self.card.dips(point);
        let on_button = card::contains(&self.hits.button, at);
        let on_close = self.hits.close.is_some_and(|close| card::contains(&close, at));
        let mut action = None;
        match message {
            WM_LBUTTONDOWN => self.holding = on_button,
            WM_LBUTTONUP => {
                if self.holding && on_button {
                    action = Some(Action::Button);
                } else if on_close {
                    action = Some(Action::Close);
                }
                self.holding = false;
            }
            _ if !self.holding => return (None, None),
            _ => {}
        }
        let pressed = self.holding && on_button;
        if pressed == self.pressed {
            return (action, None);
        }
        self.pressed = pressed;
        (action, self.repaint())
    }

    fn repaint(&mut self) -> Option<Vec<u8>> {
        let (pixels, hits) = paint(&self.card, &self.stage, self.installed.as_deref(), self.pressed).ok()?;
        self.hits = hits;
        Some(pixels)
    }
}

fn show(pixels: &[u8]) {
    if let Some((window, size)) = SETUP.with_borrow(|setup| setup.as_ref().map(|setup| (setup.window, setup.card.size))) {
        card::present(window, size, pixels, None);
    }
}

/// Moves the card to a new stage and shows it.
fn stage(stage: Stage) {
    if let Some(pixels) = SETUP.with_borrow_mut(|setup| {
        let setup = setup.as_mut()?;
        setup.stage = stage;
        setup.pressed = false;
        setup.repaint()
    }) {
        show(&pixels);
    }
}

/// The button: installs, or once installed opens the app and leaves.
fn act() {
    let done = SETUP.with_borrow(|setup| match setup.as_ref().map(|setup| &setup.stage) {
        Some(Stage::Done(exe)) => Some(Some(exe.clone())),
        Some(Stage::Installing) | None => None,
        _ => Some(None),
    });
    match done {
        Some(Some(exe)) => {
            install::launch(&exe);
            close();
        }
        Some(None) => {
            stage(Stage::Installing);
            stage(match install::install(APP, VERSION) {
                Ok(exe) => Stage::Done(exe),
                Err(error) => Stage::Failed(error),
            });
        }
        None => {}
    }
}

fn close() {
    if SETUP.with_borrow(|setup| setup.as_ref().is_some_and(|setup| matches!(setup.stage, Stage::Installing))) {
        return;
    }
    if let Some(setup) = SETUP.take() {
        unsafe {
            let _ = DestroyWindow(setup.window);
        }
    }
}
