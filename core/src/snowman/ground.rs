//! Where a snowman can walk in the wallpaper's picture: the stretches of level ground found in it, as the Mac's
//! SnowmanGround has them, or a strip along the bottom of the screen for a picture without any.

use std::collections::BinaryHeap;

use crate::geometry::V2;
use crate::wallpaper::{GROUND, Light, Scene};

/// How many points across a cell of the ground is.
const CELL_POINTS: usize = 8;

#[derive(Clone, Debug)]
pub struct Ground {
    pub width: usize,
    pub height: usize,
    /// How many points across a cell is.
    pub cell: f32,
    /// The screen's size in points.
    pub screen: V2,
    /// How near each cell is, on the picture's own scale, top row first; below zero where it can't be walked.
    pub near: Vec<f32>,
    /// Which stretch of ground each cell is in: one can't be walked to from another.
    group: Vec<i32>,
    pub far: f32,
    pub close: f32,
    pub light: Light,
    /// A strip along the bottom of the screen for a wallpaper with no ground of its own: what stands on it is in front of
    /// the whole picture, neither hidden by it nor paled by its distance.
    pub in_front: bool,
}

impl Ground {
    /// For a wallpaper with no ground to walk: a strip along the bottom of the screen, above the panel `bottom` points up,
    /// as if on the snow drift there, as deep as the middle of a picture.
    pub fn strip(scene: &Scene, screen: V2, bottom: f32) -> Ground {
        let cell = CELL_POINTS as f32;
        let (width, height) = (((screen.x / cell) as usize).max(1), ((screen.y / cell) as usize).max(1));
        let mut near = vec![-1.0; width * height];
        let (low, high) = (bottom + 12.0, bottom + 60.0);
        for cy in 0..height {
            let up = screen.y - (cy as f32 + 0.5) * cell;
            if up < low || up > high {
                continue;
            }
            for cx in 0..width {
                let across = (cx as f32 + 0.5) * cell;
                // Clear of the New Year's tree in the corner.
                if across <= screen.x * 0.18 || across >= screen.x * 0.94 {
                    continue;
                }
                near[cy * width + cx] = 0.96 - 0.06 * (up - low) / (high - low);
            }
        }
        let group = near.iter().map(|near| if *near >= 0.0 { 0 } else { -1 }).collect();
        Ground { width, height, cell, screen, near, group, far: 0.6, close: 1.2, light: scene.light.clone(), in_front: true }
    }

    /// The level ground in the picture, in stretches big enough to walk about in; none if there are none.
    pub fn new(scene: &Scene, screen: V2) -> Option<Ground> {
        let relief = &scene.relief;
        let step = (CELL_POINTS / relief.cell.max(1)).max(1);
        let (width, height) = (relief.width / step, relief.height / step);
        let count = relief.width * relief.height;
        if width <= 4 || height <= 4 || relief.hold.len() < count || relief.near.len() < count {
            return None;
        }
        let cell = (step * relief.cell) as f32;
        let (far, close) = (scene.terrain.far, scene.terrain.close);
        let mut near = vec![-1.0f32; width * height];
        for cy in 0..height {
            let top = cy as f32 * cell;
            // Not in the sky's part of the screen, nor under the panel.
            if top <= screen.y * 0.1 || top + cell >= screen.y - 70.0 {
                continue;
            }
            for cx in 0..width {
                let left = cx as f32 * cell;
                if left <= 20.0 || left + cell >= screen.x - 20.0 {
                    continue;
                }
                let (mut hold, mut lowest, mut highest, mut sum) = (0usize, 255u8, 0u8, 0usize);
                for y in (cy * step)..(cy * step + step) {
                    for x in (cx * step)..(cx * step + step) {
                        let i = y * relief.width + x;
                        hold += relief.hold[i] as usize;
                        let n = relief.near[i];
                        lowest = lowest.min(n);
                        highest = highest.max(n);
                        sum += n as usize;
                    }
                }
                let cells = (step * step) as f32;
                let average = sum as f32 / cells / 255.0;
                let nearness = (average - far) / (close - far).max(0.01);
                if hold as f32 / cells / 255.0 >= 0.2
                    && lowest as f32 / 255.0 > GROUND + 0.03
                    && (highest - lowest) as f32 / 255.0 < 0.06
                    && nearness > 0.04
                    && nearness < 0.97
                {
                    near[cy * width + cx] = average;
                }
            }
        }
        // Only stretches big enough to walk about in.
        let mut group = vec![-1i32; width * height];
        let mut sizes: Vec<usize> = Vec::new();
        for start in 0..near.len() {
            if near[start] < 0.0 || group[start] >= 0 {
                continue;
            }
            let id = sizes.len() as i32;
            let (mut stack, mut count) = (vec![start], 0);
            group[start] = id;
            while let Some(i) = stack.pop() {
                count += 1;
                let (x, y) = ((i % width) as i32, (i / width) as i32);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if (dx == 0 && dy == 0) || nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                            continue;
                        }
                        let j = ny as usize * width + nx as usize;
                        if group[j] >= 0 || near[j] < 0.0 || (near[j] - near[i]).abs() > rise(dx, dy) {
                            continue;
                        }
                        group[j] = id;
                        stack.push(j);
                    }
                }
            }
            sizes.push(count);
        }
        if !sizes.iter().any(|size| *size >= 60) {
            return None;
        }
        for i in 0..near.len() {
            if group[i] < 0 || sizes[group[i] as usize] < 60 {
                near[i] = -1.0;
                group[i] = -1;
            }
        }
        Some(Ground { width, height, cell, screen, near, group, far, close, light: scene.light.clone(), in_front: false })
    }

    /// How near `near` is between the farthest and the nearest of the picture, 0 to 1.
    pub fn nearness(&self, near: f32) -> f32 {
        ((near - self.far) / (self.close - self.far).max(0.01)).clamp(0.0, 1.0)
    }

    /// How tall a snowman stands, in points, as near as `near`: the farther off, the smaller.
    pub fn height(&self, near: f32) -> f32 {
        (16.0 + 93.0 * self.nearness(near).powf(1.35)) * self.screen.y / 1440.0
    }

    pub fn walkable(&self, cell: (i32, i32)) -> bool {
        cell.0 >= 0
            && cell.0 < self.width as i32
            && cell.1 >= 0
            && cell.1 < self.height as i32
            && self.near[cell.1 as usize * self.width + cell.0 as usize] >= 0.0
    }

    /// The cell under a point, in the screen's points from its bottom left.
    pub fn cell_at(&self, point: V2) -> (i32, i32) {
        ((point.x / self.cell) as i32, ((self.screen.y - point.y) / self.cell) as i32)
    }

    /// The middle of a cell, in the screen's points from its bottom left.
    pub fn point_of(&self, cell: (i32, i32)) -> V2 {
        V2::new((cell.0 as f32 + 0.5) * self.cell, self.screen.y - (cell.1 as f32 + 0.5) * self.cell)
    }

    /// How near the ground is at a point, between the cells about it that can be walked.
    pub fn near_at(&self, point: V2) -> f32 {
        let (fx, fy) = (point.x / self.cell - 0.5, (self.screen.y - point.y) / self.cell - 0.5);
        let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
        let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
        let (mut sum, mut weight) = (0.0, 0.0);
        for (dx, dy, w) in [(0, 0, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] {
            let c = (x0 + dx, y0 + dy);
            if !self.walkable(c) || w <= 0.0 {
                continue;
            }
            sum += self.near[c.1 as usize * self.width + c.0 as usize] * w;
            weight += w;
        }
        if weight > 0.0 {
            return sum / weight;
        }
        let c = self.cell_at(point);
        if self.walkable(c) { self.near[c.1 as usize * self.width + c.0 as usize] } else { self.far }
    }

    /// The light snow takes on at a point, 0 to 1 a channel.
    pub fn tint_at(&self, point: V2) -> [f32; 3] {
        let light = &self.light;
        if light.width == 0 || light.height == 0 || light.pixels.len() < light.width * light.height * 4 {
            return [1.0; 3];
        }
        let x = ((point.x as i32) / light.cell.max(1) as i32).clamp(0, light.width as i32 - 1) as usize;
        let y = (((self.screen.y - point.y) as i32) / light.cell.max(1) as i32).clamp(0, light.height as i32 - 1) as usize;
        let i = (y * light.width + x) * 4;
        [light.pixels[i] as f32 / 255.0, light.pixels[i + 1] as f32 / 255.0, light.pixels[i + 2] as f32 / 255.0]
    }

    /// A cell to stand on, picked with `pick` among those neither far off nor close if `middling`, and in the same
    /// stretch as `with` if given.
    pub fn anywhere(&self, pick: &mut impl FnMut(usize) -> usize, middling: bool, with: Option<(i32, i32)>) -> Option<(i32, i32)> {
        let same = with.filter(|cell| self.walkable(*cell)).map(|cell| self.group[cell.1 as usize * self.width + cell.0 as usize]);
        let fits = |i: usize, middling: bool| {
            let near = self.near[i];
            let nearness = self.nearness(near);
            near >= 0.0 && same.is_none_or(|group| self.group[i] == group) && (!middling || (nearness > 0.2 && nearness < 0.75))
        };
        let mut cells: Vec<usize> = (0..self.near.len()).filter(|i| fits(*i, middling)).collect();
        if cells.is_empty() {
            cells = (0..self.near.len()).filter(|i| fits(*i, false)).collect();
        }
        if cells.is_empty() {
            return None;
        }
        let i = cells[pick(cells.len()) % cells.len()];
        Some(((i % self.width) as i32, (i / self.width) as i32))
    }

    /// The way along the ground from one cell to another, its cells' middles in the screen's points; empty if there is
    /// none.
    pub fn path(&self, start: (i32, i32), goal: (i32, i32)) -> Vec<V2> {
        if !self.walkable(start) || !self.walkable(goal) || start == goal {
            return Vec::new();
        }
        let width = self.width;
        let count = width * self.height;
        let mut cost = vec![f32::INFINITY; count];
        let mut came = vec![-1i32; count];
        let estimate = |i: usize| {
            let (dx, dy) = ((i % width) as f32 - goal.0 as f32, (i / width) as f32 - goal.1 as f32);
            (dx * dx + dy * dy).sqrt()
        };
        let first = start.1 as usize * width + start.0 as usize;
        let last = goal.1 as usize * width + goal.0 as usize;
        cost[first] = 0.0;
        let mut open = BinaryHeap::new();
        open.push(Open(estimate(first), first));
        let mut visited = 0;
        while let Some(Open(_, i)) = open.pop() {
            if i == last {
                break;
            }
            visited += 1;
            if visited > 6000 {
                return Vec::new();
            }
            let (x, y) = ((i % width) as i32, (i / width) as i32);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let c = (x + dx, y + dy);
                    if (dx == 0 && dy == 0) || !self.walkable(c) {
                        continue;
                    }
                    let j = c.1 as usize * width + c.0 as usize;
                    if (self.near[j] - self.near[i]).abs() > rise(dx, dy) {
                        continue;
                    }
                    let through = cost[i] + if dx != 0 && dy != 0 { 1.414 } else { 1.0 };
                    if through < cost[j] {
                        cost[j] = through;
                        came[j] = i as i32;
                        open.push(Open(through + estimate(j), j));
                    }
                }
            }
        }
        if came[last] < 0 {
            return Vec::new();
        }
        let mut cells = vec![last];
        let mut i = last;
        while i != first && came[i] >= 0 {
            i = came[i] as usize;
            cells.push(i);
        }
        cells.iter().rev().skip(1).map(|i| self.point_of(((i % width) as i32, (i / width) as i32))).collect()
    }
}

/// How much nearer or farther the next cell may be for a figure to step onto it.
fn rise(dx: i32, dy: i32) -> f32 {
    if dx != 0 && dy != 0 { 0.07 } else { 0.05 }
}

/// A cell still to look at, the most promising first: the heap is a max-heap, so the order is turned around.
struct Open(f32, usize);

impl PartialEq for Open {
    fn eq(&self, other: &Open) -> bool {
        self.0 == other.0
    }
}

impl Eq for Open {}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Open) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Open {
    fn cmp(&self, other: &Open) -> std::cmp::Ordering {
        other.0.total_cmp(&self.0)
    }
}
