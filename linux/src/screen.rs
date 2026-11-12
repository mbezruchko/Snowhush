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

/// How long a frame may go unshown before the display counts as off or hidden; the snow then waits for it.
const STALLED: Duration = Duration::from_secs(1);
/// How often a screen with nothing on it and nothing coming looks again, besides being woken.
const RESTING: Duration = Duration::from_secs(1);
/// Seconds the snow takes to fade out of a full-screen app's way, and back in once it goes.
const FADE_OUT: f32 = 0.25;
const FADE_IN: f32 = 0.35;
/// Frames that fail to draw in a row before the drawing surface is made again.
const RENEW_AFTER: u32 = 3;
/// How long a display waits after a frame failed to draw before trying again.
const RETRY: Duration = Duration::from_millis(250);
/// How often a run of failing frames is written to the log after its first.
const FAILURE_LOG: Duration = Duration::from_secs(60);

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
    /// Whether snow settles on the shelf.
    pub panel: bool,
    /// Placed once already: a change of margins after that moves the ground.
    placed: bool,
    /// A full-screen app the snow shows over has put the panels away: the snow takes the whole display.
    full_screen: bool,
    /// Whether a full-screen app was last seen in front, and since when.
    full_screen_seen: (bool, Instant),
    /// The panel's shelf a full-screen app put away, which takes up the drift under it when it comes back.
    shelf_put_away: bool,
    /// How far the bottom of the overlay was last moved down, in logical pixels, up if negative: the size the
    /// compositor settles on next comes with the ground moving that far.
    ground_moved: Option<f32>,
    size: (u32, u32),
    last_step: Option<Instant>,
    idle_frames: u32,
    interactive_rate: u32,
    /// Every frame at `interactive_rate`, whatever the snow is doing and whatever the power.
    pub max_rate: bool,
    surfaces: Vec<WindowSurface>,
    windows_seen: u64,
    windows_changed_at: Instant,
    covered: bool,
    /// How much of the snow shows, from 0 to 1.
    opacity: f32,
    /// When the frame now waiting for the compositor was drawn.
    shown_pending: Option<Instant>,
    /// Frames went unshown for a while, so the snow is waiting.
    stalled: bool,
    backdrop: Option<Backdrop>,
    shadow_tint: [f32; 4],
    /// Where the cursor's candle was last drawn, in overlay coordinates.
    lantern: V2,
    /// Whether the last frame drawn showed Halloween's pumpkins.
    pumpkins_shown: bool,
    /// Frames that failed to draw in a row, and when that was last written to the log.
    failures: u32,
    failure_logged: Option<Instant>,
    /// The drawing surface was let go, to be made again at the next frame.
    renew: bool,
}

impl Screen {
    /// `layer` is where the snow goes among the windows: see `layer_for`.
    pub fn new(
        app: (&CompositorState, &LayerShell, &QueueHandle<App>),
        output: WlOutput,
        name: String,
        bounds: Area,
        refresh: u32,
        simulation: SnowSimulation,
        layer: Layer,
    ) -> Screen {
        let (compositor, layer_shell, qh) = app;
        let surface = compositor.create_surface(qh);
        // An empty input region: every click and scroll goes through to what is under the snow.
        if let Ok(region) = Region::new(compositor) {
            surface.set_input_region(Some(region.wl_region()));
        }
        let layer = layer_shell.create_layer_surface(qh, surface, layer, Some("snowhush"), Some(&output));
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
            panel: true,
            placed: false,
            full_screen: false,
            full_screen_seen: (false, Instant::now()),
            shelf_put_away: false,
            ground_moved: None,
            size: (bounds.w as u32, bounds.h as u32),
            last_step: None,
            idle_frames: 0,
            interactive_rate: refresh.clamp(60, 120),
            max_rate: false,
            surfaces: Vec::new(),
            windows_seen: 0,
            windows_changed_at: Instant::now(),
            covered: false,
            opacity: 1.0,
            shown_pending: None,
            stalled: false,
            backdrop: None,
            shadow_tint: [0.0; 4],
            lantern: V2::new(-1e6, -1e6),
            pumpkins_shown: false,
            failures: 0,
            failure_logged: None,
            renew: false,
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
                Ok(surface) => {
                    self.surface = Some(surface);
                    self.renew = false;
                }
                Err(error) => log!("{}: no surface: {error}", self.name),
            },
        }
        if (width, height) != self.size {
            log!("{}: {width}x{height}", self.name);
        }
        self.size = (width, height);
        // Reaching further down, as over a panel a full-screen app has put away, the drift falls from where it lay onto
        // the new bottom; stopping short again, it comes up onto the panel. Everything else stays where it is.
        match self.ground_moved.take() {
            Some(by) if by > 0.0 => self.simulation.lower_ground(width as f32, height as f32, by),
            Some(by) if by < 0.0 => self.simulation.raise_ground(width as f32, height as f32, -by),
            _ => self.simulation.resize(width as f32, height as f32),
        }
        self.resume();
    }

    /// Follows a full-screen app the snow shows over putting the panels away, or bringing them back, once the change has
    /// lasted, as the app goes full screen or comes out of it in a few steps; returns whether to place the overlay again.
    pub fn follow_full_screen(&mut self, in_front: bool, now: Instant) -> bool {
        if self.full_screen_seen.0 != in_front {
            self.full_screen_seen = (in_front, now);
        }
        if in_front == self.full_screen || now - self.full_screen_seen.1 < Duration::from_millis(300) {
            return false;
        }
        log!("{}: {}", self.name, if in_front { "panels put away by a full-screen app" } else { "panels back" });
        self.full_screen = in_front;
        true
    }

    /// Back to the work area, as when the snow no longer shows over full-screen apps; returns whether it took more.
    pub fn leave_full_screen(&mut self) -> bool {
        std::mem::take(&mut self.full_screen)
    }

    /// Fits the snow to the display's work area, so the ground lies on top of the panel; with the bottom
    /// panel as a shelf, the snow reaches down to the display's edge instead, around and behind the panel.
    pub fn place(&mut self, bounds: Area, work: Area, shelf: Option<Shelf>) {
        self.bounds = bounds;
        // Under a full-screen app the snow shows over, the panels are out of sight: no work area left out, and no
        // panel for the snow to lie on, whose snow falls to the bottom of the display as off a window closing.
        let work = if self.full_screen { bounds } else { work };
        let shelf = if self.full_screen { None } else { shelf };
        if shelf != self.shelf {
            log!("{}: panel {shelf:?}", self.name);
            // Given back, the panel comes up over the drift that fell from it and takes it up again.
            if self.full_screen && self.shelf.is_some() {
                self.shelf_put_away = true;
            } else if let Some(back) = shelf
                && std::mem::take(&mut self.shelf_put_away)
            {
                self.simulation.lift_onto(back.id);
            }
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
        if self.placed && margins.0 == self.margins.0 && margins.2 != self.margins.2 {
            self.ground_moved = Some((self.margins.2 - margins.2) as f32);
        }
        self.placed = true;
        self.margins = margins;
        self.layer.set_margin(margins.0, margins.1, margins.2, margins.3);
        self.layer.commit();
    }

    pub fn set_layer(&mut self, behind: bool, over_full_screen: bool) {
        self.layer.set_layer(layer_for(behind, over_full_screen));
        self.layer.commit();
        self.resume();
    }

    /// The overlay's place on the shared desktop: where the margins put it, as big as the compositor made it.
    pub fn area(&self) -> Area {
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
        // The display is off or the overlay hidden, as behind a lock screen: the snow waits there as it does while
        // locked, and a frame now and then asks to be shown again.
        if self.shown_pending.is_some_and(|pending| now - pending >= STALLED) {
            self.stalled = true;
            self.last_step = None;
            self.next_due = now + STALLED;
            let pumpkins = !self.simulation.pumpkins().is_empty();
            let lantern = self.lantern;
            return self.present(gpu, qh, now, ambient, lantern, time, pumpkins);
        }
        let dt = self.last_step.map_or(1.0 / 60.0, |last| (now - last).as_secs_f32().min(0.05));
        self.last_step = Some(now);
        if let Some((report, generation, moving)) = report {
            self.follow(report, generation, moving, snow_on_windows, now);
            if let Some(slide) = report.slide {
                self.simulation.space_slid(slide.velocity(now));
            }
        }
        let changed = self.simulation.step(dt, self.pointer(cursor));
        // Nothing to draw and nothing on its way: the frame shown last stays up, as it would, till the snow is stirred.
        if !changed && !self.pumpkins_shown && self.simulation.is_empty() {
            self.fade(gpu, dt, true);
            self.last_step = None;
            self.next_due = now + RESTING;
            return Ok(false);
        }
        // The candle follows the cursor even over snow that lies still.
        let lantern = self.local(cursor.0, cursor.1);
        let lantern_moved = ambient.night > 0.0 && (lantern - self.lantern).length() > 0.5 && (self.lit_by(lantern) || self.lit_by(self.lantern));
        self.lantern = lantern;
        let fading = self.fade(gpu, dt, false);
        if self.hidden() {
            return Ok(false);
        }

        let fastest = if budget == PowerBudget::Full { self.interactive_rate } else { self.interactive_rate.min(60) };
        // 24 only where it divides the display's rate evenly; elsewhere it would judder.
        let calm = if budget == PowerBudget::Saver && self.interactive_rate % 24 == 0 { 24 } else { 30 };
        let rate = match self.simulation.pace() {
            Pace::Calm => calm,
            Pace::Lively => fastest.min(60),
            Pace::Interactive => fastest,
        };
        let rate = if self.max_rate {
            self.interactive_rate
        } else if lantern_moved || fading {
            rate.max(fastest.min(60))
        } else {
            rate
        };
        self.next_due = now + Duration::from_secs_f64(1.0 / rate as f64);
        self.idle_frames = if changed || lantern_moved { 0 } else { self.idle_frames + 1 };
        // A still pile only needs a slow refresh for its glints, and the pumpkins for their candles' flicker; once
        // they are gone, until a frame without them is shown.
        let pumpkins = !self.simulation.pumpkins().is_empty();
        let slow_refresh = self.simulation.has_snow() || pumpkins || self.pumpkins_shown;
        if !(changed || lantern_moved || fading || (slow_refresh && self.idle_frames % 2 == 0)) {
            return Ok(false);
        }
        // The compositor hasn't shown the last frame yet: drawing another would only be thrown away.
        if self.shown_pending.is_some() {
            return Ok(false);
        }
        self.present(gpu, qh, now, ambient, lantern, time, pumpkins)
    }

    /// Draws the snow as it is and asks the compositor to say when it shows it.
    #[allow(clippy::too_many_arguments)]
    fn present(
        &mut self,
        gpu: &mut Gpu,
        qh: &QueueHandle<App>,
        now: Instant,
        ambient: Ambient,
        lantern: V2,
        time: f32,
        pumpkins: bool,
    ) -> Result<bool, String> {
        if self.surface.is_none() && !self.renew {
            return Ok(false);
        }
        let wl_surface = self.layer.wl_surface();
        wl_surface.frame(qh, FrameCallbackData(wl_surface.clone()));
        self.shown_pending = Some(now);
        let panel = self.shelf.map(|shelf| {
            // Below the rounded corners only, so snow curling round them stays in front.
            let r = windows::local_rect(&shelf.rect, &self.area());
            [r.min_x(), r.min_y(), r.max_x(), r.max_y() - shelf.radius]
        });
        if let Err(error) = self.draw(gpu, Ambient { shadow_tint: self.shadow_tint, lantern, ..ambient }, panel, time) {
            self.failures += 1;
            if self.failure_logged.is_none_or(|at| now - at >= FAILURE_LOG) {
                self.failure_logged = Some(now);
                log!("{}: frame failed, {} in a row: {error}", self.name, self.failures);
            }
            if self.failures.is_multiple_of(RENEW_AFTER) {
                self.drop_surface(gpu);
            }
            // Nothing was committed, so the compositor will not say it showed the frame: that is no sign of the
            // display being off.
            self.shown_pending = None;
            self.next_due = self.next_due.max(now + RETRY);
            return Err(error);
        }
        if self.failures > 0 {
            log!("{}: drawing again after {} failed frames", self.name, self.failures);
            self.failures = 0;
            self.failure_logged = None;
        }
        self.pumpkins_shown = pumpkins;
        Ok(true)
    }

    /// Draws on the surface, made again first if it was let go.
    fn draw(&mut self, gpu: &mut Gpu, ambient: Ambient, panel: Option<[f32; 4]>, time: f32) -> Result<(), String> {
        let surface = match self.surface.take() {
            Some(surface) => surface,
            None => gpu.surface(self.layer.wl_surface(), self.size.0 as i32, self.size.1 as i32)?,
        };
        self.renew = false;
        let drawn = gpu.draw(&surface, &self.simulation, ambient, panel, time, self.opacity);
        self.surface = Some(surface);
        drawn
    }

    /// Lets the drawing surface go, to be made again at the next frame, as when drawing on it keeps failing.
    pub fn drop_surface(&mut self, gpu: &Gpu) {
        if let Some(surface) = self.surface.take() {
            gpu.release(surface);
            self.renew = true;
        }
    }

    /// Whether a candle at this point, in overlay coordinates, lights anything on this display.
    fn lit_by(&self, at: V2) -> bool {
        let reach = LANTERN_REACH * 3.0;
        at.x > -reach && at.y > -reach && at.x < self.simulation.width() + reach && at.y < self.simulation.height() + reach
    }

    /// The compositor showed the last frame.
    pub fn shown(&mut self) {
        self.shown_pending = None;
        // Shown again after a while away: the snow goes on at once from where it waited.
        if std::mem::take(&mut self.stalled) {
            self.next_due = Instant::now();
        }
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
        if !report.screens.is_empty() {
            self.simulation.forget_closed(|id| windows::exists(report, id));
        }
        let area = self.area();
        let listed = if snow_on_windows { report } else { &Report::default() };
        let mut surfaces = windows::surfaces(listed, &area, self.shelf.as_ref().filter(|_| self.panel), &self.name);
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
        let surfaces = windows::surfaces(&Report::default(), &self.area(), self.shelf.as_ref().filter(|_| self.panel), &self.name);
        self.simulation.update_windows(&surfaces, 0.1, &HashSet::new(), &HashSet::new(), false);
        self.simulation.covers.clear();
        self.surfaces = surfaces;
        self.windows_seen = 0;
    }

    /// Reads the windows again at the next frame, as when snow on windows comes back on.
    pub fn rewatch(&mut self) {
        self.windows_seen = 0;
    }

    /// Fades the snow out while a full-screen app covers the display, still falling, then clears and stops it, as over
    /// a full-screen app on a Mac; once the app goes, the snow fades back in from where it stopped.
    pub fn set_covered(&mut self, covered: bool) {
        if covered == self.covered {
            return;
        }
        self.covered = covered;
        self.resume();
    }

    pub fn covered(&self) -> bool {
        self.covered
    }

    /// Frames kept failing though the drawing surface was made again: the graphics themselves may be lost.
    pub fn failing(&self) -> bool {
        self.failures >= 2 * RENEW_AFTER
    }

    /// Frames went unshown for a while, as with the display off: the snow waits.
    pub fn stalled(&self) -> bool {
        self.stalled
    }

    /// Faded all the way out under a full-screen app: nothing to draw until it goes.
    pub fn hidden(&self) -> bool {
        self.covered && self.opacity <= 0.0
    }

    /// Takes the fade a step toward shown, or gone under a full-screen app; true while it lasts.
    fn fade(&mut self, gpu: &Gpu, dt: f32, at_once: bool) -> bool {
        let target = if self.covered { 0.0 } else { 1.0 };
        if self.opacity == target {
            return false;
        }
        let step = if at_once { 1.0 } else { dt / if self.covered { FADE_OUT } else { FADE_IN } };
        self.opacity = if target > self.opacity { (self.opacity + step).min(1.0) } else { (self.opacity - step).max(0.0) };
        if self.hidden()
            && let Some(surface) = &self.surface
        {
            let _ = gpu.clear(surface);
        }
        true
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

    /// Wakes the display only if it rests, so one already drawing keeps its pace.
    pub fn stir(&mut self, now: Instant) {
        if self.last_step.is_none() && !self.stalled {
            self.wake(now);
        }
    }

    pub fn resume(&mut self) {
        self.last_step = None;
        self.next_due = Instant::now();
        self.shown_pending = None;
        self.stalled = false;
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

/// `behind` puts the snow under the windows, just over the desktop; without it the snow is above them, though under
/// menus, notifications and full-screen windows, unless `over_full_screen` puts it over all of them.
pub fn layer_for(behind: bool, over_full_screen: bool) -> Layer {
    if behind {
        Layer::Bottom
    } else if over_full_screen {
        Layer::Overlay
    } else {
        Layer::Top
    }
}
