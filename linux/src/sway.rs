//! Sway's windows, from its IPC socket (`$SWAYSOCK`): the tree of windows, the workspaces showing and the
//! outputs. Sway announces windows opening, closing and changing workspace, but not a floating window
//! being dragged, so the tree is also asked for a few times a second, and at about the frame rate while
//! a floating window moves.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::log;
use crate::source::{Display, Report, Shared, Window};

const GET_WORKSPACES: u32 = 1;
const SUBSCRIBE: u32 = 2;
const GET_OUTPUTS: u32 = 3;
const GET_TREE: u32 = 4;
/// How often the tree is asked for while nothing moves, to notice a floating window starting to.
const CALM: Duration = Duration::from_millis(250);
const MOVING: Duration = Duration::from_millis(16);
/// How long after a floating window last moved it still counts as being dragged.
const SETTLE: Duration = Duration::from_millis(300);

#[derive(Deserialize, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Deserialize)]
struct Output {
    name: String,
    active: bool,
    rect: Rect,
}

#[derive(Deserialize)]
struct Workspace {
    name: String,
    output: String,
    visible: bool,
    /// What the workspace's windows may take, inside the bars and the outer gaps.
    rect: Rect,
}

#[derive(Deserialize)]
struct Node {
    id: i64,
    #[serde(rename = "type")]
    kind: String,
    name: Option<String>,
    /// Borders included, the title bar above it left out.
    rect: Rect,
    /// The title bar, which only its height is wanted of.
    deco_rect: Rect,
    /// Only windows have one; the containers that split the space between them don't.
    pid: Option<i64>,
    app_id: Option<String>,
    window_properties: Option<Properties>,
    #[serde(default)]
    fullscreen_mode: u8,
    /// Whether it shows: a tab behind another doesn't.
    #[serde(default)]
    visible: bool,
    #[serde(default)]
    nodes: Vec<Node>,
    /// Bottom to top.
    #[serde(default)]
    floating_nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Properties {
    class: Option<String>,
}

pub fn start(shared: Arc<Shared>) -> Result<(), String> {
    let path = std::env::var_os("SWAYSOCK").ok_or("no SWAYSOCK")?;
    let mut requests = UnixStream::connect(&path).map_err(|error| error.to_string())?;
    let mut events = UnixStream::connect(&path).map_err(|error| error.to_string())?;
    message(&mut events, SUBSCRIBE, br#"["window","workspace","output"]"#)?;
    let (changed, changes) = mpsc::channel();
    std::thread::Builder::new()
        .name("sway events".into())
        .spawn(move || {
            while receive(&mut events).is_ok() {
                if changed.send(()).is_err() {
                    return;
                }
            }
            log!("sway: the event socket closed");
        })
        .map_err(|error| error.to_string())?;
    std::thread::Builder::new()
        .name("sway".into())
        .spawn(move || {
            let mut last: Option<(Report, HashMap<String, Rect>)> = None;
            let mut moved_at: Option<Instant> = None;
            loop {
                let (mut report, floating) = match ask(&mut requests) {
                    Ok(answer) => answer,
                    Err(error) => {
                        log!("sway: {error}");
                        return;
                    }
                };
                let now = Instant::now();
                if let Some((_, before)) = &last
                    && floating.iter().any(|(id, rect)| before.get(id).is_some_and(|was| was != rect))
                {
                    moved_at = Some(now);
                }
                report.moving = moved_at.is_some_and(|at| now - at < SETTLE);
                if last.as_ref().map(|(report, _)| report) != Some(&report) {
                    shared.publish(report.clone());
                }
                let wait = if report.moving { MOVING } else { CALM };
                last = Some((report, floating));
                match changes.recv_timeout(wait) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => while changes.try_recv().is_ok() {},
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// The displays and windows as Sway has them now, and where each floating window is.
fn ask(stream: &mut UnixStream) -> Result<(Report, HashMap<String, Rect>), String> {
    let outputs: Vec<Output> = request(stream, GET_OUTPUTS)?;
    let workspaces: Vec<Workspace> = request(stream, GET_WORKSPACES)?;
    let tree: Node = request(stream, GET_TREE)?;
    let showing = |name: &Option<String>| workspaces.iter().any(|workspace| Some(&workspace.name) == name.as_ref() && workspace.visible);

    let mut report = Report::default();
    for output in outputs.iter().filter(|output| output.active) {
        let area =
            workspaces.iter().find(|workspace| workspace.output == output.name && workspace.visible).map_or(output.rect, |workspace| workspace.rect);
        report.screens.push(Display {
            name: output.name.clone(),
            x: output.rect.x,
            y: output.rect.y,
            w: output.rect.width,
            h: output.rect.height,
            ax: area.x,
            ay: area.y,
            aw: area.width,
            ah: area.height,
        });
    }
    // Front to back: a full-screen window, then the floating ones from the top, then the tiled ones, which
    // never overlap.
    let mut floating = HashMap::new();
    let (mut fullscreen, mut above, mut tiled) = (Vec::new(), Vec::new(), Vec::new());
    for workspace in tree.nodes.iter().flat_map(|output| &output.nodes).filter(|node| node.kind == "workspace") {
        // Among the workspaces not showing is the scratchpad's, whose windows are hidden until shown.
        if !showing(&workspace.name) {
            for node in workspace.nodes.iter().chain(&workspace.floating_nodes) {
                windows_in(node, &mut |window| report.elsewhere.push(window.id.to_string()));
            }
            continue;
        }
        for node in workspace.floating_nodes.iter().rev() {
            windows_in(node, &mut |window| {
                floating.insert(window.id.to_string(), window.rect);
                if window.fullscreen_mode != 0 { fullscreen.push(reported(window)) } else { above.push(reported(window)) }
            });
        }
        for node in &workspace.nodes {
            windows_in(node, &mut |window| {
                if window.fullscreen_mode != 0 {
                    fullscreen.push(reported(window));
                } else if window.visible {
                    tiled.push(reported(window));
                }
            });
        }
    }
    report.windows = fullscreen.into_iter().chain(above).chain(tiled).collect();
    Ok((report, floating))
}

/// Every window in a container, however deep it is split.
fn windows_in<'a>(node: &'a Node, found: &mut impl FnMut(&'a Node)) {
    if node.pid.is_some() {
        found(node);
    }
    for child in node.nodes.iter().chain(&node.floating_nodes) {
        windows_in(child, found);
    }
}

/// A window as the snow sees it: its title bar on top of it, and square corners, as Sway draws them.
fn reported(node: &Node) -> Window {
    let title = node.deco_rect.height.max(0.0);
    Window {
        id: node.id.to_string(),
        class: node.app_id.clone().or_else(|| node.window_properties.as_ref().and_then(|properties| properties.class.clone())).unwrap_or_default(),
        x: node.rect.x,
        y: node.rect.y - title,
        w: node.rect.width,
        h: node.rect.height + title,
        solid: true,
        full: node.fullscreen_mode != 0,
        above: false,
        decorated: title > 0.0,
        maximized: false,
        radius: Some(0.0),
    }
}

fn request<T: DeserializeOwned>(stream: &mut UnixStream, kind: u32) -> Result<T, String> {
    message(stream, kind, b"")?;
    let (_, payload) = receive(stream)?;
    serde_json::from_slice(&payload).map_err(|error| error.to_string())
}

/// Sends a message in the i3 IPC framing: the magic, the payload's length and the message's type, both in
/// the machine's byte order, then the payload.
fn message(stream: &mut UnixStream, kind: u32, payload: &[u8]) -> Result<(), String> {
    let mut frame = b"i3-ipc".to_vec();
    frame.extend((payload.len() as u32).to_ne_bytes());
    frame.extend(kind.to_ne_bytes());
    frame.extend(payload);
    stream.write_all(&frame).map_err(|error| error.to_string())
}

fn receive(stream: &mut UnixStream) -> Result<(u32, Vec<u8>), String> {
    let mut header = [0u8; 14];
    stream.read_exact(&mut header).map_err(|error| error.to_string())?;
    if &header[..6] != b"i3-ipc" {
        return Err("not an i3 IPC message".into());
    }
    let length = u32::from_ne_bytes(header[6..10].try_into().unwrap_or_default()) as usize;
    let kind = u32::from_ne_bytes(header[10..14].try_into().unwrap_or_default());
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).map_err(|error| error.to_string())?;
    Ok((kind, payload))
}
