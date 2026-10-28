//! The windows KWin reported, turned into what the snow on one display needs, as the Windows tracker does.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use snowhush::geometry::Rect;
use snowhush::simulation::{WindowId, WindowSurface};

use crate::kwin::{Display, Report};

/// Breeze and its kin round a window's top corners by a few pixels; the exact radius belongs to the
/// decoration theme and isn't reported, so this is a close guess. Maximized windows stay square.
const CORNER_RADIUS: f32 = 6.0;

/// A rectangle in logical pixels on the shared desktop, y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Area {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn bounds(display: &Display) -> Area {
        Area { x: display.x, y: display.y, w: display.w, h: display.h }
    }

    pub fn work(display: &Display) -> Area {
        Area { x: display.ax, y: display.ay, w: display.aw, h: display.ah }
    }
}

/// The visible part of the panel along a display's bottom edge, which snow settles on like a window's top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shelf {
    pub id: WindowId,
    pub rect: Area,
    pub radius: f32,
}

/// The bottom panel's shelf. A floating panel's window takes in the gap around it, the same on every side,
/// while the work area leaves out only the panel's thickness: the gap is half the difference. The panel's
/// top is a gap above the work area's bottom, so the work area alone would bury the snow's foot in the panel.
/// Plasma lets a floating panel down to the edge while a window touches it, as a maximized one does; it then
/// fills the room the work area leaves.
pub fn shelf(report: &Report, display: &Display) -> Option<Shelf> {
    let (bounds, work) = (Area::bounds(display), Area::work(display));
    let room = bounds.bottom() - work.bottom();
    if room < 4.0 {
        return None;
    }
    let panel = report.panels.iter().find(|panel| {
        (panel.y + panel.h - bounds.bottom()).abs() < 1.0 && panel.w > panel.h && panel.x >= bounds.x - 1.0 && panel.x + panel.w <= bounds.right() + 1.0
    })?;
    let gap = ((panel.h - room) / 2.0).max(0.0);
    let touched = report.windows.iter().any(|window| {
        window.solid && window.y + window.h >= work.bottom() - 1.0 && window.x < panel.x + panel.w && window.x + window.w > panel.x
    });
    let rect = if gap >= 1.0 && !touched {
        Area { x: panel.x + gap, y: panel.y + gap, w: panel.w - 2.0 * gap, h: panel.h - 2.0 * gap }
    } else {
        Area { x: panel.x, y: work.bottom(), w: panel.w, h: room }
    };
    // A floating panel's corners are about as round as its gap is wide; one down at the edge is square.
    let radius = if gap >= 1.0 && !touched { gap as f32 } else { 0.0 };
    Some(Shelf { id: window_id(&panel.id), rect, radius })
}

/// KWin names windows by UUID; the simulation by number.
pub fn window_id(id: &str) -> WindowId {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish() as WindowId
}

/// The windows on one display that snow can settle on, in its overlay coordinates: y up from the bottom of
/// `area`, which the overlay covers one logical pixel to one. The shelf comes first: the panel is in front.
pub fn surfaces(report: &Report, area: &Area, shelf: Option<&Shelf>) -> Vec<WindowSurface> {
    let (width, height) = (area.w as f32, area.h as f32);
    let mut blockers: Vec<Rect> = Vec::new();
    let mut surfaces = Vec::new();
    if let Some(shelf) = shelf {
        let frame = local_rect(&shelf.rect, area);
        surfaces.push(WindowSurface { id: shelf.id, frame, blockers: Vec::new(), corner_radius: shelf.radius, corner_exponent: 2.0 });
        blockers.push(frame);
    }
    for window in &report.windows {
        if window.x + window.w <= area.x || window.x >= area.right() || window.y + window.h <= area.y || window.y >= area.bottom() {
            continue;
        }
        let frame = Rect {
            x: (window.x - area.x) as f32,
            y: (area.bottom() - (window.y + window.h)) as f32,
            width: window.w as f32,
            height: window.h as f32,
        };
        let room_above = (window.y - area.y) as f32 >= 6.0 && frame.max_y() < height;
        let on_screen = frame.max_x() > 0.0 && frame.min_x() < width && frame.max_y() > 0.0;
        let fills_screen = frame.width * frame.height >= width * height * 0.9;
        if room_above && on_screen && !fills_screen && window.solid && frame.width >= 80.0 && frame.height >= 40.0 {
            let rounded = window.decorated && !window.maximized;
            let radius = if rounded { CORNER_RADIUS } else { 0.0 }.min(frame.width / 2.0).min(frame.height / 2.0);
            let region = Rect { x: frame.x, y: frame.max_y() - radius, width: frame.width, height: 40.0 + radius };
            surfaces.push(WindowSurface {
                id: window_id(&window.id),
                frame,
                blockers: blockers.iter().filter(|b| intersects(b, &region)).copied().collect(),
                corner_radius: radius,
                corner_exponent: 2.0,
            });
        }
        blockers.push(frame);
    }
    surfaces
}

/// The full-screen window covering the display with these bounds, if any: the frontmost window on it, looking past
/// ones kept above the rest, is full screen and fills it.
pub fn covering<'a>(report: &'a Report, bounds: &Area) -> Option<&'a str> {
    for window in &report.windows {
        if window.x + window.w <= bounds.x || window.x >= bounds.right() || window.y + window.h <= bounds.y || window.y >= bounds.bottom() {
            continue;
        }
        let fills = window.x <= bounds.x && window.y <= bounds.y && window.x + window.w >= bounds.right() && window.y + window.h >= bounds.bottom();
        if fills && window.full {
            return Some(&window.class);
        }
        if !window.above {
            return None;
        }
    }
    None
}

/// Whether a listed window covers most of this work area, as when maximized or full screen.
pub fn fills(report: &Report, id: WindowId, area: &Area) -> bool {
    report.windows.iter().any(|window| {
        let width = ((window.x + window.w).min(area.right()) - window.x.max(area.x)).max(0.0);
        let height = ((window.y + window.h).min(area.bottom()) - window.y.max(area.y)).max(0.0);
        window_id(&window.id) == id && width * height * 10.0 >= area.w * area.h * 9.0
    })
}

/// Whether a window that left the list went to another desktop or activity, rather than closing or minimizing.
pub fn elsewhere(report: &Report, id: WindowId) -> bool {
    report.elsewhere.iter().any(|other| window_id(other) == id)
}

/// Which side edges of a display, left and right, have no display past them, so snow heaps up against them.
pub fn walls(display: &Display, displays: &[Display]) -> (bool, bool) {
    let (area, bounds) = (Area::work(display), Area::bounds(display));
    let beside = |x: f64, left_edge: bool| {
        displays.iter().any(|other| {
            other.name != display.name
                && other.y < bounds.bottom()
                && other.y + other.h > bounds.y
                && if left_edge { other.x + other.w == x } else { other.x == x }
        })
    };
    // A panel along a side stands between the snow and any display past it.
    (area.x != bounds.x || !beside(bounds.x, true), area.right() != bounds.right() || !beside(bounds.right(), false))
}

/// A rectangle on the shared desktop in the overlay coordinates of `area`.
pub fn local_rect(rect: &Area, area: &Area) -> Rect {
    Rect { x: (rect.x - area.x) as f32, y: (area.bottom() - rect.bottom()) as f32, width: rect.w as f32, height: rect.h as f32 }
}

fn intersects(a: &Rect, b: &Rect) -> bool {
    a.min_x() < b.max_x() && b.min_x() < a.max_x() && a.min_y() < b.max_y() && b.min_y() < a.max_y()
}
