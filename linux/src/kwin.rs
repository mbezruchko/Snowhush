//! What Wayland keeps from apps and KWin knows: where the windows are, which is in front, and the pointer.
//! A KWin script (`snowhush.js`) reports them to this app over D-Bus.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use zbus::blocking::Connection;

use crate::log;
use crate::prefs;
use crate::source::{Report, Shared};

pub const SERVICE: &str = "io.github.mbezruchko.Snowhush";
const PATH: &str = "/io/github/mbezruchko/Snowhush";
const SCRIPT: &str = include_str!("snowhush.js");
const PLUGIN: &str = "snowhush";
/// KWin's effects that animate a switch between desktops.
const SWITCH_EFFECTS: [&str; 2] = ["slide", "fadedesktop"];
/// How long KWin's slide takes, at the usual animation speed, to bring the windows within a few dozen pixels of
/// where they stop; it then creeps the rest of the way for about as long again.
const SLIDE_SETTLES: f32 = 0.35;
/// The spring KWin's slide moves the desktops on: this stiff at the usual animation speed, and damped this much.
const SLIDE_STIFFNESS: f32 = 300.0;
const SLIDE_DAMPING: f32 = 1.1;

struct Reports {
    shared: Arc<Shared>,
    switch: Arc<Mutex<Switch>>,
}

/// The desktop last reported with its column in the grid, and whether the desktops are still sliding after the
/// latest switch, the `count`th.
#[derive(Default)]
struct Switch {
    desktop: Option<(String, i32)>,
    count: u64,
    sliding: bool,
    slide: Option<Slide>,
}

/// The desktops sliding sideways on a switch, as KWin animates them: since when, how far the windows go, in
/// pixels and negative to the left, and how stiff the spring is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slide {
    start: Instant,
    distance: f32,
    stiffness: f32,
}

impl Slide {
    /// How fast the windows move across at `now`, in pixels a second, negative to the left.
    pub fn velocity(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32();
        let omega = self.stiffness.sqrt();
        let spread = (SLIDE_DAMPING * SLIDE_DAMPING - 1.0).sqrt();
        let (slow, fast) = (-omega * (SLIDE_DAMPING - spread), -omega * (SLIDE_DAMPING + spread));
        self.distance * omega / (2.0 * spread) * ((slow * t).exp() - (fast * t).exp())
    }
}

#[zbus::interface(name = "io.github.mbezruchko.Snowhush.KWin")]
impl Reports {
    fn cursor(&self, position: &str) {
        let mut parts = position.split(' ').map(|part| part.parse::<f32>());
        if let (Some(Ok(x)), Some(Ok(y))) = (parts.next(), parts.next()) {
            self.shared.set_cursor(x, y);
        }
    }

    fn windows(&self, state: &str, #[zbus(connection)] connection: &zbus::Connection) {
        let mut report: Report = match serde_json::from_str(state) {
            Ok(report) => report,
            Err(error) => {
                log!("unreadable window report: {error}");
                return;
            }
        };
        let Ok(mut switch) = self.switch.lock() else { return };
        if let Some((desktop, column)) = switch.desktop.clone()
            && desktop != report.desktop
        {
            let factor = prefs::animation_factor();
            let across = columns_across(column, &report);
            if across != 0 && factor > 0.0 && prefs::slide_enabled() {
                let distance = -across as f32 * desktop_width(&report);
                switch.slide = Some(Slide { start: Instant::now(), distance, stiffness: SLIDE_STIFFNESS / factor });
            }
            switch.count += 1;
            switch.sliding = true;
            wait_out_slide(self.shared.clone(), self.switch.clone(), switch.count, factor, connection.clone().into());
        }
        switch.desktop = Some((report.desktop.clone(), report.column));
        report.switching = switch.sliding;
        report.slide = switch.slide;
        self.shared.publish(report);
    }
}

/// KWin reports the windows of a new desktop where they will stop as soon as it starts sliding them in; once they
/// are nearly there, the report goes out again for the snow to grow back onto them.
fn wait_out_slide(shared: Arc<Shared>, switch: Arc<Mutex<Switch>>, count: u64, factor: f32, connection: Connection) {
    std::thread::spawn(move || {
        // The slide is a spring, stiffer the faster animations are set, so its time goes as the square root of their
        // duration factor.
        let settled = Instant::now() + Duration::from_secs_f32(SLIDE_SETTLES * factor.sqrt());
        while Instant::now() < settled && sliding(&connection) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let Ok(mut switch) = switch.lock() else { return };
        if switch.count != count {
            return;
        }
        switch.sliding = false;
        if let (_, Some(latest)) = shared.report() {
            shared.publish(Report { switching: false, ..Report::clone(&latest) });
        }
    });
}

/// How many desktops across KWin slides from one column of its grid to the report's: the short way round where
/// it wraps, positive to the right.
fn columns_across(from: i32, report: &Report) -> i32 {
    let across = report.column - from;
    if report.wraps && across.abs() as f32 > report.columns as f32 / 2.0 { across - across.signum() * report.columns } else { across }
}

/// KWin slides the desktops by the width of all the displays together.
fn desktop_width(report: &Report) -> f32 {
    let left = report.screens.iter().map(|screen| screen.x).fold(f64::INFINITY, f64::min);
    let right = report.screens.iter().map(|screen| screen.x + screen.w).fold(f64::NEG_INFINITY, f64::max);
    if right > left { (right - left) as f32 } else { 0.0 }
}

fn sliding(connection: &Connection) -> bool {
    let reply = connection.call_method(
        Some("org.kde.KWin"),
        "/Effects",
        Some("org.freedesktop.DBus.Properties"),
        "Get",
        &("org.kde.kwin.Effects", "activeEffects"),
    );
    let Ok(value) = reply.and_then(|reply| reply.body().deserialize::<zbus::zvariant::OwnedValue>()) else { return false };
    let Ok(active) = Vec::<String>::try_from(value) else { return false };
    active.iter().any(|effect| SWITCH_EFFECTS.contains(&effect.as_str()))
}

/// Takes the app's name on the session bus, which also keeps a second copy from starting,
/// and listens for the script's reports. `None` when another Snowhush already runs.
pub fn serve(shared: Arc<Shared>) -> zbus::Result<Option<Connection>> {
    let built = zbus::blocking::connection::Builder::session()?
        .name(SERVICE)?
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .serve_at(PATH, Reports { shared, switch: Arc::default() })?
        .build();
    match built {
        Ok(connection) => Ok(Some(connection)),
        Err(zbus::Error::NameTaken) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Puts the script into KWin, replacing a copy left by an earlier run. Loaded again whenever
/// Show in Screenshots changes, since the script applies it to the snow's surfaces.
pub fn load_script(connection: &Connection, show_in_screenshots: bool) -> Result<(), String> {
    unload_script(connection);
    let path = script_path().ok_or("no runtime folder")?;
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let text = format!("const APP_PID = {};\nconst SHOW_IN_SCREENSHOTS = {show_in_screenshots};\n{SCRIPT}", std::process::id());
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    let path = path.to_string_lossy().into_owned();
    let id: i32 = scripting(connection, "/Scripting", "org.kde.kwin.Scripting", "loadScript", &(path.as_str(), PLUGIN))?;
    if id < 0 {
        return Err("KWin refused the script".into());
    }
    let script = format!("/Scripting/Script{id}");
    connection.call_method(Some("org.kde.KWin"), script.as_str(), Some("org.kde.kwin.Script"), "run", &()).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn unload_script(connection: &Connection) {
    let _: Result<bool, String> = scripting(connection, "/Scripting", "org.kde.kwin.Scripting", "unloadScript", &(PLUGIN,));
}

fn scripting<B, R>(connection: &Connection, path: &str, interface: &str, method: &str, body: &B) -> Result<R, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'de> Deserialize<'de> + zbus::zvariant::Type,
{
    let reply = connection.call_method(Some("org.kde.KWin"), path, Some(interface), method, body).map_err(|error| error.to_string())?;
    reply.body().deserialize::<R>().map_err(|error| error.to_string())
}

fn script_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(PathBuf::from(base).join("snowhush").join("kwin-script.js"))
}
