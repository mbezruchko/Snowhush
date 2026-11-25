//! What the renderers of the snow lying in a wallpaper share, as the Mac's PictureRenderer works it out: the caps along
//! the ridges, the noise the snow's edge is broken up with, how thick it lies, and the part of the picture a simulation
//! covers.

use super::{Light, Ridge, Scene, Terrain};
use crate::geometry::smoothstep;

/// A cap's full thickness on the nearest ridge once all the snow has fallen, in points.
const CAP_DEPTH: f32 = 9.0;
/// How thick the snow lies on the nearest level ground once all of it has fallen, in points.
const DEPTH: f32 = 5.0;

/// One ridge's cap: its columns in the list of them all, where it starts and how near it is.
#[derive(Clone, Copy, Debug)]
pub struct Cap {
    pub first: usize,
    pub count: usize,
    pub left: f32,
    pub near: f32,
}

/// How much of the snow the shaders show for `level` fallen: the first snow shows soon; past covering what holds it, it
/// goes on getting deeper at half the pace.
pub fn amount(level: f32) -> f32 {
    if level <= 1.0 { level.max(0.0).powf(0.6) } else { 1.0 + (level - 1.0) * 0.5 }
}

/// How thick the snow lies where it lies thickest, in points.
pub fn thickest(level: f32) -> f32 {
    (DEPTH * level.max(0.0).powf(0.7)).max(0.01)
}

/// A cap's full thickness on a ridge `near`: the farther off, the thinner the same snow looks on it.
pub fn cap_thickness(level: f32, near: f32) -> f32 {
    CAP_DEPTH * level.max(0.0).powf(0.8) * (0.35 + 0.65 * near)
}

/// Each ridge's cap, worked out once: thinner where the ridge climbs steeply, uneven along it as drifted snow is, and
/// tapering away to its ends. A column is its top, that top smoothed along the ridge, how much of a cap's full thickness
/// lies there, and how far the cap's foot is sunk into the thing's top, with eight added on a mountain's ridge.
pub fn caps(ridges: &[Ridge], column_width: f32) -> (Vec<[f32; 4]>, Vec<Cap>) {
    let fraction = |x: f32| x - x.floor();
    let mut columns = Vec::new();
    let mut caps = Vec::new();
    for ridge in ridges.iter().filter(|ridge| ridge.tops.len() > 4) {
        let tops = &ridge.tops;
        let count = tops.len();
        // The same ridge always gets the same cap, however often its wallpaper is read again.
        let seed = ridge.left * 12.9898 + count as f32 * 78.233;
        let phases =
            (2.0 * std::f32::consts::PI * fraction(seed.sin() * 43758.547), 2.0 * std::f32::consts::PI * fraction((seed + 1.7).sin() * 43758.547));
        let near = ridge.nearness.clamp(0.0, 1.0);
        caps.push(Cap { first: columns.len(), count, left: ridge.left, near });
        for k in 0..count {
            let (a, b) = (k.saturating_sub(3), (k + 3).min(count - 1));
            let slope = (tops[b] - tops[a]).abs() / ((b - a) as f32 * column_width);
            let lie = (1.0 - slope / 2.2).clamp(0.3, 1.0);
            let x = k as f32 * column_width;
            let uneven = 0.85 + 0.2 * (x * 0.045 + phases.0).sin() + 0.1 * (x * 0.13 + phases.1).sin();
            let end = (k.min(count - 1 - k) as f32 / 10.0).min(1.0);
            let taper = end * end * (3.0 - 2.0 * end);
            let (low, high) = (k.saturating_sub(5), (k + 5).min(count - 1));
            let smooth = tops[low..=high].iter().sum::<f32>() / (high - low + 1) as f32;
            let room = ridge.room.get(k).copied().unwrap_or(1.0);
            // A cap sinks its foot three points into a mountain's top, and hardly at all into a branch's.
            let foot = 1.0 + 2.0 * smoothstep(0.25, 0.9, room);
            // On a branch the cap goes by the branch's own thickness, however near it is.
            let slim = (room / (0.35 + 0.65 * near)).min(1.0);
            let carried = slim + (1.0 - slim) * smoothstep(0.6, 0.95, room);
            // A mountain far off is small on the screen, and a cap a few points thick on it would be a drift as deep as a
            // house: there the snow shows on its faces, and its ridge is left without a line drawn round it.
            let mountain = if ridge.mass.get(k).is_some_and(|mass| *mass >= 0.5) { 1.0 } else { 0.0 };
            let far = 1.0 - mountain * (1.0 - smoothstep(0.3, 0.45, near));
            columns.push([tops[k], smooth, lie * uneven * taper * carried * far, foot + 8.0 * mountain]);
        }
    }
    (columns, caps)
}

/// A gradient noise that tiles, 32 cells across 256 pixels, a byte each: unlike noise of values at a grid's corners it
/// has no square blobs for a threshold to cut out. Always the same, so the snow's edge doesn't move between drawings.
pub fn noise() -> Vec<u8> {
    let (size, cells) = (256usize, 32usize);
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    let slopes: Vec<(f32, f32)> = (0..cells * cells)
        .map(|_| {
            let angle = next() * 2.0 * std::f32::consts::PI;
            (angle.cos(), angle.sin())
        })
        .collect();
    let slope = |x: i32, y: i32| slopes[((y + cells as i32) as usize % cells) * cells + (x + cells as i32) as usize % cells];
    let dot = |s: (f32, f32), x: f32, y: f32| s.0 * x + s.1 * y;
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let step = cells as f32 / size as f32;
    let mut pixels = vec![0u8; size * size];
    for py in 0..size {
        for px in 0..size {
            let (x, y) = ((px as f32 + 0.5) * step, (py as f32 + 0.5) * step);
            let (ix, iy) = (x.floor(), y.floor());
            let (fx, fy) = (x - ix, y - iy);
            let (cx, cy) = (ix as i32, iy as i32);
            let (sx, sy) = (fade(fx), fade(fy));
            let a = dot(slope(cx, cy), fx, fy);
            let b = dot(slope(cx + 1, cy), fx - 1.0, fy);
            let c = dot(slope(cx, cy + 1), fx, fy - 1.0);
            let d = dot(slope(cx + 1, cy + 1), fx - 1.0, fy - 1.0);
            let (top, bottom) = (a + (b - a) * sx, c + (d - c) * sx);
            pixels[py * size + px] = ((0.5 + 0.75 * (top + (bottom - top) * sy)).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    pixels
}

impl Scene {
    /// The part of what was found in the picture a simulation covers, `place` its left and top on the display and its
    /// size, in points: the simulation knows only its own part of the display, which may leave out panels. The light
    /// comes at the terrain's cells. Returns where the part starts on the display, in points, rounded to the cells.
    pub fn cropped(&self, place: [f32; 4]) -> (Terrain, Light, [f32; 2]) {
        let whole = &self.terrain;
        let cell = whole.cell.max(1);
        let x0 = ((place[0] / cell as f32).round() as usize).min(whole.width.saturating_sub(1));
        let y0 = ((place[1] / cell as f32).round() as usize).min(whole.height.saturating_sub(1));
        let width = ((place[2] / cell as f32).ceil() as usize).min(whole.width - x0).max(1);
        let height = ((place[3] / cell as f32).ceil() as usize).min(whole.height - y0).max(1);
        let light = &self.light;
        let mut near = Vec::with_capacity(width * height);
        let mut pixels = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let (wx, wy) = (x0 + x, y0 + y);
                near.push(whole.near[wy * whole.width + wx]);
                let (lx, ly) = ((wx * cell / light.cell).min(light.width - 1), (wy * cell / light.cell).min(light.height - 1));
                let k = (ly * light.width + lx) * 4;
                pixels.extend_from_slice(&light.pixels[k..k + 4]);
            }
        }
        let terrain = Terrain { width, height, cell, near, far: whole.far, close: whole.close };
        let light = Light { width, height, cell, pixels, haze: light.haze };
        (terrain, light, [(x0 * cell) as f32, (y0 * cell) as f32])
    }
}
