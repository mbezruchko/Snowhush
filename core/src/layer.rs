use crate::geometry::V2;

/// A height field of snow columns lying on a bed: the screen bottom, or the top edge of a window
/// whose rounded corners curve the bed down at both ends.
pub struct SnowLayer {
    pub column_width: f32,
    pub opens_at_edges: bool,
    pub origin: V2,
    pub heights: Vec<f32>,
    /// How much of each column's snow a wall has packed down already, so it isn't packed again.
    pub packed: Vec<f32>,
    /// The bed under each column relative to `origin.y`; zero along a flat edge, negative where a corner rounds off.
    pub bed: Vec<f32>,
    pub visible: Vec<bool>,
    pub top: f32,
    pub bed_depth: f32,
    pub occluders: Vec<[f32; 4]>,
    pub moved_at: f64,
    pub spill_left: f32,
    pub spill_right: f32,
    /// Smoothed path of the supporting edge; the snow reacts to its accelerations, not to raw jittery samples.
    pub track: V2,
    pub track_velocity: V2,
    /// The snow body's displacement from its support: x slides along it, y lifts off it.
    pub offset: V2,
    pub offset_velocity: V2,
    pub previous_base_y: f32,
    pub agitation: f32,
    pub swing_direction: V2,
    pub swing_peak: f32,
    pub previous_swing_peak: f32,

    capacity: Vec<f32>,
    depth: f32,
    phases: (f32, f32),
    corner: (f32, f32),
    supported: Vec<bool>,
    supported_range: Option<(usize, usize)>,
    parity: bool,
}

impl SnowLayer {
    pub fn new(origin: V2, columns: usize, column_width: f32, depth: f32, opens_at_edges: bool, phases: (f32, f32)) -> SnowLayer {
        let mut layer = SnowLayer {
            column_width,
            opens_at_edges,
            origin,
            heights: vec![0.0; columns],
            packed: vec![0.0; columns],
            bed: vec![0.0; columns],
            visible: vec![true; columns],
            top: 0.0,
            bed_depth: 0.0,
            occluders: Vec::new(),
            moved_at: -1.0,
            spill_left: 0.0,
            spill_right: 0.0,
            track: origin,
            track_velocity: V2::ZERO,
            offset: V2::ZERO,
            offset_velocity: V2::ZERO,
            previous_base_y: origin.y,
            agitation: 0.0,
            swing_direction: V2::ZERO,
            swing_peak: 0.0,
            previous_swing_peak: 0.0,
            capacity: Vec::new(),
            depth,
            phases,
            corner: (0.0, 2.0),
            supported: vec![false; columns],
            supported_range: None,
            parity: false,
        };
        layer.update_capacity();
        layer
    }

    pub fn count(&self) -> usize {
        self.heights.len()
    }

    pub fn width(&self) -> f32 {
        self.heights.len() as f32 * self.column_width
    }

    pub fn base(&self) -> V2 {
        self.origin + self.offset
    }

    pub fn index(&self, x: f32) -> Option<usize> {
        let i = ((x - self.base().x) / self.column_width).floor();
        (i >= 0.0 && (i as usize) < self.heights.len()).then_some(i as usize)
    }

    pub fn clamped_index(&self, x: f32) -> usize {
        let i = ((x - self.base().x) / self.column_width).floor() as i64;
        i.clamp(0, self.heights.len() as i64 - 1) as usize
    }

    pub fn center_x(&self, i: usize) -> f32 {
        self.base().x + (i as f32 + 0.5) * self.column_width
    }

    /// Height of the snow's underside (the bed) at column `i`, in overlay coordinates.
    pub fn floor(&self, i: usize) -> f32 {
        self.base().y + self.bed[i]
    }

    pub fn surface(&self, i: usize) -> f32 {
        self.base().y + self.bed[i] + self.heights[i]
    }

    /// Rounds the bed's ends like the window's corners: a superellipse quadrant of the given radius.
    pub fn shape_corners(&mut self, radius: f32, exponent: f32) {
        if radius == self.corner.0 && exponent == self.corner.1 {
            return;
        }
        self.corner = (radius, exponent);
        self.update_bed();
    }

    /// Moves the snow one column along its support and returns the column that slid off the end.
    pub fn slide_column(&mut self, direction: i32) -> (f32, f32, f32) {
        self.release_support();
        if direction > 0 {
            let last = self.heights.len() - 1;
            let x = self.center_x(last);
            let y = self.floor(last);
            let dropped = self.heights.pop().unwrap_or(0.0);
            self.heights.insert(0, 0.0);
            self.packed.pop();
            self.packed.insert(0, 0.0);
            return (dropped, x, y);
        }
        let x = self.center_x(0);
        let y = self.floor(0);
        let dropped = self.heights.remove(0);
        self.heights.push(0.0);
        self.packed.remove(0);
        self.packed.push(0.0);
        (dropped, x, y)
    }

    pub fn fill(&self, i: usize) -> f32 {
        (1.0 - self.heights[i] / self.capacity[i]).max(0.0)
    }

    pub fn set(&mut self, i: usize, height: f32) {
        self.heights[i] = height.max(0.0);
        self.packed[i] = self.packed[i].min(self.heights[i]);
    }

    pub fn replace(&mut self, fresh: Vec<f32>) {
        let count = fresh.len();
        self.heights = fresh;
        self.packed = vec![0.0; count];
        self.visible = vec![true; count];
        self.supported = vec![false; count];
        self.supported_range = None;
        self.update_capacity();
        self.update_bed();
    }

    pub fn resize(&mut self, columns: usize) {
        if columns == self.heights.len() {
            return;
        }
        let mut fresh = self.heights.clone();
        fresh.resize(columns, 0.0);
        self.replace(fresh);
    }

    pub fn update_visibility(&mut self, screen_width: f32, blockers: &[[f32; 4]]) {
        for i in 0..self.heights.len() {
            let x = self.center_x(i);
            let y = self.origin.y + self.bed[i] + 1.0;
            self.visible[i] = x >= 0.0 && x <= screen_width && !blockers.iter().any(|b| x >= b[0] && x < b[2] && y >= b[1] && y < b[3]);
        }
    }

    pub fn refresh_top(&mut self) {
        self.top = self.heights.iter().copied().fold(0.0, f32::max);
    }

    /// Adds snow centred on `x` and returns the area that fell outside the layer.
    pub fn deposit(&mut self, area: f32, x: f32, wide: bool) -> f32 {
        if area <= 0.0 {
            return 0.0;
        }
        let center = ((x - self.base().x) / self.column_width).floor() as i64;
        let amount = area / self.column_width;
        let kernel: &[(i64, f32)] =
            if wide { &[(-2, 0.0625), (-1, 0.25), (0, 0.375), (1, 0.25), (2, 0.0625)] } else { &[(-1, 0.25), (0, 0.5), (1, 0.25)] };
        let mut missed = 0.0;
        for &(offset, weight) in kernel {
            let i = center + offset;
            if i >= 0 && (i as usize) < self.heights.len() {
                self.heights[i as usize] += amount * weight;
            } else {
                missed += area * weight;
            }
        }
        missed
    }

    /// Piles snow against a wall just past column `edge`, as a plough heaps it: a wedge falling away from the wall
    /// at `slope` per column, the way `inward` points.
    pub fn heap(&mut self, area: f32, edge: usize, inward: i32, slope: f32) {
        let amount = area / self.column_width;
        if amount <= 0.0 {
            return;
        }
        let mut low = self.bed[edge] + self.heights[edge];
        let mut high = low + amount;
        for _ in 0..20 {
            let middle = (low + high) / 2.0;
            if self.wedge(middle, edge, inward, slope, 0.0) < amount {
                low = middle;
            } else {
                high = middle;
            }
        }
        let total = self.wedge(high, edge, inward, slope, 0.0);
        self.wedge(high, edge, inward, slope, amount / total);
    }

    /// The snow it takes to raise the surface to a line falling away from `level` at `edge`; adds `scale` of it.
    fn wedge(&mut self, level: f32, edge: usize, inward: i32, slope: f32, scale: f32) -> f32 {
        let reach = if inward > 0 { self.heights.len() - edge } else { edge + 1 };
        let mut total = 0.0;
        for k in 0..reach {
            let line = level - slope * k as f32;
            if line <= -self.bed_depth {
                break;
            }
            let i = if inward > 0 { edge + k } else { edge - k };
            let gap = line - (self.bed[i] + self.heights[i]);
            if gap <= 0.0 {
                continue;
            }
            total += gap;
            self.heights[i] += gap * scale;
            self.packed[i] += gap * scale;
        }
        total
    }

    /// Columns touching the cursor are held up by it, so their walls don't slump into the hole until it leaves.
    pub fn hold(&mut self, i: usize) {
        self.supported[i] = true;
        self.supported_range = Some(match self.supported_range {
            Some((low, high)) => (low.min(i), high.max(i)),
            None => (i, i),
        });
    }

    pub fn release_support(&mut self) {
        let Some((low, high)) = self.supported_range.take() else { return };
        for held in &mut self.supported[low..=high] {
            *held = false;
        }
    }

    /// Slumps slopes steeper than the angle of repose and rounds crests, measured on the snow surface over the bed,
    /// so snow slides off rounded corners. Open layers shed what slides past their ends into `spill`.
    pub fn relax(&mut self, repose: f32, rounding: f32) -> bool {
        let count = self.heights.len();
        if count <= 1 || self.top <= 0.0 {
            return false;
        }
        let limit = repose * self.column_width;
        let forward_first = self.parity;
        self.parity = !self.parity;
        let mut moved = false;
        let mut spilled_left = 0.0;
        let mut spilled_right = 0.0;
        let open = self.opens_at_edges;
        let h = &mut self.heights;
        let p = &mut self.packed;
        let b = &self.bed;
        let held = &self.supported;

        for pass in 0..6 {
            let forward = (pass % 2 == 0) == forward_first;
            for k in 0..count - 1 {
                let i = if forward { k } else { count - 2 - k };
                if held[i] || held[i + 1] {
                    continue;
                }
                let difference = (b[i] + h[i]) - (b[i + 1] + h[i + 1]);
                if difference > limit + 0.01 {
                    let transfer = ((difference - limit) * 0.5).min(h[i]);
                    shift(h, p, transfer, i, i + 1);
                    if transfer > 0.0 {
                        moved = true;
                    }
                } else if difference < -limit - 0.01 {
                    let transfer = ((-difference - limit) * 0.5).min(h[i + 1]);
                    shift(h, p, transfer, i + 1, i);
                    if transfer > 0.0 {
                        moved = true;
                    }
                }
            }
            if open {
                if !held[0] && h[0] > limit + 0.01 {
                    let transfer = (h[0] - limit) * 0.5;
                    h[0] -= transfer;
                    p[0] = p[0].min(h[0]);
                    spilled_left += transfer;
                    moved = true;
                }
                if !held[count - 1] && h[count - 1] > limit + 0.01 {
                    let transfer = (h[count - 1] - limit) * 0.5;
                    h[count - 1] -= transfer;
                    p[count - 1] = p[count - 1].min(h[count - 1]);
                    spilled_right += transfer;
                    moved = true;
                }
            }
        }
        // Pulls each slope toward its neighbours' average: rounds crests and notches, leaves even slopes alone.
        for i in 1..count.saturating_sub(2).max(1) {
            if held[i - 1] || held[i] || held[i + 1] || held[i + 2] {
                continue;
            }
            let (s0, s1, s2, s3) = (b[i - 1] + h[i - 1], b[i] + h[i], b[i + 1] + h[i + 1], b[i + 2] + h[i + 2]);
            let kink = (s1 - s2) - 0.5 * ((s0 - s1) + (s2 - s3));
            if kink.abs() <= 0.02 {
                continue;
            }
            let transfer = if kink > 0.0 { (kink * rounding).min(h[i]) } else { (kink * rounding).max(-h[i + 1]) };
            if transfer > 0.0 {
                shift(h, p, transfer, i, i + 1);
            } else {
                shift(h, p, -transfer, i + 1, i);
            }
            if transfer.abs() > 0.003 {
                moved = true;
            }
        }
        self.spill_left += spilled_left * self.column_width;
        self.spill_right += spilled_right * self.column_width;
        moved
    }

    /// Snow down to `floor` melts slowly; anything heaped above it settles back down over `settle` seconds.
    pub fn thaw(&mut self, dt: f32, floor: f32, rate: f32, settle: f32) -> bool {
        if self.top <= 0.0 {
            return false;
        }
        let mut fastest: f32 = 0.0;
        for (h, packed) in self.heights.iter_mut().zip(&mut self.packed).filter(|(h, _)| **h > 0.0) {
            let loss = h.min((rate + (*h - floor).max(0.0) / settle) * dt);
            *h -= loss;
            *packed = packed.min(*h);
            fastest = fastest.max(loss);
        }
        // Sub-pixel melting is picked up by the idle glint refresh instead of forcing full-rate frames.
        fastest > 0.01
    }

    /// Writes the smoothed snow surface for each column, then the bed under it.
    pub fn write_smoothed(&self, buffer: &mut [f32]) {
        let count = self.heights.len();
        for i in 0..count {
            let left = self.heights[i.saturating_sub(1)];
            let right = self.heights[(i + 1).min(count - 1)];
            buffer[i] = self.bed[i] + 0.25 * left + 0.5 * self.heights[i] + 0.25 * right;
            buffer[count + i] = self.bed[i];
        }
    }

    fn update_bed(&mut self) {
        let (radius, exponent) = self.corner;
        let span = self.width();
        let column_width = self.column_width;
        self.bed = (0..self.heights.len())
            .map(|i| {
                if radius <= 0.0 {
                    return 0.0;
                }
                let x = (i as f32 + 0.5) * column_width;
                let inset = x.min(span - x);
                if inset >= radius {
                    return 0.0;
                }
                let u = 1.0 - inset / radius;
                -radius * (1.0 - (1.0 - u.powf(exponent)).max(0.0).powf(1.0 / exponent))
            })
            .collect();
        self.bed_depth = -self.bed.iter().copied().fold(0.0, f32::min);
    }

    fn update_capacity(&mut self) {
        let (phase0, phase1) = self.phases;
        self.capacity = (0..self.heights.len())
            .map(|i| {
                let x = i as f32 * self.column_width;
                let drift = 0.12 * (x * 0.0105 + phase0).sin() + 0.07 * (x * 0.037 + phase1).sin();
                self.depth * (0.95 + drift)
            })
            .collect();
    }
}

/// Moves snow between columns, packed snow in proportion.
fn shift(h: &mut [f32], p: &mut [f32], amount: f32, from: usize, to: usize) {
    let share = if h[from] > 0.0 { p[from].min(amount * p[from] / h[from]) } else { 0.0 };
    h[from] -= amount;
    h[to] += amount;
    p[from] -= share;
    p[to] += share;
}
