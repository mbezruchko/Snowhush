//! The snow lying in one display's wallpaper: a layer of its own just over the wallpaper and under every window, drawn
//! again only while that snow changes, a few times a second at most. The wallpaper is read off the snow's thread, once
//! per picture; what is found in it lets the flakes fall among its things too.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use smithay_client_toolkit::compositor::{CompositorState, Region};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface};
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_output::WlOutput;

use snowhush_core::simulation::{COLUMN_WIDTH, SnowSimulation};
use snowhush_core::wallpaper::Scene;

use crate::log;
use crate::render::{Gpu, Lying, Painting, Surface};
use crate::snow::App;
use crate::wallpaper::Wallpaper;
use crate::windows::Area;

/// The least time between two drawings: drawn closer than that, the snow would cost frames for changes too small to see.
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
    /// Something of the snow shows on the surface.
    showing: bool,
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
        }
    }

    pub fn is(&self, layer: &LayerSurface) -> bool {
        self.layer.wl_surface() == layer.wl_surface()
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
        let other = self.wallpaper.as_ref().map(|old| &old.path) != wallpaper.as_ref().map(|new| &new.path);
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
    /// not read yet, gives the simulation the picture where the overlay `area` lies on the display `bounds`, and draws
    /// the snow if it changed. `frost` draws it at all; without, the picture is read only for its depth.
    pub fn tick(&mut self, gpu: &mut Gpu, simulation: &mut SnowSimulation, bounds: &Area, area: &Area, frost: bool, now: Instant) {
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
        let Some(scene) = &self.scene else { return };
        let cell = scene.terrain.cell as f32;
        let place = [(area.x - bounds.x) as f32, (area.y - bounds.y) as f32, simulation.width(), simulation.height()];
        if self.given.map(|(given, _)| given) != Some(place) {
            let (terrain, light, origin) = scene.cropped(place);
            simulation.set_picture(Some(&terrain), Some(&light));
            self.given = Some((place, origin));
        }
        if simulation.take_picture_cleared() {
            self.clear(gpu);
        }
        if !frost {
            self.clear(gpu);
            return;
        }
        let (level, veil) = (simulation.picture_level(), simulation.picture_veil());
        if level <= 0.0 && veil <= 0.0 && !simulation.has_picture_dabs() {
            self.clear(gpu);
            return;
        }
        let version = simulation.picture_version();
        if self.drawn_version == Some(version) || now - self.drawn_at < REDRAW {
            return;
        }
        let (Some(surface), Some(painting)) = (&self.surface, &mut self.painting) else { return };
        let (dabs, specks, dabs_size, dabs_version) = simulation.picture_dabs();
        let origin = self.given.map_or([0.0; 2], |(_, origin)| origin);
        let lying = Lying {
            level,
            veil,
            dabs,
            specks,
            dabs_size,
            dabs_version,
            dabs_area: [origin[0], origin[1], dabs_size.0 as f32 * cell, dabs_size.1 as f32 * cell],
        };
        match gpu.draw_picture(surface, painting, &lying, [bounds.w as f32, bounds.h as f32]) {
            Ok(()) => {
                self.showing = true;
                self.drawn_version = Some(version);
                self.drawn_at = now;
            }
            Err(error) => log!("{}: wallpaper snow not drawn: {error}", self.name),
        }
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
