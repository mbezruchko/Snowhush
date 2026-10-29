//! niri's windows, from its event stream on `$NIRI_SOCKET`. niri tells where a floating window is, but not
//! where the windows tiled in its scrolling strip are, so only floating windows collect snow; they always lie
//! in front of the tiled ones, and not while one is dragged with the mouse. The overview shows every workspace
//! at once, small, so the snow hides while it is open.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::log;
use crate::source::{Display, Report, Shared, Window};

/// How long after a floating window last moved it still counts as being dragged.
const SETTLE: Duration = Duration::from_millis(300);

#[derive(Deserialize, Clone)]
struct NiriWindow {
    id: u64,
    app_id: Option<String>,
    workspace_id: Option<u64>,
    is_floating: bool,
    focus_timestamp: Option<Timestamp>,
    layout: Layout,
}

#[derive(Deserialize, Clone, Copy, PartialEq, PartialOrd)]
struct Timestamp {
    secs: u64,
    nanos: u32,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
struct Layout {
    tile_size: (f64, f64),
    /// From the top left of the output, bars included; only floating windows have one.
    tile_pos_in_workspace_view: Option<(f64, f64)>,
}

#[derive(Deserialize, Clone)]
struct Workspace {
    id: u64,
    output: Option<String>,
    is_active: bool,
}

#[derive(Deserialize)]
struct Output {
    logical: Option<Logical>,
}

#[derive(Deserialize, Clone, Copy)]
struct Logical {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Default)]
struct State {
    windows: HashMap<u64, NiriWindow>,
    workspaces: Vec<Workspace>,
    outputs: HashMap<String, Logical>,
    overview: bool,
}

pub fn start(shared: Arc<Shared>) -> Result<(), String> {
    let path = std::env::var_os("NIRI_SOCKET").ok_or("no NIRI_SOCKET")?;
    let mut stream = UnixStream::connect(&path).map_err(|error| error.to_string())?;
    stream.write_all(b"\"EventStream\"\n").map_err(|error| error.to_string())?;
    let displays = outputs(&path)?;
    let (events, received) = mpsc::channel::<Value>();
    std::thread::Builder::new()
        .name("niri events".into())
        .spawn(move || {
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
                if events.send(event).is_err() {
                    return;
                }
            }
            log!("niri: the event stream closed");
        })
        .map_err(|error| error.to_string())?;
    std::thread::Builder::new()
        .name("niri".into())
        .spawn(move || {
            let mut state = State { outputs: displays, ..State::default() };
            let mut last = None;
            let mut moved_at: Option<Instant> = None;
            loop {
                match received.recv_timeout(SETTLE) {
                    Ok(event) => {
                        if state.apply(&event) {
                            moved_at = Some(Instant::now());
                        }
                        if event.get("WorkspacesChanged").is_some()
                            && let Ok(outputs) = outputs(&path)
                        {
                            state.outputs = outputs;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                let mut report = state.report();
                report.moving = moved_at.is_some_and(|at| at.elapsed() < SETTLE);
                if last.as_ref() != Some(&report) {
                    shared.publish(report.clone());
                    last = Some(report);
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

impl State {
    /// Takes in one event; returns whether a floating window moved.
    fn apply(&mut self, event: &Value) -> bool {
        if let Some(windows) = event.pointer("/WindowsChanged/windows").and_then(parse::<Vec<NiriWindow>>) {
            self.windows = windows.into_iter().map(|window| (window.id, window)).collect();
        } else if let Some(window) = event.pointer("/WindowOpenedOrChanged/window").and_then(parse::<NiriWindow>) {
            self.windows.insert(window.id, window);
        } else if let Some(id) = event.pointer("/WindowClosed/id").and_then(Value::as_u64) {
            self.windows.remove(&id);
        } else if let Some(changes) = event.pointer("/WindowLayoutsChanged/changes").and_then(Value::as_array) {
            let mut moved = false;
            for change in changes {
                let (Some(id), Some(layout)) = (change.get(0).and_then(Value::as_u64), change.get(1).and_then(parse::<Layout>)) else { continue };
                if let Some(window) = self.windows.get_mut(&id) {
                    moved |= window.is_floating && window.layout.tile_pos_in_workspace_view != layout.tile_pos_in_workspace_view;
                    window.layout = layout;
                }
            }
            return moved;
        } else if let Some(timestamp) = event.pointer("/WindowFocusTimestampChanged/focus_timestamp").and_then(parse::<Timestamp>) {
            if let Some(window) = event.pointer("/WindowFocusTimestampChanged/id").and_then(Value::as_u64).and_then(|id| self.windows.get_mut(&id)) {
                window.focus_timestamp = Some(timestamp);
            }
        } else if let Some(workspaces) = event.pointer("/WorkspacesChanged/workspaces").and_then(parse::<Vec<Workspace>>) {
            self.workspaces = workspaces;
        } else if let Some(id) = event.pointer("/WorkspaceActivated/id").and_then(Value::as_u64) {
            let output = self.workspaces.iter().find(|workspace| workspace.id == id).and_then(|workspace| workspace.output.clone());
            for workspace in self.workspaces.iter_mut().filter(|workspace| workspace.output == output) {
                workspace.is_active = workspace.id == id;
            }
        } else if let Some(open) = event.pointer("/OverviewOpenedOrClosed/is_open").and_then(Value::as_bool) {
            self.overview = open;
        }
        false
    }

    fn report(&self) -> Report {
        let mut report = Report { hidden: self.overview, ..Report::default() };
        for (name, area) in &self.outputs {
            report.screens.push(Display {
                name: name.clone(),
                x: area.x,
                y: area.y,
                w: area.width,
                h: area.height,
                ax: area.x,
                ay: area.y,
                aw: area.width,
                ah: area.height,
            });
        }
        let mut showing: Vec<(&NiriWindow, Logical)> = Vec::new();
        for window in self.windows.values() {
            // A window dragged with the mouse is on no workspace and has no place until it is dropped, so its
            // snow waits for it there, as for a window on another workspace.
            let Some(workspace) = self.workspaces.iter().find(|workspace| Some(workspace.id) == window.workspace_id) else {
                if window.workspace_id.is_none() {
                    report.elsewhere.push(window.id.to_string());
                }
                continue;
            };
            if !workspace.is_active {
                report.elsewhere.push(window.id.to_string());
                continue;
            }
            if let (true, Some(output)) = (window.is_floating, workspace.output.as_ref().and_then(|name| self.outputs.get(name))) {
                showing.push((window, *output));
            }
        }
        report.screens.sort_by(|a, b| a.name.cmp(&b.name));
        report.elsewhere.sort();
        // The last focused floating window is raised over the others.
        showing
            .sort_by(|(a, _), (b, _)| b.focus_timestamp.partial_cmp(&a.focus_timestamp).unwrap_or(std::cmp::Ordering::Equal).then(a.id.cmp(&b.id)));
        for (window, output) in showing {
            let Some((x, y)) = window.layout.tile_pos_in_workspace_view else { continue };
            report.windows.push(Window {
                id: window.id.to_string(),
                class: window.app_id.clone().unwrap_or_default(),
                x: output.x + x,
                y: output.y + y,
                w: window.layout.tile_size.0,
                h: window.layout.tile_size.1,
                solid: true,
                full: false,
                above: false,
                decorated: false,
                maximized: false,
                radius: Some(0.0),
            });
        }
        report
    }
}

fn parse<T: DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

/// The outputs and where they sit, asked on a connection of its own, which niri answers once.
fn outputs(path: &std::ffi::OsStr) -> Result<HashMap<String, Logical>, String> {
    let mut stream = UnixStream::connect(path).map_err(|error| error.to_string())?;
    stream.write_all(b"\"Outputs\"\n").map_err(|error| error.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).map_err(|error| error.to_string())?;
    let reply: Value = serde_json::from_str(&reply).map_err(|error| error.to_string())?;
    let outputs: HashMap<String, Output> =
        reply.pointer("/Ok/Outputs").and_then(|outputs| serde_json::from_value(outputs.clone()).ok()).ok_or("no outputs in niri's answer")?;
    Ok(outputs.into_iter().filter_map(|(name, output)| Some((name, output.logical?))).collect())
}
