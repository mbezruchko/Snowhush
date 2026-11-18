//! The snow lying in one display's wallpaper: a window of its own in the desktop's wallpaper layer, over the wallpaper
//! and under the icons and the snow, drawn again only while that snow changes, a few times a second at most. The
//! wallpaper is read off the snow's thread, once per picture; what is found in it lets the flakes fall among its things.

use std::sync::mpsc;

use snowhush_core::simulation::{COLUMN_WIDTH, SnowSimulation};
use snowhush_core::wallpaper::Scene;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, IsWindow, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos};
use windows::core::Result;

use crate::desktop::DesktopLayer;
use crate::log;
use crate::render::{Gpu, Lying, Painting, Surface};
use crate::screen::{Monitor, create_behind, dip_rect};
use crate::wallpaper::Wallpaper;

/// The least time between two drawings, in seconds: closer than that, the snow would cost frames for changes too small
/// to see.
const REDRAW: f64 = 0.15;

pub struct PictureSnow {
    name: String,
    window: HWND,
    surface: Surface,
    /// The display in DIPs.
    size: (u32, u32),
    /// The wallpaper the display shows, as last told.
    wallpaper: Option<Wallpaper>,
    /// The wallpaper being read, and where what is found comes back.
    reading: Option<(Wallpaper, mpsc::Receiver<Option<Scene>>)>,
    /// The wallpaper read last, whether anything was found in it or not.
    read: Option<Wallpaper>,
    scene: Option<Scene>,
    painting: Option<Painting>,
    /// The overlay's place on the display the simulation was given the picture for, left and top in DIPs and size, and
    /// where the part of the picture it was given starts.
    given: Option<([f32; 4], [f32; 2])>,
    drawn_version: Option<u64>,
    drawn_at: f64,
    /// Something of the snow shows on the surface.
    showing: bool,
}

impl PictureSnow {
    /// In the desktop's wallpaper layer, just under `over`, the overlay when it is there too.
    pub fn new(gpu: &Gpu, monitor: &Monitor, desktop: &DesktopLayer, over: Option<HWND>) -> Result<PictureSnow> {
        let window = create_behind(&monitor.bounds, desktop)?;
        if let Some(over) = over {
            unsafe {
                let _ = SetWindowPos(window, Some(over), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
        let rect = dip_rect(monitor, &monitor.bounds);
        let size = ((rect[2] - rect[0]).round().max(1.0) as u32, (rect[3] - rect[1]).round().max(1.0) as u32);
        let physical = ((monitor.bounds.right - monitor.bounds.left) as f32, (monitor.bounds.bottom - monitor.bounds.top) as f32);
        let surface = match Surface::new(gpu, window, size.0, size.1, (physical.0 / size.0 as f32, physical.1 / size.1 as f32)) {
            Ok(surface) => surface,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(window);
                }
                return Err(error);
            }
        };
        Ok(PictureSnow {
            name: monitor.name.clone(),
            window,
            surface,
            size,
            wallpaper: None,
            reading: None,
            read: None,
            scene: None,
            painting: None,
            given: None,
            drawn_version: None,
            drawn_at: f64::NEG_INFINITY,
            showing: false,
        })
    }

    /// Whether its window is still there: Explorer restarting takes the wallpaper layer and everything in it.
    pub fn alive(&self) -> bool {
        unsafe { IsWindow(Some(self.window)).as_bool() }
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
        self.painting = None;
        simulation.set_picture(None, None);
        self.clear(gpu);
    }

    fn clear(&mut self, gpu: &Gpu) {
        if self.showing {
            let _ = self.surface.clear(gpu);
        }
        self.showing = false;
        self.drawn_version = None;
    }

    /// Looks after the picture's snow once the simulation has stepped: takes in what a reading found, reads a wallpaper
    /// not read yet, gives the simulation the picture where the overlay `area` lies on the display, both in DIPs from
    /// its top left, and draws the snow if it changed. `frost` draws it at all; without, the picture is read only for
    /// its depth. `now` in seconds.
    pub fn tick(&mut self, gpu: &Gpu, simulation: &mut SnowSimulation, area: [f32; 2], frost: bool, now: f64) {
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
        {
            let (sender, found) = mpsc::channel();
            let (size, reading, name) = (self.size, wallpaper.clone(), self.name.clone());
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
        let place = [area[0], area[1], simulation.width(), simulation.height()];
        if self.given.map(|(given, _)| given) != Some(place) {
            let (terrain, light, origin) = scene.cropped(place);
            simulation.set_picture(Some(&terrain), Some(&light));
            self.given = Some((place, origin));
        }
        if simulation.take_picture_cleared() {
            self.clear(gpu);
        }
        let (level, veil) = (simulation.picture_level(), simulation.picture_veil());
        if !frost || (level <= 0.0 && veil <= 0.0 && !simulation.has_picture_dabs()) {
            self.clear(gpu);
            return;
        }
        let version = simulation.picture_version();
        if self.drawn_version == Some(version) || now - self.drawn_at < REDRAW {
            return;
        }
        let Some(painting) = &mut self.painting else { return };
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
        match self.surface.draw_picture(gpu, painting, &lying, [self.size.0 as f32, self.size.1 as f32]) {
            Ok(()) => {
                self.showing = true;
                self.drawn_version = Some(version);
                self.drawn_at = now;
            }
            Err(error) => log!("{}: wallpaper snow not drawn: {error}", self.name),
        }
    }

    fn take(&mut self, scene: Option<Scene>, gpu: &Gpu, simulation: &mut SnowSimulation) {
        self.forget(gpu, simulation);
        let Some(scene) = scene else { return };
        log!("{}: wallpaper read, {} ridges", self.name, scene.ridges.len());
        match gpu.paint(&scene, COLUMN_WIDTH) {
            Ok(painting) => self.painting = Some(painting),
            Err(error) => log!("{}: wallpaper not taken in: {error}", self.name),
        }
        self.scene = Some(scene);
    }
}

impl Drop for PictureSnow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.window);
        }
    }
}
