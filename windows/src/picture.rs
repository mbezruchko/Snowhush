//! The snow lying in one display's wallpaper: a window of its own in the desktop's wallpaper layer, over the wallpaper
//! and under the icons and the snow, drawn again only while that snow changes, a few times a second at most. The
//! wallpaper is read off the snow's thread, once per picture; what is found in it lets the flakes fall among its things.

use std::sync::mpsc;

use snowhush_core::geometry::V2;
use snowhush_core::simulation::{COLUMN_WIDTH, SnowSimulation};
use snowhush_core::snowman::room::Room;
use snowhush_core::snowman::{Fight, Ground, writing};
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
    /// Something shows on the surface.
    showing: bool,
    /// The snowball fight in the picture, while there is one; and where the overlay's simulation starts on the display.
    fight: Option<Fight>,
    offset: V2,
    /// Whether the display is seen, not covered by a full-screen window: the bot throws only then.
    pub visible: bool,
    /// What the words over the picture said when last shown.
    said: Vec<String>,
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
            fight: None,
            offset: V2::ZERO,
            visible: true,
            said: Vec::new(),
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
        self.painting = None;
        self.fight = None;
        simulation.set_keeps_awake(false);
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
    /// its depth. The `main` display shows the round at its top, and `hint`, how to throw, while it isn't empty. `now`
    /// in seconds.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        gpu: &Gpu,
        simulation: &mut SnowSimulation,
        area: [f32; 2],
        frost: bool,
        fighting: bool,
        room: &mut Room,
        main: bool,
        hint: &str,
        now: f64,
    ) {
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
        let view = [self.size.0 as f32, self.size.1 as f32];
        let place = [area[0], area[1], simulation.width(), simulation.height()];
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
            self.fight = Fight::new(ground, now, now.to_bits() ^ 0xA5A5_5A5A);
        } else if !fighting {
            self.fight = None;
        }
        simulation.set_keeps_awake(self.fight.is_some());
        let mut moved = false;
        let offset = self.offset;
        if let Some(fight) = &mut self.fight {
            fight.follow(room, main, now);
            let covers: Vec<[f32; 4]> =
                simulation.covers.iter().map(|c| [c[0] + offset.x, c[1] + offset.y, c[2] + offset.x, c[3] + offset.y]).collect();
            moved = fight.step(now, simulation, offset, &covers, self.visible, room);
        }
        let snowmen = self.fight.as_ref().map(|fight| fight.drawings(room)).unwrap_or_default();
        let words = if self.fight.is_some() { writing::shown(&snowmen, room, hint, main, view) } else { Vec::new() };
        let said: Vec<String> = words.iter().map(|shown| shown.writing.key.clone()).collect();
        let changed = said != self.said;
        self.said = said;
        let cleared = simulation.take_picture_cleared();
        let (level, veil) = (simulation.picture_level(), simulation.picture_veil());
        let snow = frost && !cleared && (level > 0.0 || veil > 0.0 || simulation.has_picture_dabs());
        let Some(painting) = &mut self.painting else { return };
        if !snow {
            painting.uncache();
            self.drawn_version = None;
            if snowmen.is_empty() && words.is_empty() {
                if self.showing {
                    let _ = self.surface.clear(gpu);
                    self.showing = false;
                }
            } else if moved || changed || !self.showing {
                self.showing = self.surface.present_picture(gpu, painting, view, &snowmen, &words).is_ok();
            }
            return;
        }
        let version = simulation.picture_version();
        if self.drawn_version != Some(version) && now - self.drawn_at >= REDRAW {
            let (dabs, specks, dabs_size, dabs_version) = simulation.picture_dabs();
            let origin = self.given.map_or([0.0; 2], |(_, origin)| origin);
            let dabs_area = [origin[0], origin[1], dabs_size.0 as f32 * cell, dabs_size.1 as f32 * cell];
            let lying = Lying { level, veil, dabs, specks, dabs_size, dabs_version, dabs_area };
            match self.surface.draw_picture(gpu, painting, &lying, view, &snowmen, &words) {
                Ok(()) => {
                    self.showing = true;
                    self.drawn_version = Some(version);
                    self.drawn_at = now;
                }
                Err(error) => log!("{}: wallpaper snow not drawn: {error}", self.name),
            }
        } else if moved || changed {
            let _ = self.surface.present_picture(gpu, painting, view, &snowmen, &words);
        }
    }

    /// A click at `point` on the display, in DIPs from its top left: over a snowman, or anywhere on the wallpaper, a
    /// snowball goes into the picture there. Returns whether it did.
    pub fn click(&mut self, point: [f32; 2], simulation: &mut SnowSimulation, now: f64) -> bool {
        let offset = self.offset;
        let height = self.size.1 as f32;
        let Some(fight) = &mut self.fight else { return false };
        let at = V2::new(point[0], height - point[1]);
        let near = fight.aim_near(at, fight.ground().near_at(at));
        fight.throw_ball(at, near, now, simulation, offset);
        true
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

    /// Whether a snowball fight is on in this display's picture.
    pub fn fighting(&self) -> bool {
        self.fight.is_some()
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
