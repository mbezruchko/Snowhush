//! Snow in the wallpaper's picture: flakes far enough off fall among the things in it, hidden behind whatever is nearer,
//! and where one comes down on a surface as near as it is, it leaves a dab of snow and a speck of frost. Meanwhile snow
//! comes to lie over all the picture can hold, as it goes on falling, and melts once it stops. The same as the Mac's.

use super::{Flake, MAX_MOTES, Mode, Mote, SnowSimulation, flakes_per_megapoint};
use crate::geometry::V2;
use crate::wallpaper::{GROUND, Light, Terrain};

/// A flake this deep is as near as the nearest thing in the picture; deeper ones fall in front of all of it, onto the
/// windows and the drift. Depth runs on without a break: there is no line between the two kinds.
const SCENE_DEPTH: f32 = 0.7;
/// How far apart in nearness a flake and a surface can be for the flake to land on it rather than pass it.
const SCENE_TOLERANCE: f32 = 0.025;
/// What stands in front of the wallpaper is marked a cell to every this many points.
const FRONT_CELL: usize = 16;
/// How much snow lies in the picture at the most: up to 1 it comes to cover what holds it, from there to this, far more
/// slowly and only while nothing melts it, it lies deeper and buries more.
pub const DEEPEST: f32 = 2.0;

pub(super) struct Picture {
    /// How near each spot of the wallpaper is, a cell to every `cell` points from the screen's top left, 0 to 255.
    near: Vec<u8>,
    width: usize,
    height: usize,
    cell: usize,
    far: f32,
    close: f32,
    /// What the flakes seen landing have left, 0 to 1 a cell, and the frost each leaves right where it came down;
    /// as drawn, 0 to 255, written out now and then.
    dab_amount: Vec<f32>,
    speck_amount: Vec<f32>,
    pub dabs: Vec<u8>,
    pub specks: Vec<u8>,
    /// `speck_amount` as drawn, coming in over most of a second rather than at once.
    speck_shown: Vec<f32>,
    specks_easing: bool,
    dabs_touched: bool,
    pub has_dabs: bool,
    pub level: f32,
    pub veil: f32,
    level_written: f32,
    veil_written: f32,
    /// Counts up whenever the snow in the picture changes: what has fallen everywhere, the veil, or the dabs.
    pub version: u64,
    pub dabs_version: u64,
    pub blown_at: f64,
    ticked_at: f64,
    written_at: f64,
    /// How near a flake of each depth is in the picture, and how high on the screen anything in it reaches.
    flake_nearness: [f32; 256],
    pub top: f32,
    pub turn: usize,
    front: Vec<bool>,
    front_size: (usize, usize),
    front_frames: Vec<[f32; 4]>,
    dab_weights: [f32; 25],
    /// The picture's air as the falling flakes' shader reads it, a cell to every terrain cell, RGBA: the light there,
    /// and how near what stands there is.
    pub air: Vec<u8>,
    pub air_version: u64,
}

impl Picture {
    pub fn new() -> Picture {
        let mut weights = [0f32; 25];
        for (k, weight) in weights.iter_mut().enumerate() {
            let (dx, dy) = ((k % 5) as f32 - 2.0, (k / 5) as f32 - 2.0);
            *weight = (-(dx * dx + dy * dy) / 2.2).exp();
        }
        let total: f32 = weights.iter().sum();
        Picture {
            near: Vec::new(),
            width: 0,
            height: 0,
            cell: 1,
            far: 0.0,
            close: 1.0,
            dab_amount: Vec::new(),
            speck_amount: Vec::new(),
            dabs: Vec::new(),
            specks: Vec::new(),
            speck_shown: Vec::new(),
            specks_easing: false,
            dabs_touched: false,
            has_dabs: false,
            level: 0.0,
            veil: 0.0,
            level_written: -1.0,
            veil_written: -1.0,
            version: 0,
            dabs_version: 0,
            blown_at: -10.0,
            ticked_at: 0.0,
            written_at: 0.0,
            flake_nearness: [0.0; 256],
            top: 0.0,
            turn: 0,
            front: vec![false],
            front_size: (1, 1),
            front_frames: Vec::new(),
            dab_weights: weights.map(|weight| weight * 3.0 / total),
            air: Vec::new(),
            air_version: 0,
        }
    }

    pub fn shown(&self) -> bool {
        self.width > 0
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// The surfaces of the wallpaper now shown, for the flakes to fall among, and its light; none for none.
    pub fn set(&mut self, terrain: Option<&Terrain>, light: Option<&Light>, screen_height: f32) {
        self.air.clear();
        self.air_version += 1;
        if let (Some(terrain), Some(light)) = (terrain, light)
            && light.width > 0
            && light.height > 0
            && light.pixels.len() >= light.width * light.height * 4
        {
            self.air = vec![255; terrain.width * terrain.height * 4];
            for y in 0..terrain.height {
                let ly = (y * light.height / terrain.height).min(light.height - 1);
                for x in 0..terrain.width {
                    let from = (ly * light.width + (x * light.width / terrain.width).min(light.width - 1)) * 4;
                    let to = (y * terrain.width + x) * 4;
                    self.air[to..to + 3].copy_from_slice(&light.pixels[from..from + 3]);
                    self.air[to + 3] = terrain.near[y * terrain.width + x];
                }
            }
        }
        self.near = terrain.map(|terrain| terrain.near.clone()).unwrap_or_default();
        let count = self.near.len();
        self.dab_amount = vec![0.0; count];
        self.speck_amount = vec![0.0; count];
        self.speck_shown = vec![0.0; count];
        self.dabs = vec![0; count];
        self.specks = vec![0; count];
        self.specks_easing = false;
        self.has_dabs = false;
        self.dabs_version += 1;
        self.version += 1;
        self.top = 0.0;
        self.width = terrain.map_or(0, |terrain| terrain.width);
        self.height = terrain.map_or(0, |terrain| terrain.height);
        self.cell = terrain.map_or(1, |terrain| terrain.cell.max(1));
        self.far = terrain.map_or(0.0, |terrain| terrain.far);
        self.close = terrain.map_or(1.0, |terrain| terrain.close);
        if let Some(terrain) = terrain {
            for (k, nearness) in self.flake_nearness.iter_mut().enumerate() {
                *nearness = terrain.far + (terrain.close - terrain.far) * (k as f32 / 255.0 / SCENE_DEPTH).powf(0.7);
            }
            let ground = (GROUND * 255.0) as u8;
            if let Some(first) = self.near.iter().position(|near| *near > ground) {
                self.top = screen_height - (first / terrain.width * terrain.cell) as f32;
            }
        }
    }

    /// Marks where the windows and the panels stand in front of the wallpaper, when they have moved: a flake over them
    /// is in front of them, not in the picture, and with a window over the whole screen that is every flake.
    pub fn cover(&mut self, frames: Vec<[f32; 4]>, width: f32, height: f32) {
        let size = ((width as usize / FRONT_CELL + 1).max(1), (height as usize / FRONT_CELL + 1).max(1));
        if frames == self.front_frames && size == self.front_size {
            return;
        }
        self.front = vec![false; size.0 * size.1];
        for frame in &frames {
            let columns = ((frame[0] as i32 / FRONT_CELL as i32).max(0) as usize)
                ..((frame[2] as i32 / FRONT_CELL as i32 + 1).min(size.0 as i32).max(0) as usize);
            let rows = (((height - frame[3]) as i32 / FRONT_CELL as i32).max(0) as usize)
                ..(((height - frame[1]) as i32 / FRONT_CELL as i32 + 1).min(size.1 as i32).max(0) as usize);
            for row in rows {
                for column in columns.clone() {
                    self.front[row * size.0 + column] = true;
                }
            }
        }
        self.front_frames = frames;
        self.front_size = size;
    }

    /// Whether a window or a panel stands in front of the wallpaper at a point, `y` from the screen's top.
    pub fn in_front(&self, x: f32, y: f32) -> bool {
        let (width, height) = self.front_size;
        self.front[((y as usize) / FRONT_CELL).min(height - 1) * width + ((x as usize) / FRONT_CELL).min(width - 1)]
    }

    /// How near a flake as deep as `depth` is in the picture.
    pub fn nearness(&self, depth: f32) -> f32 {
        self.flake_nearness[((depth * 255.0) as usize).min(255)]
    }

    /// Where a flake at `position` with `depth` is in the picture, in a screen `width` by `height`: nothing if it is in
    /// front of all of it there; else how near the picture is there and whether the flake lands on it.
    pub fn falls(&self, position: V2, depth: f32, width: f32, height: f32) -> Option<(f32, bool)> {
        // Most flakes are over the sky or still in front of what is under them: those are told apart first, cheaply.
        if position.y >= self.top || position.y < 0.0 || position.x < 0.0 || position.x >= width {
            return None;
        }
        let (x, y) = (position.x, height - position.y);
        if self.in_front(x, y) {
            return None;
        }
        let (cx, cy) = (((x as usize) / self.cell).min(self.width - 1), ((y as usize) / self.cell).min(self.height - 1));
        let level = self.near[cy * self.width + cx];
        let own = self.nearness(depth);
        let near = level as f32 / 255.0;
        if level <= (GROUND * 255.0) as u8 || near < own - SCENE_TOLERANCE {
            return None;
        }
        Some((near, near <= own + SCENE_TOLERANCE))
    }

    /// How far off `near` is in the picture, 0 nearest to 1 farthest.
    pub fn farness(&self, near: f32) -> f32 {
        1.0 - ((near - self.far) / (self.close - self.far).max(0.01)).clamp(0.0, 1.0)
    }

    /// A flake has landed in the picture: a dab of snow around where it fell, bigger for a bigger flake, and the farther
    /// off the less of it shows, as everything there is smaller.
    pub fn lay_dab(&mut self, size: f32, near: f32, position: V2, height: f32) {
        let (x, y) = (position.x, height - position.y);
        let cell = self.cell as f32;
        let (cx, cy) = (((x / cell) as usize).min(self.width - 1) as i32, ((y / cell) as usize).min(self.height - 1) as i32);
        let nearness = 1.0 - self.farness(near);
        let dab = size * size * 0.004 * (0.3 + 0.7 * nearness);
        for dy in -2..=2 {
            for dx in -2..=2 {
                let (nx, ny) = (cx + dx, cy + dy);
                if nx < 0 || nx >= self.width as i32 || ny < 0 || ny >= self.height as i32 {
                    continue;
                }
                let k = ny as usize * self.width + nx as usize;
                self.dab_amount[k] = (self.dab_amount[k] + dab * self.dab_weights[((dy + 2) * 5 + dx + 2) as usize]).min(1.0);
            }
        }
        // The frost lies where the flake touched, shared among the four cells round that point as the texture will blend
        // them back, so that it shows there and not at a cell's middle.
        let (fx, fy) = (x / cell - 0.5, y / cell - 0.5);
        let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
        let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
        let speck = (size * 0.3).min(0.9) * (0.6 + 0.4 * nearness) * 1.6;
        for (dx, dy, share) in [(0, 0, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] {
            let (nx, ny) = (x0 + dx, y0 + dy);
            if nx < 0 || nx >= self.width as i32 || ny < 0 || ny >= self.height as i32 {
                continue;
            }
            let k = ny as usize * self.width + nx as usize;
            self.speck_amount[k] = (self.speck_amount[k] + speck * share).min(1.0);
        }
        self.dabs_touched = true;
        self.has_dabs = true;
    }

    /// The snow in a picture as it is `seconds` later, from its `level`, with `falling` how thick the snowfall is against
    /// the usual and `melting` whether nothing keeps it. At the usual snowfall it takes about eight minutes to cover all
    /// it can, and then most of an hour to lie as deep as it gets; melting, it never lies deeper than it covers, and goes
    /// in a few minutes once the snowfall stops.
    pub fn snow_after(level: f32, seconds: f32, falling: f32, melting: bool) -> f32 {
        let most = if melting { 1.0 } else { DEEPEST };
        let (mut level, mut left) = (level, seconds);
        if level < 1.0 && falling > 0.0 {
            let covering = left.min((1.0 - level) * 480.0 / falling);
            level += falling * covering / 480.0;
            left -= covering;
        }
        if (1.0..most).contains(&level) && falling > 0.0 {
            level = (level + falling * left / 3000.0).min(most);
        }
        if level > most {
            level = most.max(level - seconds / 300.0);
        }
        if falling <= 0.0 && melting {
            level = (level - seconds / 180.0).max(0.0);
        }
        level
    }

    /// The snow in the picture grows with the snowfall, `falling` and `thick`, melts, or goes with a gust clearing the
    /// snow; and is written out for drawing, five times a second at most while it changes.
    pub fn tend(&mut self, dt: f32, clock: f64, falling: f32, thick: f32, melting: bool, frost: bool) {
        self.turn = self.turn.wrapping_add(1);
        if !self.shown() {
            return;
        }
        if !frost {
            self.level = 0.0;
        }
        self.veil += (0.3 * thick.sqrt() - self.veil) * (dt / 12.0).min(1.0);
        if self.veil < 0.002 && thick == 0.0 {
            self.veil = 0.0;
        }
        if clock - self.blown_at < 1.5 {
            self.level = 0.0;
            if self.has_dabs {
                self.dab_amount.fill(0.0);
                self.speck_amount.fill(0.0);
                self.speck_shown.fill(0.0);
                self.specks_easing = false;
                self.has_dabs = false;
                self.dabs_touched = true;
            }
        } else {
            self.level = Self::snow_after(self.level, dt, falling, melting);
            // The dabs left by the flakes seen landing melt as the rest does, a second at a time.
            if clock - self.ticked_at >= 1.0 {
                let elapsed = (clock - self.ticked_at).min(5.0) as f32;
                self.ticked_at = clock;
                if melting && self.has_dabs {
                    let loss = elapsed / 180.0;
                    let (mut most, mut most_speck) = (0f32, 0f32);
                    for amount in &mut self.dab_amount {
                        *amount = (*amount - loss).max(0.0);
                        most = most.max(*amount);
                    }
                    for amount in &mut self.speck_amount {
                        *amount = (*amount - loss).max(0.0);
                        most_speck = most_speck.max(*amount);
                    }
                    self.has_dabs = most > 0.0 || most_speck > 0.0;
                    self.dabs_touched = true;
                }
            }
        }
        // Told five times a second while anything in it changes: in steps that small the picture can be drawn one after
        // another without fading each in over the last.
        let changed = self.dabs_touched || self.specks_easing || self.level != self.level_written || self.veil != self.veil_written;
        if clock - self.written_at < 0.2 || !changed {
            return;
        }
        let elapsed = (clock - self.written_at).min(1.0) as f32;
        self.written_at = clock;
        self.level_written = self.level;
        self.veil_written = self.veil;
        self.version += 1;
        if !(self.dabs_touched || self.specks_easing) {
            return;
        }
        self.dabs_touched = false;
        // A flake's frost comes in over most of a second.
        let share = (elapsed / 0.5).min(1.0);
        let mut widest = 0f32;
        for (shown, amount) in self.speck_shown.iter_mut().zip(&self.speck_amount) {
            let gap = amount - *shown;
            widest = widest.max(gap.abs());
            *shown += gap * share;
        }
        if widest < 0.5 / 255.0 {
            self.speck_shown.copy_from_slice(&self.speck_amount);
            self.specks_easing = false;
        } else {
            self.specks_easing = true;
        }
        let byte = |amount: &f32| (amount.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        for (to, from) in self.dabs.iter_mut().zip(&self.dab_amount) {
            *to = byte(from);
        }
        for (to, from) in self.specks.iter_mut().zip(&self.speck_shown) {
            *to = byte(from);
        }
        self.dabs_version += 1;
    }
}

impl SnowSimulation {
    /// The surfaces of the wallpaper now shown, for the flakes to fall among, and its light; none for none.
    pub fn set_picture(&mut self, terrain: Option<&Terrain>, light: Option<&Light>) {
        self.picture.set(terrain, light, self.height);
    }

    /// How much snow has fallen over the whole picture, up to `PICTURE_DEEPEST`, and how much the snow now falling
    /// veils its farthest things.
    pub fn picture_level(&self) -> f32 {
        self.picture.level
    }

    pub fn picture_veil(&self) -> f32 {
        self.picture.veil
    }

    /// Counts up whenever the snow in the picture changes, at most five times a second.
    pub fn picture_version(&self) -> u64 {
        self.picture.version
    }

    /// What the flakes seen landing have left, and the frost right where each came down, a byte a terrain cell, with
    /// the grid's size and a count that goes up whenever they change.
    pub fn picture_dabs(&self) -> (&[u8], &[u8], (usize, usize), u64) {
        (&self.picture.dabs, &self.picture.specks, self.picture.size(), self.picture.dabs_version)
    }

    pub fn has_picture_dabs(&self) -> bool {
        self.picture.has_dabs
    }

    /// The picture's light and nearness for the flakes falling in it, RGBA a terrain cell, with its size and a count
    /// that goes up whenever it changes; empty while there is no picture.
    pub fn picture_air(&self) -> (&[u8], (usize, usize), u64) {
        let size = if self.picture.air.is_empty() { (0, 0) } else { self.picture.size() };
        (&self.picture.air, size, self.picture.air_version)
    }

    /// Puts back the snow that lay in the picture when the app last ran, or lies in the one now showing.
    pub fn restore_picture(&mut self, level: f32) {
        self.picture.level = level.clamp(0.0, DEEPEST);
    }

    /// The display was dark or away for `seconds`: it went on snowing meanwhile.
    pub fn pass_picture(&mut self, seconds: f32) {
        if self.picture.shown() && seconds > 0.0 {
            self.picture.level = Picture::snow_after(self.picture.level, seconds.min(8.0 * 3600.0), self.picture_falling(), self.picture_melting());
        }
    }

    /// The snow in any picture `seconds` after it lay at `level`: a wallpaper out of sight is snowed on too.
    pub fn snow_in_picture(&self, level: f32, seconds: f32) -> f32 {
        Picture::snow_after(level, seconds, self.picture_falling(), self.picture_melting())
    }

    /// Whether a gust has cleared the snow since this was last asked: the picture's goes with it.
    pub fn take_picture_cleared(&mut self) -> bool {
        std::mem::take(&mut self.picture_cleared)
    }

    /// How thick the snowfall on the picture is against the usual, and whether nothing keeps what lies there.
    fn picture_falling(&self) -> f32 {
        let snowing = self.air.modes.contains(&Mode::Snow) && self.accumulates && !self.holding() && self.picture_frost;
        if snowing { flakes_per_megapoint(self.intensity) * self.density / flakes_per_megapoint(0.5) } else { 0.0 }
    }

    fn picture_melting(&self) -> bool {
        self.thaws || !self.accumulates || !self.air.modes.contains(&Mode::Snow)
    }

    /// The snow in the picture is melting away with nothing falling: the simulation goes on until it is gone.
    pub(super) fn picture_thaws(&self) -> bool {
        let picture = &self.picture;
        picture.shown() && self.intensity <= 0.0 && ((picture.level > 0.0 && self.picture_melting()) || picture.veil > 0.0)
    }

    pub(super) fn tend_picture(&mut self, dt: f32) {
        let falling = self.picture_falling();
        let snowing = self.air.modes.contains(&Mode::Snow) && !self.holding() && self.picture_frost;
        let thick = if snowing { (flakes_per_megapoint(self.intensity) * self.density / flakes_per_megapoint(1.0)).min(1.0) } else { 0.0 };
        let melting = self.picture_melting();
        self.picture.tend(dt, self.air.clock, falling, thick, melting, self.picture_frost);
    }

    /// What stands in front of the desktop: every app window, and the panel the ground stands on.
    pub(super) fn cover_picture(&mut self) {
        let mut frames = self.covers.clone();
        let panel = self.layers[0].base().y;
        if panel > 0.0 {
            frames.push([0.0, 0.0, self.width, panel]);
        }
        self.picture.cover(frames, self.width, self.height);
    }

    /// A flake far enough off falls in the picture, at its own depth. Where something in the wallpaper is nearer than
    /// it, it has gone behind that; where a surface is as near as it is, it lands there and leaves a little snow.
    /// Returns whether it is gone.
    pub(super) fn falls_in_picture(&mut self, flake: &Flake) -> bool {
        let Some((near, lands)) = self.picture.falls(flake.position, flake.depth, self.width, self.height) else { return false };
        if lands && self.picture_frost && self.accumulates {
            self.picture.lay_dab(flake.size, near, flake.position, self.height);
            if self.air.motes.len() < MAX_MOTES {
                let light = 1.0 - ((flake.depth - 0.45) / 0.45).clamp(0.0, 1.0);
                self.air.motes.push(Mote {
                    position: flake.position,
                    size: flake.size,
                    alpha: flake.alpha,
                    life: 1.6,
                    scene: Some((self.picture.nearness(flake.depth), light)),
                    still: Some(flake.soft),
                    ..Default::default()
                });
            }
        }
        true
    }
}
