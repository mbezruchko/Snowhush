//! Where the other windows are, which Wayland keeps from apps: each compositor tells it in its own way, and
//! each way ends in the same report. KWin runs a script that calls the app; Hyprland, Sway and niri answer
//! over their own sockets.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use calloop::ping::Ping;
use serde::Deserialize;
use zbus::blocking::Connection as Bus;

use crate::{hyprland, kwin, log, niri, sway};

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

/// How closely the snow needs the windows followed, for a compositor that has to be asked where they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Demand {
    /// Nothing shows, as while locked or with the displays off: asked only when the compositor announces something.
    Off,
    /// No snow shows: asked when the compositor announces something, and now and then.
    Calm,
    /// As closely as the compositor allows.
    Live,
}

/// How often a source asks while the snow's demand is calm.
const IDLE: Duration = Duration::from_secs(1);
/// How often a source waiting to ask looks whether the snow came to need more.
const LOOK: Duration = Duration::from_millis(100);

/// What the compositor last told, read by the snow loop at each frame.
pub struct Shared {
    cursor: AtomicU64,
    report: Mutex<(u64, Option<Arc<Report>>)>,
    started_moving: Mutex<bool>,
    /// Wakes the snow loop at once, for the first report and when a drag starts.
    wake: Ping,
    /// What the snow loop needs of the windows and the pointer, set at each frame.
    windows_demand: AtomicU8,
    pointer_wanted: AtomicBool,
}

impl Shared {
    pub fn new(wake: Ping) -> Shared {
        Shared {
            cursor: AtomicU64::new(pack(-1e6, -1e6)),
            report: Mutex::new((0, None)),
            started_moving: Mutex::new(false),
            wake,
            windows_demand: AtomicU8::new(Demand::Live as u8),
            pointer_wanted: AtomicBool::new(true),
        }
    }

    pub fn set_demand(&self, windows: Demand, pointer: bool) {
        self.windows_demand.store(windows as u8, Ordering::Relaxed);
        self.pointer_wanted.store(pointer, Ordering::Relaxed);
    }

    pub fn windows_demand(&self) -> Demand {
        match self.windows_demand.load(Ordering::Relaxed) {
            0 => Demand::Off,
            1 => Demand::Calm,
            _ => Demand::Live,
        }
    }

    /// Whether the pointer is worth asking the compositor for: something on the displays can part around it.
    pub fn pointer_wanted(&self) -> bool {
        self.pointer_wanted.load(Ordering::Relaxed)
    }

    /// Waits until a source that last asked under the demand `asked` should ask the compositor again: `close` after
    /// that while the demand is live, a second while it is calm, and never while it is off, but at once when the
    /// compositor announces something or the demand goes up. The announcements that came, or None once they stopped.
    pub fn wait<T>(&self, announcements: &Receiver<T>, asked: Demand, close: Duration) -> Option<Vec<T>> {
        let start = Instant::now();
        loop {
            let demand = self.windows_demand();
            if demand > asked {
                return Some(Vec::new());
            }
            let wait = match demand {
                Demand::Live => close,
                Demand::Calm => IDLE,
                Demand::Off => Duration::MAX,
            };
            let left = wait.saturating_sub(start.elapsed());
            if left.is_zero() {
                return Some(Vec::new());
            }
            match announcements.recv_timeout(left.min(LOOK)) {
                Ok(first) => return Some(std::iter::once(first).chain(announcements.try_iter()).collect()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
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

/// How long to wait before asking a compositor again after a request failed: half a second, then twice as long
/// each time, up to five seconds.
pub fn backoff(last: Option<Duration>) -> Duration {
    last.map_or(Duration::from_millis(500), |wait| (wait * 2).min(Duration::from_secs(5)))
}

fn pack(x: f32, y: f32) -> u64 {
    ((x.to_bits() as u64) << 32) | y.to_bits() as u64
}

/// The compositor the windows come from, picked by what the session says it runs. A compositor started
/// inside another leaves its own variable to the apps in it, so the innermost is asked first.
pub enum Source {
    Hyprland,
    Niri,
    Sway,
    KWin,
}

impl Source {
    pub fn start(bus: &Bus, shared: Arc<Shared>, show_in_screenshots: bool) -> Source {
        // Hyprland's instance is checked against the app's own Wayland socket, so it is right wherever it runs.
        let hyprland = hyprland::instance();
        let source = if hyprland.is_some() {
            Source::Hyprland
        } else if std::env::var_os("NIRI_SOCKET").is_some() {
            Source::Niri
        } else if std::env::var_os("SWAYSOCK").is_some() {
            Source::Sway
        } else {
            Source::KWin
        };
        let started = match source {
            Source::Hyprland => hyprland.ok_or_else(|| "no Hyprland instance".to_string()).and_then(|folder| hyprland::start(shared, folder)),
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
            Source::Hyprland => "Hyprland",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn asks_again_as_closely_as_the_snow_needs() {
        let (ping, _source) = calloop::ping::make_ping().expect("a ping");
        let shared = Arc::new(Shared::new(ping));
        let (announce, announcements) = mpsc::channel::<()>();
        let close = Duration::from_millis(16);

        let start = Instant::now();
        assert_eq!(shared.wait(&announcements, Demand::Live, close).map(|lines| lines.len()), Some(0));
        assert!(start.elapsed() >= close && start.elapsed() < IDLE);

        shared.set_demand(Demand::Calm, false);
        let start = Instant::now();
        assert_eq!(shared.wait(&announcements, Demand::Calm, close).map(|lines| lines.len()), Some(0));
        assert!(start.elapsed() >= IDLE);

        shared.set_demand(Demand::Off, false);
        announce.send(()).expect("sent");
        announce.send(()).expect("sent");
        assert_eq!(shared.wait(&announcements, Demand::Off, close).map(|lines| lines.len()), Some(2));

        let raising = shared.clone();
        let raised = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            raising.set_demand(Demand::Live, true);
        });
        let start = Instant::now();
        assert_eq!(shared.wait(&announcements, Demand::Off, close).map(|lines| lines.len()), Some(0));
        assert!(start.elapsed() >= Duration::from_millis(300) && start.elapsed() < Duration::from_millis(300) + IDLE);
        raised.join().expect("joined");

        drop(announce);
        assert!(shared.wait(&announcements, Demand::Live, close).is_none());
    }
}
