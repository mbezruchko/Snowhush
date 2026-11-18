//! The snow lying in one display's wallpaper: a layer of its own just over the wallpaper and under every window, drawn
//! again only while that snow changes, a few times a second at most. The wallpaper is read off the snow's thread, once
//! per picture; what is found in it lets the flakes fall among its things too, and snowmen walk its ground for a
//! snowball fight, drawn on the same layer, which then takes clicks on them alone.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use smithay_client_toolkit::compositor::{CompositorState, Region};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface};
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_surface::WlSurface;

use snowhush_core::geometry::V2;
use snowhush_core::simulation::{COLUMN_WIDTH, SnowSimulation};
use snowhush_core::snowman::room::Room;
use snowhush_core::snowman::{Drawing, Fight, Ground};
use snowhush_core::wallpaper::Scene;

use crate::log;
use crate::render::{Gpu, Lying, Painting, Surface};
use crate::snow::App;
use crate::wallpaper::Wallpaper;
use crate::windows::Area;

/// The least time between two drawings of the snow: drawn closer than that, it would cost frames for changes too small
/// to see.
const REDRAW: Duration = Duration::from_millis(150);

pub struct PictureSnow {
    name: String,
    layer: LayerSurface,
    surface: Option<Surface>,
    size: (u32, u32),
    /// The wallpaper the display shows, as last told.
    wallpaper: Option<Wallpaper>,
    /// The wallpaper being read, and where what is found comes back.
    reading: Option<(Wallpaper, mpsc::Receiver<Option<Scene>>)>,
    /// The wallpaper read last, whether anything was found in it or not.
    read: Option<Wallpaper>,
    scene: Option<Scene>,
    painting: Option<Painting>,
    /// The overlay's place on the display the simulation was given the picture for, left and top in points and size,
    /// and where the part of the picture it was given starts.
    given: Option<([f32; 4], [f32; 2])>,
    drawn_version: Option<u64>,
    drawn_at: Instant,
    /// Something shows on the surface.
    showing: bool,
    /// The snowball fight in the picture, while there is one; and where the overlay's simulation starts on the display.
    fight: Option<Fight>,
    offset: V2,
    /// Whether the display is seen, not covered by a full-screen window: the bot throws only then.
    pub visible: bool,
    started: Instant,
    /// When the last practice throw went, with SNOWHUSH_FIGHT_PRACTICE=1: one at the first snowman every two seconds, for
    /// trying the fight out where nothing can click.
    practiced: Instant,
    /// Where clicks are taken: over the snowmen, left, top, width and height in the surface's points.
    taking: Vec<[i32; 4]>,
}

impl PictureSnow {
    pub fn new(app: (&CompositorState, &LayerShell, &QueueHandle<App>), output: &WlOutput, name: &str) -> PictureSnow {
        let (compositor, layer_shell, qh) = app;
        let surface = compositor.create_surface(qh);
        if let Ok(region) = Region::new(compositor) {
            surface.set_input_region(Some(region.wl_region()));
        }
        // Over the wallpaper, which a wallpaper daemon or the desktop puts on this layer before us, and under everything.
        let layer = layer_shell.create_layer_surface(qh, surface, Layer::Background, Some("snowhush-wallpaper"), Some(output));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        PictureSnow {
            name: name.to_string(),
            layer,
            surface: None,
            size: (0, 0),
            wallpaper: None,
            reading: None,
            read: None,
            scene: None,
            painting: None,
            given: None,
            drawn_version: None,
            drawn_at: Instant::now(),
            showing: false,
            fight: None,
            offset: V2::ZERO,
            visible: true,
            started: Instant::now(),
            taking: Vec::new(),
            practiced: Instant::now(),
        }
    }

    pub fn is(&self, layer: &LayerSurface) -> bool {
        self.layer.wl_surface() == layer.wl_surface()
    }

    pub fn owns(&self, surface: &WlSurface) -> bool {
        self.layer.wl_surface() == surface
    }

    pub fn configure(&mut self, gpu: &Gpu, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        match &mut self.surface {
            Some(surface) => gpu.resize(surface, width as i32, height as i32),
            None => match gpu.surface(self.layer.wl_surface(), width as i32, height as i32) {
                Ok(surface) => self.surface = Some(surface),
                Err(error) => log!("{}: no wallpaper surface: {error}", self.name),
            },
        }
        if (width, height) != self.size {
            self.size = (width, height);
            self.drawn_version = None;
            // Read for another size, the snow in the picture lies elsewhere.
            if self.read.is_some() {
                self.read = None;
            }
        }
        if let Some(surface) = &self.surface {
            let _ = gpu.clear(surface);
            self.showing = false;
        }
    }

    /// The display shows `wallpaper` now: one it didn't before is read, and the old one's snow goes at once.
    pub fn show(&mut self, wallpaper: Option<Wallpaper>, gpu: &Gpu, simulation: &mut SnowSimulation) {
        if wallpaper == self.wallpaper {
            return;
        }
        log!("{}: wallpaper {:?}", self.name, wallpaper.as_ref().map(|wallpaper| &wallpaper.path));
        let other = matches!((&self.wallpaper, &wallpaper), (Some(old), Some(new)) if old.path != new.path);
        self.wallpaper = wallpaper;
        self.forget(gpu, simulation);
        // Another picture starts over, and snow forms on it from the first flake.
        if other {
            simulation.restore_picture(0.0);
        }
    }

    fn forget(&mut self, gpu: &Gpu, simulation: &mut SnowSimulation) {
        self.read = None;
        self.scene = None;
        self.given = None;
        self.fight = None;
        simulation.set_keeps_awake(false);
        if let Some(painting) = self.painting.take() {
            gpu.unpaint(painting);
        }
        simulation.set_picture(None, None);
        self.clear(gpu);
    }

    fn clear(&mut self, gpu: &Gpu) {
        if self.showing
            && let Some(surface) = &self.surface
        {
            let _ = gpu.clear(surface);
        }
        self.showing = false;
        self.drawn_version = None;
    }

    /// Looks after the picture's snow once the simulation has stepped: takes in what a reading found, reads a wallpaper
    /// not read yet, gives the simulation the picture where the overlay `area` lies on the display `bounds`, moves the
    /// snowmen on while `fighting`, and draws what changed. `frost` draws the snow at all; without, the picture is read
    /// only for its depth.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        gpu: &mut Gpu,
        compositor: &CompositorState,
        simulation: &mut SnowSimulation,
        bounds: &Area,
        area: &Area,
        frost: bool,
        fighting: bool,
        room: &mut Room,
        main: bool,
        now: Instant,
    ) {
        self.take_reading(gpu, simulation, bounds);
        let Some(scene) = &self.scene else { return };
        let cell = scene.terrain.cell as f32;
        let view = [bounds.w as f32, bounds.h as f32];
        let place = [(area.x - bounds.x) as f32, (area.y - bounds.y) as f32, simulation.width(), simulation.height()];
        if self.given.map(|(given, _)| given) != Some(place) {
            let (terrain, light, origin) = scene.cropped(place);
            simulation.set_picture(Some(&terrain), Some(&light));
            self.given = Some((place, origin));
        }
        self.offset = V2::new(place[0], view[1] - place[1] - place[3]);
        let fighting = fighting && simulation.picture_depth;
        if fighting && self.fight.is_none() {
            let screen = V2::new(view[0], view[1]);
            let ground = Ground::new(scene, screen).unwrap_or_else(|| Ground::strip(scene, screen, self.offset.y));
            log!("{}: a snowball fight on {}", self.name, if ground.in_front { "the strip along the bottom" } else { "the picture's ground" });
            let seed = now.duration_since(self.started).as_nanos() as u64 ^ 0xA5A5_5A5A;
            self.fight = Fight::new(ground, self.seconds(now), seed);
        } else if !fighting {
            self.fight = None;
        }
        simulation.set_keeps_awake(self.fight.is_some());
        let mut moved = false;
        let seconds = self.seconds(now);
        let offset = self.offset;
        if let Some(fight) = &mut self.fight {
            fight.follow(room, main, seconds);
            let covers: Vec<[f32; 4]> =
                simulation.covers.iter().map(|c| [c[0] + offset.x, c[1] + offset.y, c[2] + offset.x, c[3] + offset.y]).collect();
            moved = fight.step(seconds, simulation, offset, &covers, self.visible, room);
        }
        let snowmen = self.fight.as_ref().map(|fight| fight.drawings(room)).unwrap_or_default();
        let cleared = simulation.take_picture_cleared();
        let (level, veil) = (simulation.picture_level(), simulation.picture_veil());
        let snow = frost && !cleared && (level > 0.0 || veil > 0.0 || simulation.has_picture_dabs());
        let (Some(surface), Some(painting)) = (&self.surface, &mut self.painting) else { return };
        if !snow {
            gpu.uncache(painting);
            self.drawn_version = None;
            if snowmen.is_empty() {
                if self.showing {
                    let _ = gpu.clear(surface);
                    self.showing = false;
                }
            } else if moved || !self.showing {
                self.showing = gpu.present_picture(surface, painting, view, &snowmen).is_ok();
            }
        } else {
            let version = simulation.picture_version();
            if self.drawn_version != Some(version) && now - self.drawn_at >= REDRAW {
                let (dabs, specks, dabs_size, dabs_version) = simulation.picture_dabs();
                let origin = self.given.map_or([0.0; 2], |(_, origin)| origin);
                let dabs_area = [origin[0], origin[1], dabs_size.0 as f32 * cell, dabs_size.1 as f32 * cell];
                let lying = Lying { level, veil, dabs, specks, dabs_size, dabs_version, dabs_area };
                match gpu.draw_picture(surface, painting, &lying, view, &snowmen) {
                    Ok(()) => {
                        self.showing = true;
                        self.drawn_version = Some(version);
                        self.drawn_at = now;
                    }
                    Err(error) => log!("{}: wallpaper snow not drawn: {error}", self.name),
                }
            } else if moved {
                let _ = gpu.present_picture(surface, painting, view, &snowmen);
            }
        }
        self.take_clicks_over(&snowmen, view[1], compositor);
        if let Some(first) = snowmen.first()
            && now - self.practiced > Duration::from_secs(2)
            && std::env::var("SNOWHUSH_FIGHT_PRACTICE").is_ok_and(|value| value == "1")
        {
            self.practiced = now;
            let [left, bottom, right, top] = first.reach;
            let middle = ((left + right) / 2.0, view[1] - (bottom + (top - bottom) * 0.47));
            self.click((middle.0 as f64, middle.1 as f64), simulation, now);
        }
    }

    fn seconds(&self, now: Instant) -> f64 {
        now.duration_since(self.started).as_secs_f64()
    }

    fn take_reading(&mut self, gpu: &mut Gpu, simulation: &mut SnowSimulation, bounds: &Area) {
        if let Some((wallpaper, found)) = &self.reading {
            match found.try_recv() {
                Ok(scene) => {
                    let wallpaper = wallpaper.clone();
                    self.reading = None;
                    if Some(&wallpaper) == self.wallpaper.as_ref() {
                        self.take(scene, gpu, simulation);
                        self.read = Some(wallpaper);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.reading.is_none()
            && self.read.as_ref() != self.wallpaper.as_ref()
            && let Some(wallpaper) = self.wallpaper.clone()
            && self.size.0 > 0
        {
            let (sender, found) = mpsc::channel();
            let size = (bounds.w.round() as u32, bounds.h.round() as u32);
            let reading = wallpaper.clone();
            let name = self.name.clone();
            let started = std::thread::Builder::new().name("wallpaper".into()).spawn(move || {
                let scene = reading.read_apart(size, COLUMN_WIDTH);
                if scene.is_none() {
                    log!("{name}: nothing read in {}", reading.path.display());
                }
                let _ = sender.send(scene);
            });
            if started.is_ok() {
                self.reading = Some((wallpaper, found));
            }
        }
    }

    /// Takes clicks over the snowmen alone, a display `height` points tall: everywhere else they go on to the desktop.
    fn take_clicks_over(&mut self, snowmen: &[Drawing], height: f32, compositor: &CompositorState) {
        let taking: Vec<[i32; 4]> = snowmen
            .iter()
            .map(|figure| {
                let [left, bottom, right, top] = figure.reach;
                [left.floor() as i32, (height - top).floor() as i32, (right - left).ceil() as i32, (top - bottom).ceil() as i32]
            })
            .collect();
        if taking == self.taking {
            return;
        }
        let Ok(region) = Region::new(compositor) else { return };
        for [x, y, width, height] in &taking {
            region.add(*x, *y, *width, *height);
        }
        let surface = self.layer.wl_surface();
        surface.set_input_region(Some(region.wl_region()));
        surface.commit();
        self.taking = taking;
    }

    /// Someone in the room hit someone, `me` being this player: the snowmen show it.
    pub fn thrown(&mut self, from: &str, at: &str, me: &str, simulation: &mut SnowSimulation) {
        if let Some(fight) = &mut self.fight {
            fight.thrown(from, at, me, simulation);
        }
    }

    /// The friends this player hit since this was last asked.
    pub fn take_hits(&mut self) -> Vec<String> {
        self.fight.as_mut().map(Fight::take_hits).unwrap_or_default()
    }

    /// A click on the layer at `point`, in its points from its top left: a snowball goes into the picture there.
    pub fn click(&mut self, point: (f64, f64), simulation: &mut SnowSimulation, now: Instant) {
        let seconds = self.seconds(now);
        let height = self.size.1 as f32;
        let offset = self.offset;
        let Some(fight) = &mut self.fight else { return };
        let at = V2::new(point.0 as f32, height - point.1 as f32);
        let near = fight.aim_near(at, fight.ground().near_at(at));
        log!("{}: snowball thrown at {:.0}, {:.0}", self.name, at.x, at.y);
        fight.throw_ball(at, near, seconds, simulation, offset);
    }

    fn take(&mut self, scene: Option<Scene>, gpu: &mut Gpu, simulation: &mut SnowSimulation) {
        self.forget(gpu, simulation);
        let Some(scene) = scene else { return };
        log!("{}: wallpaper read, {} ridges", self.name, scene.ridges.len());
        match gpu.paint(&scene, COLUMN_WIDTH) {
            Ok(painting) => self.painting = Some(painting),
            Err(error) => log!("{}: wallpaper not taken in: {error}", self.name),
        }
        self.scene = Some(scene);
    }

    pub fn release(mut self, gpu: &Gpu) {
        if let Some(painting) = self.painting.take() {
            gpu.unpaint(painting);
        }
        if let Some(surface) = self.surface.take() {
            gpu.release(surface);
        }
    }
}
