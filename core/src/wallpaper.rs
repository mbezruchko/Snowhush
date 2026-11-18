//! The wallpaper as a display shows it, and what snow on it needs to know of it: the tops of the things in it found
//! from a depth map, where its surfaces hold snow, and how the picture lights it. Wherever a nearer thing ends against
//! something farther behind it, its top edge can catch snow. The same as the Mac's Wallpaper.swift: the depth comes
//! from a network the platform runs, once per picture, and what is found is kept on disk.

mod cache;
pub mod geometry;
mod ridges;
#[cfg(test)]
mod tests;

pub use cache::name as cache_name;
pub use geometry::{Grid, Network, depths, snapped};
pub use ridges::find;

use crate::geometry::smoothstep;

/// The sky's nearness and less, in the depth map's own range.
pub const SKY: f32 = 0.06;
/// Clouds come out a little nearer than the sky: only what is nearer still is ground for snow to lie on.
pub const GROUND: f32 = 0.12;
/// How much grain the picture shows before it counts as a thing with a surface, as a share of how light it is there.
/// A bank of cloud, mist, anything out of focus and snow lying smooth already show next to none; rock, bark, needles
/// and the ripples of a dune show several times this.
const GRAIN: (f32, f32) = (0.012, 0.0225);
/// How fast the depth grows nearer going down on ground that holds snow fully, where a picture has none to measure.
const RISE: (f32, f32) = (0.001, 0.006);
/// How steep a step in depth, over four points, is the edge of something rather than a surface.
const JUMP: (f32, f32) = (0.012, 0.03);

/// What snow on the wallpaper needs to know of it: where it can lie, and how the picture lights it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub ridges: Vec<Ridge>,
    pub light: Light,
    pub terrain: Terrain,
    pub relief: Relief,
}

/// What the snow lying in the wallpaper is drawn from, a cell to every `cell` points, top row first, each 0 to 255:
/// how light the picture is, so snow shows first where it is lighter than around, on flat ground and old snow, and last
/// on the dark of trees and rock; how much each cell is the top of something darker than what is just over it, a
/// branch against the mist, a ledge under the sky; how well it holds snow, from 0 for the sky, a cliff or the side of
/// something to 1 for ground facing up; and how near it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Relief {
    pub width: usize,
    pub height: usize,
    pub cell: usize,
    pub lightness: Vec<u8>,
    pub tops: Vec<u8>,
    pub hold: Vec<u8>,
    pub near: Vec<u8>,
}

impl Relief {
    /// The four planes side by side, RGBA: lightness, tops, hold, near.
    pub fn packed(&self) -> Vec<u8> {
        let count = self.width * self.height;
        let mut packed = vec![0; count * 4];
        for (k, plane) in [&self.lightness, &self.tops, &self.hold, &self.near].into_iter().enumerate() {
            for (i, value) in plane.iter().take(count).enumerate() {
                packed[4 * i + k] = *value;
            }
        }
        packed
    }
}

/// How near each spot of the wallpaper is, for the flakes that fall among its things: a cell to every `cell` points,
/// top row first, 0 to 255 for 0 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    pub width: usize,
    pub height: usize,
    pub cell: usize,
    pub near: Vec<u8>,
    /// How near the farthest and the nearest of the things in it are, the sky aside.
    pub far: f32,
    pub close: f32,
}

/// The light snow would take on across the wallpaper, a cell to every `cell` points, top row first, RGBA: from the
/// brightest light on what it lies on, so it glows where the picture glows and greys where it is dusk.
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub width: usize,
    pub height: usize,
    pub cell: usize,
    pub pixels: Vec<u8>,
    /// The colour things fade into far off: of the farthest of what the picture shows, the sky if it has one. Red,
    /// green and blue, 0 to 1.
    pub haze: [f32; 3],
}

/// The top of something in the wallpaper that snow can lie on: a ridge against the sky, a roof, a branch.
#[derive(Clone, Debug, PartialEq)]
pub struct Ridge {
    /// Where its first column starts, in points from the screen's left edge.
    pub left: f32,
    /// The top of the thing at each column, in points from the screen's bottom.
    pub tops: Vec<f32>,
    /// How near it looks, from 0 for the horizon to 1 for right in front.
    pub nearness: f32,
    /// How much of a full cap of snow each column can carry, 0 to 1: a mountain all of it, a thin branch a thin line.
    pub room: Vec<f32>,
    /// Whether each column is a mountain's, 1, or a branch's or a rock's, 0: under a mountain's ridge the sky over it
    /// doesn't show again.
    pub mass: Vec<f32>,
}

/// The screen's wallpaper pixels, a pixel to a point, top row first, RGBX.
#[derive(Clone, Debug)]
pub struct Shown {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Shown {
    fn rgb(&self, i: usize) -> [f32; 3] {
        [self.pixels[i] as f32, self.pixels[i + 1] as f32, self.pixels[i + 2] as f32]
    }

    /// The mean of three pixels across, at `x`, on row `y`.
    fn colour(&self, x: i32, y: i32) -> [f32; 3] {
        let mut sum = [0.0; 3];
        for dx in -1..=1 {
            let i = (y as usize * self.width + (x + dx).clamp(0, self.width as i32 - 1) as usize) * 4;
            sum = add(sum, self.rgb(i));
        }
        scale(sum, 1.0 / 3.0)
    }
}

/// Everything found in the picture, with the depth from `network`. Slow: seconds, for another process to do.
pub fn scene(shown: &Shown, network: &mut dyn Network, column_width: f32) -> Option<Scene> {
    let depths = depths(shown, network, (3, 2))?;
    let cell = 2;
    let lightness = lightness(shown, cell);
    let depth = snapped(&depths.fine, &lightness, 4, 0.0004);
    let far = haze(shown, &depth);
    let mut light = light(shown, far, skylight(shown, &depth), 16);
    light.haze = far;
    let mut ridges = find(shown, &depths.whole, column_width);
    for ridge in &mut ridges {
        let (room, lift) = body(ridge, shown, &depth, cell as f32, column_width);
        for (top, lift) in ridge.tops.iter_mut().zip(lift) {
            *top += lift;
        }
        ridge.mass = mass(ridge, shown, column_width);
        // No cap on what is as white as the cap: a line of the same snow along a ridge of snow is an outline.
        ridge.room = room
            .iter()
            .enumerate()
            .map(|(j, room)| {
                let x = (ridge.left + (j as f32 + 0.5) * column_width) as i32;
                let y = shown.height as i32 - ridge.tops[j] as i32 + 8;
                room * (1.0 - white(x, y, shown, &light))
            })
            .collect();
    }
    let hold = hold(&depth, &lightness, shown, &light, cell as f32, shown.height as f32, &ridges, column_width);
    let relief = Relief {
        width: depth.width,
        height: depth.height,
        cell,
        tops: tops(&lightness, depth.width),
        lightness,
        hold,
        near: depth.values.iter().map(|value| (value.clamp(0.0, 1.0) * 255.0) as u8).collect(),
    };
    Some(Scene { ridges, light, terrain: terrain(&depth), relief })
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], by: f32) -> [f32; 3] {
    [a[0] * by, a[1] * by, a[2] * by]
}

fn length(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn largest(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2])
}

fn smallest(c: [f32; 3]) -> f32 {
    c[0].min(c[1]).min(c[2])
}

fn sorted(values: &[f32]) -> Vec<f32> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted
}

// MARK: - Light

/// Snow lit by the picture, a cell to every sixteen points. Snow is as white as anything gets: it looks as the whitest
/// things about it do, the snow or cloud already in the picture, and a little brighter for being fresh. So each cell
/// takes the whitest quarter of its pixels, of a few cells at and below it the lightest. What is that white, and not
/// strongly coloured as lit rock is, shows the light's own colour, and the snow keeps most of it: blue at the blue
/// hour, glowing under a sunset. Of the colour of anything less white it keeps a little, and none where the bright
/// things are of another colour than all about them: twigs and lamps among needles are things, not light. Where
/// nothing about is white, in a dark wood, the snow is lit by the sky the picture has, `sky`, and takes its cast.
pub fn light(shown: &Shown, sky: [f32; 3], skylight: f32, cell: usize) -> Light {
    let width = shown.width.div_ceil(cell);
    let height = shown.height.div_ceil(cell);
    let bins = 32;
    let mut counts = vec![0u32; width * height * bins];
    let mut all = [0usize; 256];
    let level_of = |p: &[u8], i: usize| (2126 * p[i] as usize + 7152 * p[i + 1] as usize + 722 * p[i + 2] as usize) / 10000;
    let p = &shown.pixels;
    for y in 0..shown.height {
        let row = (y / cell) * width;
        for x in 0..shown.width {
            let level = level_of(p, (y * shown.width + x) * 4);
            counts[(row + x / cell) * bins + level * bins / 256] += 1;
            all[level] += 1;
        }
    }
    // Where each cell's whitest quarter begins.
    let quarters: Vec<usize> = (0..width * height)
        .map(|k| {
            let total: u32 = counts[k * bins..(k + 1) * bins].iter().sum();
            let (mut seen, mut bin) = (0, bins - 1);
            while bin > 0 && seen + counts[k * bins + bin] < total / 4 {
                seen += counts[k * bins + bin];
                bin -= 1;
            }
            bin
        })
        .collect();
    let mut whites = vec![[0f32; 4]; width * height];
    let mut sums = vec![[0f32; 4]; width * height];
    for y in 0..shown.height {
        let row = (y / cell) * width;
        for x in 0..shown.width {
            let i = (y * shown.width + x) * 4;
            let k = row + x / cell;
            let level = level_of(p, i);
            let colour = [p[i] as f32, p[i + 1] as f32, p[i + 2] as f32, 255.0];
            for c in 0..4 {
                sums[k][c] += colour[c];
            }
            if level * bins / 256 >= quarters[k] {
                for c in 0..4 {
                    whites[k][c] += colour[c];
                }
            }
        }
    }
    let mean = |s: &[f32; 4]| if s[3] > 0.0 { [s[0] / s[3], s[1] / s[3], s[2] / s[3]] } else { [0.0; 3] };
    let colours: Vec<[f32; 3]> = whites.iter().map(mean).collect();
    let means: Vec<[f32; 3]> = sums.iter().map(mean).collect();
    // The whitest fiftieth of the whole picture: what white looks like in it.
    let (mut above, mut whitest) = (0, 255);
    while whitest > 0 && above + all[whitest] < shown.width * shown.height / 50 {
        above += all[whitest];
        whitest -= 1;
    }
    let white = whitest as f32 / 255.0;
    // Snow is never far under the picture's whites, or under what can be told from the dark at all; nor is it brighter
    // than the sky that lights it. Under a dusk sky the snow in a peak's shade is dim, however bright the peak's lit
    // side, and snow laid there as bright as that would be a pale patch.
    let least = (0.9 * white.min(skylight)).max(0.45);
    // A colour as a cast on white, keeping `kept` of how far it is from grey.
    let cast = |c: [f32; 3], kept: f32| -> [f32; 3] {
        let grey = luminance(c);
        if grey <= 0.004 {
            return [1.0; 3];
        }
        let mixed = add([grey; 3], scale(sub(c, [grey; 3]), kept));
        mixed.map(|v| (v / grey).clamp(0.6, 1.5))
    };
    let overhead = cast(sky, 0.35);
    // As white as the picture's whites and not strongly coloured: it shows the light itself.
    let showing = |c: [f32; 3]| -> f32 {
        let strong = (largest(c) - smallest(c)) / largest(c).max(0.02);
        smoothstep(0.5, 0.9, luminance(c) / white.max(0.05)) * (1.0 - smoothstep(0.3, 0.6, strong))
    };
    let mut pixels = vec![255u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let (mut best, mut lightest, mut whole) = ([0.0; 3], -1.0, [0.0; 3]);
            for dy in 0..=2 {
                for dx in -1..=1 {
                    let k = (y + dy).min(height - 1) * width + (x as i32 + dx).clamp(0, width as i32 - 1) as usize;
                    let l = luminance(colours[k]);
                    if l > lightest {
                        lightest = l;
                        best = colours[k];
                        whole = means[k];
                    }
                }
            }
            // Snow in the shade of a peak is white too, and lit less than the snow in the sun beside it: a cell with
            // white of its own goes by that, not by a brighter neighbour's.
            let own = colours[y * width + x];
            let mine = showing(own);
            best = add(best, scale(sub(own, best), mine));
            whole = add(whole, scale(sub(means[y * width + x], whole), mine));
            lightest = luminance(best);
            let shows = showing(best);
            let hue = cast(best, 0.25 + 0.55 * shows);
            let level = (lightest * 1.2).max(least).min(0.95);
            let dim = ((least - lightest * 1.2) / least).clamp(0.0, 1.0);
            let apart = sub(cast(best, 1.0), cast(whole, 1.0));
            let alike = if luminance(whole) > 0.03 { 1.0 - smoothstep(0.3, 0.7, length(apart)) } else { 1.0 };
            let sure = smoothstep(0.06, 0.2, lightest) * (1.0 - 0.5 * dim) * alike;
            let tint = scale(add(overhead, scale(sub(hue, overhead), sure)), level);
            let i = (y * width + x) * 4;
            for c in 0..3 {
                pixels[i + c] = (tint[c].clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
    Light { width, height, cell, pixels, haze: [0.8, 0.82, 0.85] }
}

/// How nearly the picture is, at a spot, already the snow that would be laid there, 0 to 1: about as light and of its
/// colour. Snow in the shade of a peak is, and pale rock; snow on it would show as a flat patch a shade off, or as
/// nothing. What is of another colour than the snow's light, rock in a sunset, is not, however bright.
pub fn white(x: i32, y: i32, shown: &Shown, light: &Light) -> f32 {
    let mut sum = [0.0; 3];
    for dy in 0..=1 {
        for dx in 0..=1 {
            let row = (y + dy).clamp(0, shown.height as i32 - 1) as usize;
            let column = (x + dx).clamp(0, shown.width as i32 - 1) as usize;
            sum = add(sum, shown.rgb((row * shown.width + column) * 4));
        }
    }
    let here = scale(sum, 1.0 / (4.0 * 255.0));
    let row = (y / light.cell as i32).clamp(0, light.height as i32 - 1) as usize;
    let column = (x / light.cell as i32).clamp(0, light.width as i32 - 1) as usize;
    let k = (row * light.width + column) * 4;
    let snow = [light.pixels[k] as f32 / 255.0, light.pixels[k + 1] as f32 / 255.0, light.pixels[k + 2] as f32 / 255.0];
    let against = [here[0] / snow[0].max(0.05), here[1] / snow[1].max(0.05), here[2] / snow[2].max(0.05)];
    let unlike = (largest(against) - smallest(against)) / largest(against).max(0.02);
    smoothstep(0.55, 0.75, luminance(against)) * (1.0 - smoothstep(0.25, 0.5, unlike))
}

/// The pixels of the farthest tenth of the picture, a few apart.
fn farthest(shown: &Shown, depth: &Grid) -> Vec<[f32; 3]> {
    let ordered = sorted(&depth.values);
    let far = ordered[ordered.len() / 10];
    let cell = shown.width / depth.width;
    let mut found = Vec::new();
    for y in (0..depth.height).step_by(2) {
        for x in (0..depth.width).step_by(2) {
            if depth.values[y * depth.width + x] <= far {
                found.push(shown.rgb((y * cell * shown.width + x * cell) * 4));
            }
        }
    }
    found
}

/// How light the picture's sky is, 0 to 1: the brighter half of its farthest tenth. The half, since through a wood's
/// canopy the farthest of the picture is as much dark crowns as sky.
pub fn skylight(shown: &Shown, depth: &Grid) -> f32 {
    let levels: Vec<f32> = farthest(shown, depth).into_iter().map(|c| luminance(c) / 255.0).collect();
    if levels.is_empty() {
        return 0.8;
    }
    let levels = sorted(&levels);
    let upper = &levels[levels.len() / 2..];
    upper.iter().sum::<f32>() / upper.len() as f32
}

/// The mean colour of the farthest tenth of the picture: its sky, or failing one the mist or the far wood behind
/// everything else.
pub fn haze(shown: &Shown, depth: &Grid) -> [f32; 3] {
    let far = farthest(shown, depth);
    if far.is_empty() {
        return [0.8, 0.82, 0.85];
    }
    let sum = far.iter().fold([0.0; 3], |sum, c| add(sum, *c));
    scale(sum, 1.0 / far.len() as f32 / 255.0)
}

// MARK: - Terrain

/// How thick the thing under each column of a ridge is, as the room it leaves for a cap, 0 to 1, and how far over the
/// ridge's line its own top edge shows, in points. A cap can't stand much thicker than what it stands on. A mountain
/// goes on down from its ridge: in the depth map it steps nearer there and stays so. A branch is a sliver: it steps
/// nearer and, a few points down, back again. How thick the sliver is, and where its top edge lies, the picture itself
/// shows better than the depth map, which smears it.
pub fn body(ridge: &Ridge, shown: &Shown, depth: &Grid, cell: f32, column_width: f32) -> (Vec<f32>, Vec<f32>) {
    let full = 12.0;
    let size = (cell as usize).max(1);
    let count = ridge.tops.len();
    let mut slivers = Vec::with_capacity(count);
    let mut thick = Vec::with_capacity(count);
    let mut lifts = Vec::with_capacity(count);
    for (k, top) in ridge.tops.iter().enumerate() {
        let x = (ridge.left + (k as f32 + 0.5) * column_width) as i32;
        let y = shown.height as i32 - *top as i32;
        let (mut sliver, mut thickness, mut lift) = (false, full, 0.0f32);
        if x >= 0 && x < shown.width as i32 && y >= 16 && y + 16 < shown.height as i32 {
            let near = |r: i32| -> f32 {
                let row = (((y + r) as usize) / size).min(depth.height - 1);
                depth.values[row * depth.width + (x as usize / size).min(depth.width - 1)]
            };
            let front = [-6, -4, -2, 0, 2].into_iter().map(near).fold(0.0, f32::max);
            let under = [4, 6, 8, 10, 12, 14].into_iter().map(near).fold(f32::INFINITY, f32::min);
            let step = front - near(-12);
            if step > 0.02 && front - under >= (0.3 * step).max(0.02) {
                sliver = true;
                thickness = 5.0;
                lift = 3.5;
                let behind = scale((-13..=-10).fold([0.0; 3], |sum, r| add(sum, shown.colour(x, y + r))), 0.25);
                let rows: Vec<i32> = (-8..=12).collect();
                let unlike: Vec<f32> = rows.iter().map(|r| length(sub(shown.colour(x, y + r), behind))).collect();
                // The sliver is what stands out most from what is over it; where nothing does, it is taken for a few
                // points thick.
                let peak = unlike[0..=12].iter().copied().fold(f32::MIN, f32::max);
                if peak >= 10.0
                    && let Some(at) = unlike[0..=12].iter().position(|value| *value == peak)
                {
                    let (mut first, mut last) = (at, at);
                    while first > 0 && unlike[first - 1] > peak / 2.0 {
                        first -= 1;
                    }
                    while last < unlike.len() - 1 && unlike[last + 1] > peak / 2.0 {
                        last += 1;
                    }
                    thickness = (last - first + 1) as f32;
                    lift = (-rows[first] as f32).max(-2.0);
                }
            }
        }
        slivers.push(sliver);
        thick.push(thickness);
        lifts.push(lift);
    }
    // The depth map misses stretches of a branch and sees a stray sliver in a ridge. A thing that is a sliver along
    // most of its length is one all along, and one that hardly ever is, isn't.
    let found = slivers.clone();
    let share = found.iter().filter(|sliver| **sliver).count() as f32 / count.max(1) as f32;
    if share < 0.15 {
        return (vec![1.0; count], vec![0.0; count]);
    } else if share >= 0.5 {
        let known = sorted(&found.iter().zip(&thick).filter(|(sliver, _)| **sliver).map(|(_, t)| *t).collect::<Vec<_>>());
        for k in 0..count {
            if !found[k] {
                slivers[k] = true;
                thick[k] = known[known.len() / 2];
            }
        }
    }
    let room = evened(&slivers.iter().zip(&thick).map(|(sliver, t)| if *sliver { (t / full).clamp(0.12, 1.0) } else { 1.0 }).collect::<Vec<_>>());
    // Only the columns where the sliver showed say where its top is.
    let mut known = lifts.clone();
    for k in 0..count {
        if found[k] {
            continue;
        }
        let farthest = if slivers[k] { count } else { 8 };
        known[k] = (1..=farthest)
            .find_map(|d| {
                if k >= d && found[k - d] {
                    Some(lifts[k - d])
                } else if k + d < count && found[k + d] {
                    Some(lifts[k + d])
                } else {
                    None
                }
            })
            .unwrap_or(0.0);
    }
    let lift = evened(&known).iter().zip(&room).map(|(lift, room)| lift * (1.0 - smoothstep(0.25, 0.9, *room))).collect();
    (room, lift)
}

/// The middle of what the columns around say, then evened out: a branch stays thin where it crosses a trunk, and a
/// ridge doesn't change for one stray column.
pub fn evened(values: &[f32]) -> Vec<f32> {
    let count = values.len();
    if count == 0 {
        return Vec::new();
    }
    let middle: Vec<f32> = (0..count)
        .map(|k| {
            let around = sorted(&values[k.saturating_sub(8)..=(k + 8).min(count - 1)]);
            around[around.len() / 2]
        })
        .collect();
    (0..count)
        .map(|k| {
            let (low, high) = (k.saturating_sub(3), (k + 3).min(count - 1));
            middle[low..=high].iter().sum::<f32>() / (high - low + 1) as f32
        })
        .collect()
}

/// Whether each column of a ridge is a mountain's, 0 to 1. A mountain goes on down: sixty points under its ridge the
/// sky that is over it has not shown again. Under a branch it has, however thick the depth map takes the branch for.
/// Where no edge shows at all, or the rock under a ridge is as dark as the night over it, it is taken for no mountain.
/// A ridge that is a mountain's along most of its length is one all along, and one that hardly ever is isn't.
pub fn mass(ridge: &Ridge, shown: &Shown, column_width: f32) -> Vec<f32> {
    let found = evened(
        &ridge
            .tops
            .iter()
            .enumerate()
            .map(|(k, top)| {
                let x = (ridge.left + (k as f32 + 0.5) * column_width) as i32;
                let y = shown.height as i32 - *top as i32;
                if x < 0 || x >= shown.width as i32 || y < 16 || y + 64 >= shown.height as i32 {
                    return 0.0;
                }
                let behind = scale((-13..=-10).fold([0.0; 3], |sum, r| add(sum, shown.colour(x, y + r))), 0.25);
                let thing = scale((3..=6).fold([0.0; 3], |sum, r| add(sum, shown.colour(x, y + r))), 0.25);
                let edge = length(sub(thing, behind));
                if edge < 12.0 {
                    return 0.0;
                }
                if (8..=60).step_by(2).any(|r| length(sub(shown.colour(x, y + r), behind)) < edge * 0.35) {
                    return 0.0;
                }
                1.0
            })
            .collect::<Vec<_>>(),
    );
    let share = found.iter().sum::<f32>() / found.len().max(1) as f32;
    if share >= 0.5 {
        vec![1.0; found.len()]
    } else if share < 0.25 {
        vec![0.0; found.len()]
    } else {
        found
    }
}

/// How near each spot of the picture is, for the flakes: a cell to every four points, each the mean of the finer
/// depth's four.
pub fn terrain(depth: &Grid) -> Terrain {
    let (width, height) = (depth.width / 2, depth.height / 2);
    let mut near = vec![0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let i = 2 * y * depth.width + 2 * x;
            let v = &depth.values;
            near[y * width + x] = (v[i] + v[i + 1] + v[i + depth.width] + v[i + depth.width + 1]) / 4.0;
        }
    }
    let things = sorted(&near.iter().copied().filter(|near| *near > SKY).collect::<Vec<_>>());
    let far = if things.is_empty() { SKY } else { things[((things.len() - 1) as f32 * 0.02) as usize] };
    let close = if things.is_empty() { 1.0 } else { things[((things.len() - 1) as f32 * 0.98) as usize] };
    Terrain { width, height, cell: 4, near: near.iter().map(|near| (near.clamp(0.0, 1.0) * 255.0) as u8).collect(), far, close }
}

/// How well each cell of the depth holds snow, 0 to 255. A surface facing up recedes as it rises on the screen, so its
/// depth grows nearer going down; a cliff facing the viewer stays as near all the way down. How fast the depth grows for
/// level ground differs from picture to picture, with the lens and with how the network read it: so each picture is
/// measured against its own flattest ground. Where the depth jumps at the top of something, the top holds snow up to
/// its edge and the sky over it doesn't; above a ridge's own edge, nothing lies.
#[allow(clippy::too_many_arguments)]
pub fn hold(
    depth: &Grid,
    lightness: &[u8],
    shown: &Shown,
    lighting: &Light,
    cell: f32,
    screen_height: f32,
    ridges: &[Ridge],
    column_width: f32,
) -> Vec<u8> {
    let (width, height) = (depth.width, depth.height);
    let smooth = depth.blurred(1).values;
    let fine = &depth.values;
    // Slopes over four points each way, as much as a cell of the coarser depth they were first tuned on.
    let reach = ((4.0 / cell).round() as usize).max(1);
    let mut steepness = vec![-1f32; width * height];
    let mut sides = vec![0f32; width * height];
    let mut falls = vec![0f32; width * height];
    let mut level = Vec::new();
    for y in (2 * reach)..height.saturating_sub(2 * reach) {
        for x in reach..width.saturating_sub(reach) {
            let i = y * width + x;
            let near = smooth[i];
            if near <= GROUND {
                continue;
            }
            let down = (smooth[i + reach * width] - smooth[i - reach * width]) / 2.0;
            let across = (smooth[i + reach] - smooth[i - reach]) / 2.0;
            let (above, below) = (smooth[i - 2 * reach * width], smooth[i + 2 * reach * width]);
            // Up in a jump in depth, nearer the far side than the near: the sky over the top of something.
            if below - above > 2.0 * JUMP.0 && near - above < (below - above) / 2.0 {
                continue;
            }
            // The depth map squeezes the distance: far off, the same slope changes its depth less.
            steepness[i] = down.max(0.0) / (near + 0.15);
            falls[i] = down;
            // A surface that lies level runs nearer straight down the screen. One whose depth runs across more than
            // down leans over sideways as steeply: the side of a fir's crown against the wood behind, the flank of a
            // crag. However slight the step in depth, that is no place to lie.
            sides[i] = smoothstep(JUMP.0, JUMP.1, across.abs()).max(smoothstep(1.0, 2.2, across.abs() / down.max(1e-5)));
            if down > 0.0 && down.abs() < JUMP.0 && across.abs() < JUMP.0 && (x + y) % 5 == 0 {
                level.push(steepness[i]);
            }
        }
    }
    let level = sorted(&level);
    let flattest = if level.is_empty() { RISE.1 } else { level[((level.len() - 1) as f32 * 0.85) as usize].max(1e-5) };
    // Ground runs nearer all the way down it. In a tangle of branches every twig is nearer at its top and farther
    // again under it: slopes that face up and down by turns within a few points are no ground to lie on.
    let around = ((24.0 / cell).round() as usize).max(1);
    let net = Grid::new(width, height, falls.clone()).blurred(around).values;
    let gross = Grid::new(width, height, falls.iter().map(|fall| fall.abs()).collect()).blurred(around).values;
    let mut hold = vec![0f32; width * height];
    for i in 0..hold.len() {
        if steepness[i] >= 0.0 {
            let lasting = smoothstep(0.15, 0.5, net[i] / gross[i].max(1e-6));
            hold[i] = smoothstep(0.3 * flattest, 1.3 * flattest, steepness[i]) * (1.0 - sides[i]) * lasting;
        }
    }
    // The top of anything nearer than what is over it, a crown, a rock, a far ridge, holds snow along its edge.
    let band = ((6.0 / cell).round() as usize).max(1);
    for y in 1..height.saturating_sub(1 + band) {
        for x in 0..width {
            let i = y * width + x;
            let step = fine[i + width] - fine[i - width];
            if step <= 0.04 || fine[i + width] <= GROUND {
                continue;
            }
            let top = 0.85 * smoothstep(0.04, 0.1, step);
            for k in 1..=band {
                hold[i + k * width] = hold[i + k * width].max(top);
            }
        }
    }
    // A ridge's own edge is sharper than the depth map's: its top holds snow right up to it, and nothing lies over it.
    let columns = |body: &mut dyn FnMut(usize, f32, f32)| {
        for ridge in ridges {
            for (k, top) in ridge.tops.iter().enumerate() {
                let edge = screen_height - top - 3.5;
                if edge <= 0.0 {
                    continue;
                }
                let from = ((ridge.left + k as f32 * column_width) / cell) as i32;
                let to = ((ridge.left + (k + 1) as f32 * column_width) / cell) as i32;
                let room = ridge.room.get(k).copied().unwrap_or(1.0);
                for x in from.max(0)..to.max(from + 1).min(width as i32) {
                    body(x as usize, edge, room);
                }
            }
        }
    };
    let rows = |from: f32, to: f32| ((from / cell) as i32).max(0) as usize..((to / cell) as i32).min(height as i32).max(0) as usize;
    // Under a thing no thicker than a branch there is only what is behind it, which holds none of its snow.
    columns(&mut |x, edge, room| {
        let under = 3.5 + 12.5 * smoothstep(0.25, 0.9, room);
        for y in rows(edge, edge + under) {
            hold[y * width + x] = hold[y * width + x].max(0.8);
        }
    });
    // Softened over a few points, the edges of what holds snow follow the land rather than the cells.
    let mut hold = Grid::new(width, height, hold).blurred(((3.0 / cell).round() as usize).max(1)).values;
    for i in 0..hold.len() {
        if fine[i] <= GROUND {
            hold[i] = 0.0;
        }
    }
    // What shows no grain has no surface to see snow on: cloud and mist, what is out of focus, snow lying smooth
    // already.
    let light: Vec<f32> = lightness.iter().map(|value| *value as f32 / 255.0).collect();
    let lit = Grid::new(width, height, light.clone());
    let (even, about) = (lit.blurred(2).values, lit.blurred(8).values);
    let rough = Grid::new(width, height, light.iter().zip(&even).map(|(a, b)| (a - b).abs()).collect()).blurred(8).values;
    // Where rock ends over a bank of cloud the cloud is the nearer, and its upper edge would pass for the top of a
    // thing: so the grain is asked for a little further down too, in what the snow would lie on.
    let under = ((16.0 / cell).round() as usize).max(1);
    for i in 0..hold.len() {
        if hold[i] <= 0.0 {
            continue;
        }
        let below = (i + under * width).min(hold.len() - 1);
        let grained = (rough[i] / (about[i] + 0.1)).min(rough[below] / (about[below] + 0.1));
        hold[i] *= smoothstep(GRAIN.0, GRAIN.1, grained);
        // On what is as white already little more shows, and what did would be a patch a shade off.
        if hold[i] > 0.0 {
            let (x, y) = ((i % width) as i32 * cell as i32, (i / width) as i32 * cell as i32);
            hold[i] *= 1.0 - 0.85 * white(x, y, shown, lighting);
        }
    }
    columns(&mut |x, edge, room| {
        for y in rows(edge - 28.0, edge) {
            hold[y * width + x] = 0.0;
        }
        // A branch carries its snow on top, as its cap: what the depth map takes for a ledge under it is air.
        let thick = smoothstep(0.25, 0.9, room);
        if thick < 1.0 {
            for y in rows(edge, edge + 20.0) {
                hold[y * width + x] *= thick;
            }
        }
    });
    hold.iter().map(|hold| (hold.clamp(0.0, 1.0) * 255.0) as u8).collect()
}

/// How light the picture is, a cell to every `cell` points, 0 to 255.
pub fn lightness(shown: &Shown, cell: usize) -> Vec<u8> {
    let (width, height) = (shown.width / cell, shown.height / cell);
    let mut pixels = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for dy in 0..cell {
                for dx in 0..cell {
                    sum += luminance(shown.rgb(((y * cell + dy) * shown.width + x * cell + dx) * 4));
                }
            }
            pixels[y * width + x] = (sum / (cell * cell) as f32).min(255.0) as u8;
        }
    }
    pixels
}

/// The tops of darker things: lighter just over them than on them. Only those that run along a little, as a branch or
/// a ledge does, not a speck of the picture's grain, and softened, so the snow on them has no cells' edges.
pub fn tops(lightness: &[u8], width: usize) -> Vec<u8> {
    let height = lightness.len() / width;
    // Measured against how light it is around: in a dark wood the lit top of a bough is only a little lighter than the
    // shade under it, and is as much a top as a rock against the sky.
    let around = Grid::new(width, height, lightness.iter().map(|value| *value as f32 / 255.0).collect()).blurred(6).values;
    let mut tops = vec![0f32; width * height];
    for y in 2..height {
        for x in 0..width {
            let here = lightness[y * width + x] as f32 / 255.0;
            let over = lightness[(y - 1) * width + x].max(lightness[(y - 2) * width + x]) as f32 / 255.0;
            tops[y * width + x] = smoothstep(0.12, 0.5, (over - here) / (around[y * width + x] + 0.1));
        }
    }
    let mut along = tops.clone();
    for y in 0..height {
        for x in 1..width.saturating_sub(1) {
            let i = y * width + x;
            along[i] = tops[i - 1].min(tops[i]).min(tops[i + 1]);
        }
    }
    let mut soft = along.clone();
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let mut sum = 0.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let weight = if dx == 0 { 2.0 } else { 1.0 } * if dy == 0 { 2.0 } else { 1.0 };
                    sum += along[(y as i32 + dy) as usize * width + (x as i32 + dx) as usize] * weight;
                }
            }
            soft[y * width + x] = sum / 16.0;
        }
    }
    soft.iter().map(|value| (value.min(1.0) * 255.0) as u8).collect()
}
