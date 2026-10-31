//! Where the other windows are, which Wayland keeps from apps: each compositor tells it in its own way, and
//! each way ends in the same report. KWin runs a script that calls the app; Sway and niri answer over their
//! own sockets.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use calloop::ping::Ping;
use serde::Deserialize;
use zbus::blocking::Connection as Bus;

use crate::{kwin, log, niri, sway};

/// A display, in logical pixels on the shared desktop: its bounds and its work area, the part panels leave
/// free, whose bottom is the ground the snow settles on.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Display {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub ax: f64,
    pub ay: f64,
    pub aw: f64,
    pub ah: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Window {
    pub id: String,
    pub class: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// An app's window or dialog, which snow can settle on; menus, tooltips and notifications only hide it.
    pub solid: bool,
    pub full: bool,
    pub above: bool,
    pub decorated: bool,
    pub maximized: bool,
    /// The corner radius, where the compositor's is known; otherwise the decoration's is guessed.
    #[serde(default)]
    pub radius: Option<f32>,
}

/// A panel's window, which takes in the gaps around a floating panel.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The displays, and the windows on the current desktop front to back.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub screens: Vec<Display>,
    pub windows: Vec<Window>,
    /// Windows on other desktops or activities.
    pub elsewhere: Vec<String>,
    #[serde(default)]
    pub panels: Vec<Panel>,
    /// A window is being dragged or resized.
    pub moving: bool,
    /// The compositor shows something the snow mustn't be drawn over, such as niri's overview.
    #[serde(default)]
    pub hidden: bool,
    /// The desktop and activity shown.
    #[serde(default)]
    pub desktop: String,
    /// The desktop's column in KWin's grid of desktops, counted from 0, how many columns there are, and whether
    /// switching on past the last desktop comes round to the first.
    #[serde(default)]
    pub column: i32,
    #[serde(default)]
    pub columns: i32,
    #[serde(default)]
    pub wraps: bool,
    /// The desktops are sliding past: windows that came with the new one are listed where they will stop, not
    /// where they are now.
    #[serde(skip)]
    pub switching: bool,
    /// The latest switch that slid the desktops sideways.
    #[serde(skip)]
    pub slide: Option<kwin::Slide>,
}

/// What the compositor last told, read by the snow loop at each frame.
pub struct Shared {
    cursor: AtomicU64,
    report: Mutex<(u64, Option<Arc<Report>>)>,
    started_moving: Mutex<bool>,
    /// Wakes the snow loop at once, for the first report and when a drag starts.
    wake: Ping,
}

impl Shared {
    pub fn new(wake: Ping) -> Shared {
        Shared { cursor: AtomicU64::new(pack(-1e6, -1e6)), report: Mutex::new((0, None)), started_moving: Mutex::new(false), wake }
    }

    /// The pointer in logical pixels on the shared desktop, far away where the compositor doesn't tell.
    pub fn cursor(&self) -> (f32, f32) {
        let bits = self.cursor.load(Ordering::Relaxed);
        (f32::from_bits((bits >> 32) as u32), f32::from_bits(bits as u32))
    }

    pub fn set_cursor(&self, x: f32, y: f32) {
        self.cursor.store(pack(x, y), Ordering::Relaxed);
    }

    /// The latest report and its generation, which goes up with each one.
    pub fn report(&self) -> (u64, Option<Arc<Report>>) {
        self.report.lock().map(|report| report.clone()).unwrap_or((0, None))
    }

    pub fn publish(&self, report: Report) {
        let Ok(mut current) = self.report.lock() else { return };
        let first = current.1.is_none();
        let was_moving = current.1.as_ref().is_some_and(|report| report.moving);
        let started = report.moving && !was_moving;
        *current = (current.0 + 1, Some(Arc::new(report)));
        drop(current);
        if started && let Ok(mut flag) = self.started_moving.lock() {
            *flag = true;
        }
        if first || started {
            self.wake.ping();
        }
    }

    /// Whether a drag or resize started since the last call.
    pub fn take_started_moving(&self) -> bool {
        self.started_moving.lock().map(|mut started| std::mem::take(&mut *started)).unwrap_or(false)
    }
}

fn pack(x: f32, y: f32) -> u64 {
    ((x.to_bits() as u64) << 32) | y.to_bits() as u64
}

/// The compositor the windows come from, picked by what the session says it runs. A compositor started
/// inside another leaves its own variable to the apps in it, so the innermost is asked first.
pub enum Source {
    Niri,
    Sway,
    KWin,
}

impl Source {
    pub fn start(bus: &Bus, shared: Arc<Shared>, show_in_screenshots: bool) -> Source {
        let source = if std::env::var_os("NIRI_SOCKET").is_some() {
            Source::Niri
        } else if std::env::var_os("SWAYSOCK").is_some() {
            Source::Sway
        } else {
            Source::KWin
        };
        let started = match source {
            Source::Niri => niri::start(shared),
            Source::Sway => sway::start(shared),
            Source::KWin => kwin::load_script(bus, show_in_screenshots),
        };
        match started {
            Ok(()) => log!("windows from {}", source.name()),
            Err(error) => log!("windows from {} failed: {error}; no snow on windows", source.name()),
        }
        source
    }

    fn name(&self) -> &'static str {
        match self {
            Source::Niri => "niri",
            Source::Sway => "Sway",
            Source::KWin => "KWin",
        }
    }

    /// Only KWin can keep the snow out of screenshots, through the script, which applies it when loaded.
    pub fn show_in_screenshots(&self, bus: &Bus, on: bool) {
        if let Source::KWin = self
            && let Err(error) = kwin::load_script(bus, on)
        {
            log!("KWin script failed: {error}");
        }
    }

    pub fn stop(&self, bus: &Bus) {
        if let Source::KWin = self {
            kwin::unload_script(bus);
        }
    }
}
