//! What Wayland keeps from apps and KWin knows: where the windows are, which is in front, and the pointer.
//! A KWin script (`snowhush.js`) reports them to this app over D-Bus.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use zbus::blocking::Connection;

use crate::log;
use crate::source::{Report, Shared};

pub const SERVICE: &str = "io.github.mbezruchko.Snowhush";
const PATH: &str = "/io/github/mbezruchko/Snowhush";
const SCRIPT: &str = include_str!("snowhush.js");
const PLUGIN: &str = "snowhush";

struct Reports(Arc<Shared>);

#[zbus::interface(name = "io.github.mbezruchko.Snowhush.KWin")]
impl Reports {
    fn cursor(&self, position: &str) {
        let mut parts = position.split(' ').map(|part| part.parse::<f32>());
        if let (Some(Ok(x)), Some(Ok(y))) = (parts.next(), parts.next()) {
            self.0.set_cursor(x, y);
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
        self.0.publish(report);
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
