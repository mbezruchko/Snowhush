//! Hyprland's windows, from its IPC sockets in `$XDG_RUNTIME_DIR/hypr/<instance>/`: the monitors, the windows
//! and the pointer, asked on `.socket.sock`, one request to a connection. Hyprland announces windows opening,
//! closing, focusing and changing workspace on `.socket2.sock`, but neither a window being dragged nor the
//! pointer moving, so both are asked for: the windows ten times a second, and at about the frame rate while one
//! moves and for a moment after anything Hyprland announces; the pointer at the frame rate while it moves.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::log;
use crate::source::{Display, Report, Shared, Window};

/// How often the windows are asked for while nothing moves, to notice one starting to.
const CALM: Duration = Duration::from_millis(100);
const MOVING: Duration = Duration::from_millis(16);
/// How long after any event the windows are asked for as often as while moving: a drag starts with a click,
/// which focuses the window, and the drag itself sends nothing.
const ALERT: Duration = Duration::from_secs(1);
/// How long after a window last moved it still counts as being dragged.
const SETTLE: Duration = Duration::from_millis(300);
/// How often the pointer is asked for while it moves, and while it rests.
const POINTER_MOVING: Duration = Duration::from_millis(8);
const POINTER_RESTING: Duration = Duration::from_millis(100);

#[derive(Deserialize)]
struct Monitor {
    name: String,
    /// On the shared desktop, in logical pixels.
    x: f64,
    y: f64,
    /// In the mode's pixels, before scale and rotation.
    width: f64,
    height: f64,
    scale: f64,
    transform: u32,
    /// What bars keep: left, top, right, bottom, in logical pixels.
    reserved: [f64; 4],
    #[serde(default)]
    disabled: bool,
    #[serde(rename = "activeWorkspace")]
    active_workspace: WorkspaceRef,
    /// A special workspace shown over the active one; it has no name while none is.
    #[serde(rename = "specialWorkspace")]
    special_workspace: WorkspaceRef,
}

/// A workspace, known by its name: special workspaces have no id.
#[derive(Deserialize)]
struct WorkspaceRef {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct Client {
    address: String,
    mapped: bool,
    hidden: bool,
    /// Inside the border, in logical pixels on the shared desktop.
    at: [f64; 2],
    size: [f64; 2],
    workspace: WorkspaceRef,
    floating: bool,
    class: String,
    pinned: bool,
    /// 1 maximized, 2 full screen.
    fullscreen: i64,
    /// 0 for the window focused last: the closest Hyprland tells to which is in front.
    #[serde(rename = "focusHistoryID")]
    focus_history: i64,
}

#[derive(Deserialize, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
struct IntOption {
    int: Option<i64>,
}

/// The border and corner radius every window gets, from the config.
#[derive(Clone, Copy)]
struct Decoration {
    border: f64,
    rounding: f64,
}

/// Hyprland's own, for a config that can't be asked.
const PLAIN: Decoration = Decoration { border: 1.0, rounding: 0.0 };

/// The Hyprland instance the app's Wayland connection goes to, if it is one: the folder with its sockets.
/// A compositor started inside another passes the outer one's variables on, so the instance has to be the one
/// serving the very Wayland socket the app uses, which its lock file names after its process id.
pub fn instance() -> Option<PathBuf> {
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let folder = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("hypr").join(signature);
    let lock = std::fs::read_to_string(folder.join("hyprland.lock")).ok()?;
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    (lock.lines().nth(1) == Some(display.as_str())).then_some(folder)
}

pub fn start(shared: Arc<Shared>, folder: PathBuf) -> Result<(), String> {
    let requests = folder.join(".socket.sock");
    let mut decoration = decoration(&requests).unwrap_or_else(|error| {
        log!("hyprland: {error}; windows taken as Hyprland draws them by default");
        PLAIN
    });
    let events = UnixStream::connect(folder.join(".socket2.sock")).map_err(|error| error.to_string())?;
    let (announced, announcements) = mpsc::channel::<String>();
    std::thread::Builder::new()
        .name("hyprland events".into())
        .spawn(move || {
            for line in BufReader::new(events).lines() {
                let Ok(line) = line else { break };
                // A window's new title moves nothing, and terminals and browsers retitle themselves all the time.
                if line.starts_with("windowtitle") {
                    continue;
                }
                if announced.send(line).is_err() {
                    return;
                }
            }
            log!("hyprland: the event socket closed");
        })
        .map_err(|error| error.to_string())?;

    let pointer = requests.clone();
    let pointing = shared.clone();
    std::thread::Builder::new()
        .name("hyprland pointer".into())
        .spawn(move || {
            let mut last: Option<Point> = None;
            let mut moved_at = Instant::now();
            loop {
                let Ok(point) = request::<Point>(&pointer, "j/cursorpos") else { return };
                pointing.set_cursor(point.x as f32, point.y as f32);
                if last.as_ref() != Some(&point) {
                    moved_at = Instant::now();
                    last = Some(point);
                }
                std::thread::sleep(if moved_at.elapsed() < SETTLE { POINTER_MOVING } else { POINTER_RESTING });
            }
        })
        .map_err(|error| error.to_string())?;

    std::thread::Builder::new()
        .name("hyprland".into())
        .spawn(move || {
            let mut last: Option<(Report, HashMap<String, [f64; 4]>)> = None;
            let mut moved_at: Option<Instant> = None;
            let mut event_at: Option<Instant> = None;
            loop {
                let (mut report, places) = match ask(&requests, decoration) {
                    Ok(answer) => answer,
                    Err(error) => {
                        log!("hyprland: {error}");
                        return;
                    }
                };
                let now = Instant::now();
                if let Some((_, before)) = &last
                    && places.iter().any(|(id, place)| before.get(id).is_some_and(|was| was != place))
                {
                    moved_at = Some(now);
                }
                report.moving = moved_at.is_some_and(|at| now - at < SETTLE);
                if last.as_ref().map(|(report, _)| report) != Some(&report) {
                    shared.publish(report.clone());
                }
                let alert = event_at.is_some_and(|at| now - at < ALERT);
                let wait = if report.moving || alert { MOVING } else { CALM };
                last = Some((report, places));
                match announcements.recv_timeout(wait) {
                    Ok(line) => {
                        event_at = Some(Instant::now());
                        let mut reloaded = line.starts_with("configreloaded");
                        while let Ok(line) = announcements.try_recv() {
                            reloaded |= line.starts_with("configreloaded");
                        }
                        if reloaded && let Ok(fresh) = self::decoration(&requests) {
                            decoration = fresh;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn decoration(socket: &Path) -> Result<Decoration, String> {
    let option = |name: &str| request::<IntOption>(socket, &format!("j/getoption {name}")).map(|option| option.int.unwrap_or(0).max(0) as f64);
    Ok(Decoration { border: option("general:border_size")?, rounding: option("decoration:rounding")? })
}

/// The displays and windows as Hyprland has them now, and where each window is.
fn ask(socket: &Path, decoration: Decoration) -> Result<(Report, HashMap<String, [f64; 4]>), String> {
    let monitors: Vec<Monitor> = request(socket, "j/monitors")?;
    let clients: Vec<Client> = request(socket, "j/clients")?;
    let mut report = Report::default();
    let mut showing = Vec::new();
    let mut specials = Vec::new();
    for monitor in monitors.iter().filter(|monitor| !monitor.disabled) {
        let (width, height) = if monitor.transform % 2 == 1 { (monitor.height, monitor.width) } else { (monitor.width, monitor.height) };
        let (w, h) = (width / monitor.scale.max(0.1), height / monitor.scale.max(0.1));
        let [left, top, right, bottom] = monitor.reserved;
        report.screens.push(Display {
            name: monitor.name.clone(),
            x: monitor.x,
            y: monitor.y,
            w,
            h,
            ax: monitor.x + left,
            ay: monitor.y + top,
            aw: w - left - right,
            ah: h - top - bottom,
        });
        showing.push(monitor.active_workspace.name.as_str());
        specials.push(monitor.special_workspace.name.as_str());
    }
    showing.retain(|name| !name.is_empty());
    specials.retain(|name| !name.is_empty());

    let mut places = HashMap::new();
    let mut shown: Vec<&Client> = Vec::new();
    for client in clients.iter().filter(|client| client.mapped && !client.hidden) {
        let workspace = client.workspace.name.as_str();
        if !showing.contains(&workspace) && !specials.contains(&workspace) {
            report.elsewhere.push(client.address.clone());
            continue;
        }
        places.insert(client.address.clone(), [client.at[0], client.at[1], client.size[0], client.size[1]]);
        shown.push(client);
    }
    // Front to back: the special workspace's windows, which show over the rest, a full-screen window, the pinned
    // ones, the floating ones by how lately each was focused, which raises them, then the tiled ones, which never
    // overlap.
    let layer = |client: &Client| {
        if specials.contains(&client.workspace.name.as_str()) {
            0
        } else if client.fullscreen >= 2 {
            1
        } else if client.pinned {
            2
        } else if client.floating {
            3
        } else {
            4
        }
    };
    shown.sort_by_key(|client| (layer(client), client.focus_history));
    report.windows = shown.into_iter().map(|client| reported(client, decoration)).collect();
    Ok((report, places))
}

/// A window as the snow sees it: its border around it, and its corners rounded as the config has them, the
/// border's outer edge a border wider. A full-screen window has neither.
fn reported(client: &Client, decoration: Decoration) -> Window {
    let full = client.fullscreen >= 2;
    let border = if full { 0.0 } else { decoration.border };
    let radius = if full || decoration.rounding <= 0.0 { 0.0 } else { decoration.rounding + border };
    Window {
        id: client.address.clone(),
        class: client.class.clone(),
        x: client.at[0] - border,
        y: client.at[1] - border,
        w: client.size[0] + 2.0 * border,
        h: client.size[1] + 2.0 * border,
        solid: true,
        full,
        above: client.pinned,
        decorated: border > 0.0,
        maximized: client.fullscreen == 1,
        radius: Some(radius as f32),
    }
}

/// One request on a connection of its own, which Hyprland answers and closes.
fn request<T: DeserializeOwned>(socket: &Path, command: &str) -> Result<T, String> {
    let mut stream = UnixStream::connect(socket).map_err(|error| error.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(|error| error.to_string())?;
    stream.write_all(command.as_bytes()).map_err(|error| error.to_string())?;
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).map_err(|error| error.to_string())?;
    serde_json::from_slice(&reply).map_err(|error| format!("{command}: {error}"))
}
