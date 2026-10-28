//! What Wayland keeps from apps and KWin knows: where the windows are, which is in front, and the pointer.
//! A KWin script (`snowhush.js`) reports them to this app over D-Bus.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use calloop::ping::Ping;
use serde::Deserialize;
use zbus::blocking::Connection;

use crate::log;

pub const SERVICE: &str = "io.github.mbezruchko.Snowhush";
const PATH: &str = "/io/github/mbezruchko/Snowhush";
const SCRIPT: &str = include_str!("snowhush.js");
const PLUGIN: &str = "snowhush";

/// A display as KWin sees it, in logical pixels on the shared desktop: its bounds and its work area,
/// the part panels leave free, whose bottom is the ground the snow settles on.
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
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Report {
    pub screens: Vec<Display>,
    pub windows: Vec<Window>,
    /// Windows on other desktops or activities.
    pub elsewhere: Vec<String>,
    pub panels: Vec<Panel>,
    /// A window is being dragged or resized.
    pub moving: bool,
}

/// What the script last reported, read by the snow loop at each frame.
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

    /// The pointer in logical pixels on the shared desktop.
    pub fn cursor(&self) -> (f32, f32) {
        let bits = self.cursor.load(Ordering::Relaxed);
        (f32::from_bits((bits >> 32) as u32), f32::from_bits(bits as u32))
    }

    /// The latest report and its generation, which goes up with each one.
    pub fn report(&self) -> (u64, Option<Arc<Report>>) {
        self.report.lock().map(|report| report.clone()).unwrap_or((0, None))
    }

    /// Whether a drag or resize started since the last call.
    pub fn take_started_moving(&self) -> bool {
        self.started_moving.lock().map(|mut started| std::mem::take(&mut *started)).unwrap_or(false)
    }
}

fn pack(x: f32, y: f32) -> u64 {
    ((x.to_bits() as u64) << 32) | y.to_bits() as u64
}

struct Reports(Arc<Shared>);

#[zbus::interface(name = "io.github.mbezruchko.Snowhush.KWin")]
impl Reports {
    fn cursor(&self, position: &str) {
        let mut parts = position.split(' ').map(|part| part.parse::<f32>());
        if let (Some(Ok(x)), Some(Ok(y))) = (parts.next(), parts.next()) {
            self.0.cursor.store(pack(x, y), Ordering::Relaxed);
        }
    }

    fn windows(&self, state: &str) {
        let report: Report = match serde_json::from_str(state) {
            Ok(report) => report,
            Err(error) => {
                log!("unreadable window report: {error}");
                return;
            }
        };
        let Ok(mut current) = self.0.report.lock() else { return };
        let first = current.1.is_none();
        let was_moving = current.1.as_ref().is_some_and(|report| report.moving);
        let started = report.moving && !was_moving;
        *current = (current.0 + 1, Some(Arc::new(report)));
        drop(current);
        if started && let Ok(mut flag) = self.0.started_moving.lock() {
            *flag = true;
        }
        if first || started {
            self.0.wake.ping();
        }
    }
}

/// Takes the app's name on the session bus, which also keeps a second copy from starting,
/// and listens for the script's reports. `None` when another Snowhush already runs.
pub fn serve(shared: Arc<Shared>) -> zbus::Result<Option<Connection>> {
    let built = zbus::blocking::connection::Builder::session()?
        .name(SERVICE)?
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .serve_at(PATH, Reports(shared))?
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
    connection
        .call_method(Some("org.kde.KWin"), script.as_str(), Some("org.kde.kwin.Script"), "run", &())
        .map_err(|error| error.to_string())?;
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
