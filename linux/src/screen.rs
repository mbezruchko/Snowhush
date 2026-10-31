//! The snow on one display: its layer surface, drawing surface and simulation.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use smithay_client_toolkit::compositor::{CompositorState, FrameCallbackData, Region};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface};
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_output::WlOutput;

use snowhush_core::geometry::V2;
use snowhush_core::simulation::{Pace, SnowSimulation, WindowId, WindowSurface};

use crate::light::{self, Backdrop, Shade};
use crate::log;
use crate::render::{Ambient, Gpu, LANTERN_REACH, Surface};
use crate::snow::App;
use crate::source::Report;
use crate::windows::{self, Area, Shelf};

/// How much the snow may spend on drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerBudget {
    Full,
    /// On battery: no frame rates above 60, which are hard to tell apart in snow anyway.
    Battery,
    /// Power saver: a little less snow, drawn a little less often.
    Saver,
}

/// How long a frame may go unshown before the display counts as off or hidden; the snow then only ticks over.
const STALLED: Duration = Duration::from_secs(1);

pub struct Screen {
    pub name: String,
    pub output: WlOutput,
    pub simulation: SnowSimulation,
    pub next_due: Instant,
    /// The whole display, in logical pixels on the shared desktop.
    pub bounds: Area,
    layer: LayerSurface,
    surface: Option<Surface>,
    /// Space left free at the display's edges, top, right, bottom, left: the panels the work area leaves out.
    margins: (i32, i32, i32, i32),
    /// The bottom panel's visible part, which the snow lies on; the ground is then the display's bottom edge.
    shelf: Option<Shelf>,
    size: (u32, u32),
    last_step: Option<Instant>,
    idle_frames: u32,
    interactive_rate: u32,
    surfaces: Vec<WindowSurface>,
    windows_seen: u64,
    windows_changed_at: Instant,
    covered: bool,
    /// When the frame now waiting for the compositor was drawn.
    shown_pending: Option<Instant>,
    backdrop: Option<Backdrop>,
    shadow_tint: [f32; 4],
    /// Where the cursor's candle was last drawn, in overlay coordinates.
    lantern: V2,
    /// Whether the last frame drawn showed Halloween's pumpkins.
    pumpkins_shown: bool,
}

impl Screen {
    /// `behind` puts the snow under the windows, just over the desktop; without it the snow is above them,
    /// though under menus, notifications and full-screen windows.
    pub fn new(
        app: (&CompositorState, &LayerShell, &QueueHandle<App>),
        output: WlOutput,
        name: String,
        bounds: Area,
        refresh: u32,
        simulation: SnowSimulation,
        behind: bool,
    ) -> Screen {
        let (compositor, layer_shell, qh) = app;
        let surface = compositor.create_surface(qh);
        // An empty input region: every click and scroll goes through to what is under the snow.
        if let Ok(region) = Region::new(compositor) {
            surface.set_input_region(Some(region.wl_region()));
        }
        let layer = layer_shell.create_layer_surface(qh, surface, layer_for(behind), Some("snowhush"), Some(&output));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        // Placed by its margins alone, over panels' reserved space as much as anything else.
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        Screen {
            name,
            output,
            simulation,
            next_due: Instant::now(),
            bounds,
            layer,
            surface: None,
            margins: (0, 0, 0, 0),
            shelf: None,
            size: (bounds.w as u32, bounds.h as u32),
            last_step: None,
            idle_frames: 0,
            interactive_rate: refresh.clamp(60, 120),
            surfaces: Vec::new(),
            windows_seen: 0,
            windows_changed_at: Instant::now(),
            covered: false,
            shown_pending: None,
            backdrop: None,
            shadow_tint: [0.0; 4],
            lantern: V2::new(-1e6, -1e6),
            pumpkins_shown: false,
        }
    }

    pub fn is(&self, layer: &LayerSurface) -> bool {
        self.layer.wl_surface() == layer.wl_surface()
    }

    pub fn owns(&self, surface: &wayland_client::protocol::wl_surface::WlSurface) -> bool {
        self.layer.wl_surface() == surface
    }

    /// The compositor settled the surface's size; the snow takes the space it was given.
    pub fn configure(&mut self, gpu: &Gpu, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        match &mut self.surface {
            Some(surface) => gpu.resize(surface, width as i32, height as i32),
            None => match gpu.surface(self.layer.wl_surface(), width as i32, height as i32) {
                Ok(surface) => self.surface = Some(surface),
                Err(error) => log!("{}: no surface: {error}", self.name),
            },
        }
        if (width, height) != self.size {
            log!("{}: {width}x{height}", self.name);
        }
        self.size = (width, height);
        self.simulation.resize(width as f32, height as f32);
        self.resume();
    }

    /// Fits the snow to the display's work area, so the ground lies on top of the panel; with the bottom
    /// panel as a shelf, the snow reaches down to the display's edge instead, around and behind the panel.
    pub fn place(&mut self, bounds: Area, work: Area, shelf: Option<Shelf>) {
        self.bounds = bounds;
        if shelf != self.shelf {
            log!("{}: panel {shelf:?}", self.name);
            self.shelf = shelf;
            self.simulation.shelf = shelf.map(|shelf| shelf.id);
            self.windows_seen = 0;
        }
        let margins = (
            (work.y - bounds.y).round() as i32,
            (bounds.right() - work.right()).round() as i32,
            if shelf.is_some() { 0 } else { (bounds.bottom() - work.bottom()).round() as i32 },
            (work.x - bounds.x).round() as i32,
        );
        if margins == self.margins || margins.0 < 0 || margins.1 < 0 || margins.2 < 0 || margins.3 < 0 {
            return;
        }
        log!("{}: work area {work:?}", self.name);
        self.margins = margins;
        self.layer.set_margin(margins.0, margins.1, margins.2, margins.3);
        self.layer.commit();
    }

    pub fn set_behind(&mut self, behind: bool) {
        self.layer.set_layer(layer_for(behind));
        self.layer.commit();
        self.resume();
    }

    /// The overlay's place on the shared desktop: where the margins put it, as big as the compositor made it.
    fn area(&self) -> Area {
        Area { x: self.bounds.x + self.margins.3 as f64, y: self.bounds.y + self.margins.0 as f64, w: self.size.0 as f64, h: self.size.1 as f64 }
    }

    /// Steps the snow and draws it if anything visible changed; returns whether it drew.
    /// `report` is KWin's latest, its generation and whether a window is being dragged; the windows in it
    /// only collect snow with `snow_on_windows`, the panel always.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        gpu: &mut Gpu,
        qh: &QueueHandle<App>,
        now: Instant,
        budget: PowerBudget,
        ambient: Ambient,
        time: f32,
        cursor: (f32, f32),
        report: Option<(&Report, u64, bool)>,
        snow_on_windows: bool,
    ) -> Result<bool, String> {
        let dt = self.last_step.map_or(1.0 / 60.0, |last| (now - last).as_secs_f32().min(0.05));
        self.last_step = Some(now);
        if let Some((report, generation, moving)) = report {
            self.follow(report, generation, moving, snow_on_windows, now);
            if let Some(slide) = report.slide {
                self.simulation.space_slid(slide.velocity(now));
            }
        }
        let changed = self.simulation.step(dt, self.pointer(cursor));
        // The candle follows the cursor even over snow that lies still.
        let lantern = self.local(cursor.0, cursor.1);
        let lantern_moved = ambient.night > 0.0 && (lantern - self.lantern).length() > 0.5 && (self.lit_by(lantern) || self.lit_by(self.lantern));
        self.lantern = lantern;

        let fastest = if budget == PowerBudget::Full { self.interactive_rate } else { self.interactive_rate.min(60) };
        // 24 only where it divides the display's rate evenly; elsewhere it would judder.
        let calm = if budget == PowerBudget::Saver && self.interactive_rate % 24 == 0 { 24 } else { 30 };
        let rate = match self.simulation.pace() {
            Pace::Calm => calm,
            Pace::Lively => fastest.min(60),
            Pace::Interactive => fastest,
        };
        let rate = if lantern_moved { rate.max(fastest.min(60)) } else { rate };
        self.next_due = now + Duration::from_secs_f64(1.0 / rate as f64);
        self.idle_frames = if changed || lantern_moved { 0 } else { self.idle_frames + 1 };
        // A still pile only needs a slow refresh for its glints, and the pumpkins for their candles' flicker; once
        // they are gone, until a frame without them is shown.
        let pumpkins = !self.simulation.pumpkins().is_empty();
        let slow_refresh = self.simulation.has_snow() || pumpkins || self.pumpkins_shown;
        if !(changed || lantern_moved || (slow_refresh && self.idle_frames % 2 == 0)) {
            return Ok(false);
        }
        // The compositor hasn't shown the last frame yet: drawing another would only be thrown away.
        if let Some(pending) = self.shown_pending {
            if now - pending < STALLED {
                return Ok(false);
            }
            // The display is off or the surface hidden: tick over slowly until frames are shown again.
            self.next_due = now + Duration::from_millis(250);
        }
        let Some(surface) = &self.surface else { return Ok(false) };
        let wl_surface = self.layer.wl_surface();
        wl_surface.frame(qh, FrameCallbackData(wl_surface.clone()));
        self.shown_pending = Some(now);
        let panel = self.shelf.map(|shelf| {
            // Below the rounded corners only, so snow curling round them stays in front.
            let r = windows::local_rect(&shelf.rect, &self.area());
            [r.min_x(), r.min_y(), r.max_x(), r.max_y() - shelf.radius]
        });
        gpu.draw(surface, &self.simulation, Ambient { shadow_tint: self.shadow_tint, lantern, ..ambient }, panel, time)?;
        self.pumpkins_shown = pumpkins;
        Ok(true)
    }

    /// Whether a candle at this point, in overlay coordinates, lights anything on this display.
    fn lit_by(&self, at: V2) -> bool {
        let reach = LANTERN_REACH * 3.0;
        at.x > -reach && at.y > -reach && at.x < self.simulation.width() + reach && at.y < self.simulation.height() + reach
    }

    /// The compositor showed the last frame.
    pub fn shown(&mut self) {
        self.shown_pending = None;
    }

    /// The pointer in overlay coordinates. Down among the panel's icons it is behind the panel, as far as the
    /// snow is concerned: it would only dig into the snow that lies hidden there.
    fn pointer(&self, (x, y): (f32, f32)) -> V2 {
        if let Some(shelf) = &self.shelf {
            let r = &shelf.rect;
            if x as f64 >= r.x && (x as f64) < r.right() && y as f64 > r.y + 12.0 && (y as f64) < r.bottom() {
                return V2::new(-1e6, -1e6);
            }
        }
        self.local(x, y)
    }

    /// Hands the simulation the windows on this display when they changed.
    fn follow(&mut self, report: &Report, generation: u64, moving: bool, snow_on_windows: bool, now: Instant) {
        if generation == self.windows_seen {
            return;
        }
        self.windows_seen = generation;
        let area = self.area();
        let listed = if snow_on_windows { report } else { &Report::default() };
        let mut surfaces = windows::surfaces(listed, &area, self.shelf.as_ref());
        if report.switching {
            // Windows that came with the new desktop get their snow back once they have slid in; windows on every
            // desktop keep theirs throughout.
            surfaces.retain(|surface| self.surfaces.iter().any(|shown| shown.id == surface.id));
        }
        self.simulation.covers = windows::covers(listed, &area);
        if surfaces == self.surfaces {
            return;
        }
        let since = (now - self.windows_changed_at).as_secs_f32();
        // A window that moves outside a drag, as when it is maximized or tiled, jumps there in one go:
        // its snow is left behind instead of being carried along.
        let interval = if moving || since < 0.1 { since.min(0.25) } else { 1.0 / 60.0 };
        // Snow waits for a window gone to another desktop, or maximized or full screen and so without room for it.
        let elsewhere: HashSet<WindowId> = self
            .surfaces
            .iter()
            .map(|surface| surface.id)
            .filter(|&id| !surfaces.iter().any(|surface| surface.id == id) && (windows::elsewhere(report, id) || windows::fills(report, id, &area)))
            .collect();
        self.simulation.update_windows(&surfaces, interval, &HashSet::new(), &elsewhere, false);
        self.surfaces = surfaces;
        self.windows_changed_at = now;
    }

    /// Lets the snow on windows fall, keeping what lies on the panel.
    pub fn drop_window_snow(&mut self) {
        let surfaces = windows::surfaces(&Report::default(), &self.area(), self.shelf.as_ref());
        self.simulation.update_windows(&surfaces, 0.1, &HashSet::new(), &HashSet::new(), false);
        self.simulation.covers.clear();
        self.surfaces = surfaces;
        self.windows_seen = 0;
    }

    /// Reads the windows again at the next frame, as when snow on windows comes back on.
    pub fn rewatch(&mut self) {
        self.windows_seen = 0;
    }

    /// Hides the snow while a full-screen app covers the display, and stops it, as it stops over a full-screen app on a Mac.
    pub fn set_covered(&mut self, gpu: &Gpu, covered: bool) {
        if covered == self.covered {
            return;
        }
        self.covered = covered;
        if covered {
            if let Some(surface) = &self.surface {
                let _ = gpu.clear(surface);
            }
        } else {
            self.resume();
        }
    }

    pub fn covered(&self) -> bool {
        self.covered
    }

    /// Picks up the shadow colour again if the desktop picture changed.
    pub fn update_backdrop(&mut self, shades: &[Shade]) {
        let shade = light::shade_of(shades, &self.bounds);
        let backdrop = shade.map(|shade| shade.backdrop.clone());
        if backdrop == self.backdrop {
            return;
        }
        self.shadow_tint = shade.and_then(|shade| shade.tint).unwrap_or([0.0; 4]);
        log!("backdrop of {}: {backdrop:?}, shadow {:?}", self.name, self.shadow_tint);
        self.backdrop = backdrop;
    }

    pub fn wake(&mut self, now: Instant) {
        self.next_due = self.next_due.min(now);
    }

    pub fn resume(&mut self) {
        self.last_step = None;
        self.next_due = Instant::now();
        self.shown_pending = None;
    }

    /// A point in logical pixels on the shared desktop in overlay coordinates: up from the bottom left of the overlay.
    pub fn local(&self, x: f32, y: f32) -> V2 {
        let area = self.area();
        V2::new(x - area.x as f32, (area.bottom() as f32) - y)
    }

    pub fn release(mut self, gpu: &Gpu) {
        if let Some(surface) = self.surface.take() {
            gpu.release(surface);
        }
    }
}

fn layer_for(behind: bool) -> Layer {
    if behind { Layer::Bottom } else { Layer::Top }
}
