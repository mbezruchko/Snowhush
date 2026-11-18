//! The wallpaper's geometry beyond what one look of the depth network gives: finer depth from looking at it part by
//! part, and its edges moved onto the picture's own.

use super::{GROUND, SKY, Shown};

/// The depth network, as the platform runs it.
pub trait Network {
    /// How many values across and down a look gives.
    fn size(&self) -> (usize, usize);
    /// The depth of a part of the picture, `x`, `y`, `width` and `height` in its pixels, stretched to the network's
    /// size: the larger, the nearer, on a scale of its own each time. Row by row from the top.
    fn depth(&mut self, shown: &Shown, part: [usize; 4]) -> Option<Vec<f32>>;
}

/// A grid of values over the screen, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

impl Grid {
    pub fn new(width: usize, height: usize, values: Vec<f32>) -> Grid {
        Grid { width, height, values }
    }

    pub fn from_fn(width: usize, height: usize, value: impl Fn(usize, usize) -> f32) -> Grid {
        let mut values = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                values.push(value(x, y));
            }
        }
        Grid { width, height, values }
    }

    /// Bilinear, at a point given in cells from the top left, with cell centres at halves.
    pub fn value_at(&self, x: f32, y: f32) -> f32 {
        let u = (x - 0.5).max(0.0).min((self.width - 1) as f32);
        let v = (y - 0.5).max(0.0).min((self.height - 1) as f32);
        self.blend(u, v)
    }

    /// Bilinear, in the screen's points from its top left, for a grid spread over a screen `screen` points across.
    pub fn value_on(&self, x: f32, y: f32, screen: (f32, f32)) -> f32 {
        let u = (x / screen.0 * self.width as f32 - 0.5).max(0.0).min((self.width - 1) as f32);
        let v = (y / screen.1 * self.height as f32 - 0.5).max(0.0).min((self.height - 1) as f32);
        self.blend(u, v)
    }

    fn blend(&self, u: f32, v: f32) -> f32 {
        let (x0, y0) = (u as usize, v as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (u - x0 as f32, v - y0 as f32);
        let w = self.width;
        let top = self.values[y0 * w + x0] * (1.0 - fx) + self.values[y0 * w + x1] * fx;
        let bottom = self.values[y1 * w + x0] * (1.0 - fx) + self.values[y1 * w + x1] * fx;
        top * (1.0 - fy) + bottom * fy
    }

    /// The mean over a square `2 * radius + 1` cells across around each cell, by running sums.
    pub fn blurred(&self, radius: usize) -> Grid {
        if radius == 0 {
            return self.clone();
        }
        let (width, height) = (self.width, self.height);
        let mut across = vec![0f32; self.values.len()];
        for y in 0..height {
            let row = y * width;
            let (mut sum, mut count) = (0f32, 0i32);
            for x in 0..radius.min(width) {
                sum += self.values[row + x];
                count += 1;
            }
            for x in 0..width {
                if x + radius < width {
                    sum += self.values[row + x + radius];
                    count += 1;
                }
                if x > radius {
                    sum -= self.values[row + x - radius - 1];
                    count -= 1;
                }
                across[row + x] = sum / count as f32;
            }
        }
        let mut down = vec![0f32; self.values.len()];
        let mut sums = vec![0f32; width];
        let mut count = 0i32;
        for y in 0..radius.min(height) {
            for x in 0..width {
                sums[x] += across[y * width + x];
            }
            count += 1;
        }
        for y in 0..height {
            if y + radius < height {
                let row = (y + radius) * width;
                for x in 0..width {
                    sums[x] += across[row + x];
                }
                count += 1;
            }
            if y > radius {
                let row = (y - radius - 1) * width;
                for x in 0..width {
                    sums[x] -= across[row + x];
                }
                count -= 1;
            }
            let scale = 1.0 / count as f32;
            let row = y * width;
            for x in 0..width {
                down[row + x] = sums[x] * scale;
            }
        }
        Grid { width, height, values: down }
    }
}

/// The network's look at the whole screen, and finer depth made of it and of looks at the screen part by part: a cell to
/// every two points. Each part is seen in more detail than the whole but on a scale of its own, so it is fitted to the
/// whole and lends it only its detail; in the sky, where there is none to lend, it lends nothing.
pub struct Depths {
    /// The network's own look, its range stretched to 0 to 1.
    pub whole: Grid,
    pub fine: Grid,
}

pub fn depths(shown: &Shown, network: &mut dyn Network, parts: (usize, usize)) -> Option<Depths> {
    let (network_width, network_height) = network.size();
    let raw = network.depth(shown, [0, 0, shown.width, shown.height])?;
    let low = raw.iter().copied().fold(f32::INFINITY, f32::min);
    let high = raw.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let range = (high - low).max(1e-6);
    let whole = Grid::new(network_width, network_height, raw.iter().map(|value| (value - low) / range).collect());
    let cell = 2.0;
    let (width, height) = (shown.width / cell as usize, shown.height / cell as usize);
    // The whole, spread over the fine grid, and its smooth part: what is left of it once the detail of about the size
    // the parts see better is taken out.
    let whole_across = network_width as f32 / shown.width as f32;
    let whole_down = network_height as f32 / shown.height as f32;
    let spread = Grid::from_fn(width, height, |x, y| whole.value_at((x as f32 + 0.5) * cell * whole_across, (y as f32 + 0.5) * cell * whole_down));
    let radius = 6;
    let smooth = spread.blurred(radius);
    let mut detail = vec![0f32; width * height];
    let mut weight = vec![0f32; width * height];
    // Parts overlapping by a quarter.
    let part_width = shown.width as f32 / (parts.0 as f32 - 0.25 * (parts.0 as f32 - 1.0));
    let part_height = shown.height as f32 / (parts.1 as f32 - 0.25 * (parts.1 as f32 - 1.0));
    for row in 0..parts.1 {
        for column in 0..parts.0 {
            let left = (column as f32 * part_width * 0.75).round();
            let top = (row as f32 * part_height * 0.75).round();
            let part = [left, top, part_width.round().min(shown.width as f32 - left), part_height.round().min(shown.height as f32 - top)];
            let Some(seen) = network.depth(shown, part.map(|value| value as usize)) else { continue };
            // Fitted to the whole by least squares: seen * a + b.
            let (mut s, mut w, mut ss, mut sw) = (0f64, 0f64, 0f64, 0f64);
            let across = part[2] / network_width as f32;
            let down = part[3] / network_height as f32;
            for v in 0..network_height {
                for u in 0..network_width {
                    let x = left + (u as f32 + 0.5) * across;
                    let y = top + (v as f32 + 0.5) * down;
                    let matching = whole.value_at(x * whole_across, y * whole_down) as f64;
                    let seen = seen[v * network_width + u] as f64;
                    s += seen;
                    w += matching;
                    ss += seen * seen;
                    sw += seen * matching;
                }
            }
            let n = seen.len() as f64;
            let spread_of_seen = ss - s * s / n;
            if spread_of_seen <= 1e-9 {
                continue;
            }
            let a = ((sw - s * w / n) / spread_of_seen) as f32;
            let b = ((w - a as f64 * s) / n) as f32;
            // A part that goes the other way from the whole has misread what it saw.
            if a <= 0.0 {
                continue;
            }
            let fitted = Grid::new(network_width, network_height, seen.iter().map(|value| value * a + b).collect());
            // The same size of detail in the part's own cells.
            let own = fitted.blurred(((radius as f32 * cell / across).round() as usize).max(1));
            let (x0, x1) = ((left / cell) as usize, (((left + part[2]) / cell) as usize).min(width));
            let (y0, y1) = ((top / cell) as usize, (((top + part[3]) / cell) as usize).min(height));
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = ((x as f32 + 0.5) * cell - left) / across;
                    let py = ((y as f32 + 0.5) * cell - top) / down;
                    // Weighs most at the part's middle, fading to its edges, so parts meet without a seam.
                    let fx = px.min(network_width as f32 - px) / (network_width as f32 * 0.25);
                    let fy = py.min(network_height as f32 - py) / (network_height as f32 * 0.25);
                    let w = fx.min(fy).clamp(0.001, 1.0);
                    detail[y * width + x] += w * (fitted.value_at(px, py) - own.value_at(px, py));
                    weight[y * width + x] += w;
                }
            }
        }
    }
    let fine = (0..width * height)
        .map(|i| {
            let (smooth, spread) = (smooth.values[i], spread.values[i]);
            let lent = if weight[i] > 0.0 { detail[i] / weight[i] } else { spread - smooth };
            // Nothing in the sky has detail worth lending.
            let ground = ((smooth - SKY) / (GROUND - SKY)).clamp(0.0, 1.0);
            (smooth + lent * ground + (spread - smooth) * (1.0 - ground)).clamp(0.0, 1.0)
        })
        .collect();
    Some(Depths { whole, fine: Grid::new(width, height, fine) })
}

/// Moves the edges of a depth grid onto the edges of the picture: the guided filter, with the picture's lightness as
/// the guide. Where the picture has an edge near a step in depth, the step takes its place and shape. Where the picture
/// is even, a dark slope against a dark sky, the filter would only blur the depth, so there it stays as it was.
pub fn snapped(depth: &Grid, lightness: &[u8], radius: usize, give: f32) -> Grid {
    if lightness.len() != depth.values.len() {
        return depth.clone();
    }
    let (width, height) = (depth.width, depth.height);
    let guide = Grid::new(width, height, lightness.iter().map(|value| *value as f32 / 255.0).collect());
    let product = |a: &Grid, b: &Grid| Grid::new(width, height, a.values.iter().zip(&b.values).map(|(a, b)| a * b).collect());
    let mean_guide = guide.blurred(radius);
    let mean_depth = depth.blurred(radius);
    let mean_square = product(&guide, &guide).blurred(radius);
    let mean_both = product(&guide, depth).blurred(radius);
    let count = depth.values.len();
    let (mut slope, mut offset, mut edged) = (vec![0f32; count], vec![0f32; count], vec![0f32; count]);
    for i in 0..count {
        let variance = (mean_square.values[i] - mean_guide.values[i] * mean_guide.values[i]).max(0.0);
        let together = mean_both.values[i] - mean_guide.values[i] * mean_depth.values[i];
        slope[i] = together / (variance + give);
        offset[i] = mean_depth.values[i] - slope[i] * mean_guide.values[i];
        edged[i] = variance / (variance + 4.0 * give);
    }
    let mean_slope = Grid::new(width, height, slope).blurred(radius);
    let mean_offset = Grid::new(width, height, offset).blurred(radius);
    let mean_edged = Grid::new(width, height, edged).blurred(radius);
    let values = (0..count)
        .map(|i| {
            let moved = mean_slope.values[i] * guide.values[i] + mean_offset.values[i];
            (depth.values[i] + (moved - depth.values[i]) * mean_edged.values[i]).clamp(0.0, 1.0)
        })
        .collect();
    Grid::new(width, height, values)
}
