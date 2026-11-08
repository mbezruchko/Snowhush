use crate::geometry::V2;

/// How steep a bed snow still clings to, as a rise over run: about 55 degrees.
const CLING_SLOPE: f32 = 1.4;

/// What a heap, or some of it, is made of: the share of it that is leaves, and the share that is cherry blossom
/// petals, which are leaves too. The rest is snow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blend {
    pub leaves: f32,
    pub petals: f32,
}

impl Blend {
    pub const SNOW: Blend = Blend { leaves: 0.0, petals: 0.0 };
    pub const AUTUMN: Blend = Blend { leaves: 1.0, petals: 0.0 };
    pub const BLOSSOM: Blend = Blend { leaves: 1.0, petals: 1.0 };

    /// Only the leaves of it, in the same proportions of leaves to petals.
    pub fn leaf_only(self) -> Blend {
        if self.leaves > 0.0 { Blend { leaves: 1.0, petals: (self.petals / self.leaves).min(1.0) } } else { Blend::AUTUMN }
    }
}

/// A height field of snow columns lying on a bed: the screen bottom, or the top edge of a window
/// whose rounded corners curve the bed down at both ends.
/// A droplet of rain's spray on a window's glass: from the left end of its top edge, and how big it is.
#[derive(Clone, Copy)]
pub struct Droplet {
    pub offset: V2,
    pub radius: f32,
}

/// A piece of the film of water a running drop leaves on a window's glass: from where to where, from the left end of
/// its top edge, how wide, how old and how long it takes to dry.
#[derive(Clone, Copy)]
pub struct FilmPiece {
    pub from: V2,
    pub to: V2,
    pub width: f32,
    pub age: f32,
    pub life: f32,
}

pub struct SnowLayer {
    pub column_width: f32,
    pub opens_at_edges: bool,
    pub origin: V2,
    pub heights: Vec<f32>,
    /// How much of each column's snow a wall has packed down already, so it isn't packed again.
    pub packed: Vec<f32>,
    /// How much of each column's height is leaves, and how much of that is petals; the rest is snow.
    pub leaves: Vec<f32>,
    pub petals: Vec<f32>,
    /// The bed under each column relative to `origin.y`; zero along a flat edge, negative where a corner rounds off.
    pub bed: Vec<f32>,
    pub visible: Vec<bool>,
    pub top: f32,
    pub bed_depth: f32,
    pub occluders: Vec<[f32; 4]>,
    /// How far the window it lies on reaches down from its top edge: as far as a drop can run down it.
    pub face: f32,
    /// Windows in front of any part of the window it lies on, which hide a drop running down it.
    pub face_occluders: Vec<[f32; 4]>,
    /// Rain's spray on the window's glass, and the film running drops leave on it.
    pub spray: Vec<Droplet>,
    pub film: Vec<FilmPiece>,
    pub moved_at: f64,
    /// When the snow came back onto its window from elsewhere, to grow back into shape rather than appear at once.
    pub returned_at: f64,
    pub spill_left: f32,
    pub spill_right: f32,
    /// How high the snow lying just past each end, left then right, comes up that end: it holds up the snow on the
    /// ledge, which spills off only above it.
    pub outside: [f32; 2],
    /// Smoothed path of the supporting edge; the snow reacts to its accelerations, not to raw jittery samples.
    pub track: V2,
    pub track_velocity: V2,
    /// The snow body's displacement from its support: x slides along it, y lifts off it.
    pub offset: V2,
    pub offset_velocity: V2,
    pub previous_base_y: f32,
    pub agitation: f32,
    /// How far the snow has moved along the window since it came, as the window's left edge was dragged in or out
    /// under it: what is drawn in it moves along too.
    pub drift: f32,
    /// The window's right edge, where the snow ends short of a whole column: which edge moved tells a resize from a
    /// move.
    pub right: f32,
    /// How many leaves lie on this heap, as of the last step.
    pub lying_leaves: f32,
    /// How fast the heap was going as it came off its window at this step, for the leaves lying on it to go along.
    pub thrown: Option<V2>,
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
    /// Floats `write_smoothed` writes per column.
    pub const ROWS: usize = 4;

    pub fn new(origin: V2, columns: usize, column_width: f32, depth: f32, opens_at_edges: bool, phases: (f32, f32)) -> SnowLayer {
        let mut layer = SnowLayer {
            column_width,
            opens_at_edges,
            origin,
            heights: vec![0.0; columns],
            packed: vec![0.0; columns],
            leaves: vec![0.0; columns],
            petals: vec![0.0; columns],
            bed: vec![0.0; columns],
            visible: vec![true; columns],
            top: 0.0,
            bed_depth: 0.0,
            occluders: Vec::new(),
            face: 200.0,
            face_occluders: Vec::new(),
            spray: Vec::new(),
            film: Vec::new(),
            moved_at: -1.0,
            returned_at: -10.0,
            spill_left: 0.0,
            spill_right: 0.0,
            outside: [0.0; 2],
            track: origin,
            track_velocity: V2::ZERO,
            offset: V2::ZERO,
            offset_velocity: V2::ZERO,
            previous_base_y: origin.y,
            agitation: 0.0,
            drift: 0.0,
            right: 0.0,
            lying_leaves: 0.0,
            thrown: None,
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

    /// Where the snow's own positions along it start: what lies in it keeps its place there as it slides or the
    /// window's left edge moves under it.
    pub fn heap_x(&self) -> f32 {
        self.base().x + self.drift
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

    /// What column `i` is made of.
    pub fn blend(&self, i: usize) -> Blend {
        let h = self.heights[i];
        if h <= 0.0 || self.leaves[i] <= 0.0 {
            return Blend::SNOW;
        }
        Blend { leaves: (self.leaves[i] / h).min(1.0), petals: (self.petals[i] / h).min(1.0) }
    }

    /// What the whole layer is made of.
    pub fn overall(&self) -> Blend {
        let total: f32 = self.heights.iter().sum();
        if total <= 0.0 {
            return Blend::SNOW;
        }
        let leaves: f32 = self.leaves.iter().sum();
        let petals: f32 = self.petals.iter().sum();
        Blend { leaves: (leaves / total).min(1.0), petals: (petals / total).min(1.0) }
    }

    /// Moves the snow one column along its support and returns the column that slid off the end: its height, where
    /// it was and what it was made of.
    pub fn slide_column(&mut self, direction: i32) -> (f32, f32, f32, Blend) {
        self.release_support();
        if direction > 0 {
            let last = self.heights.len() - 1;
            let x = self.center_x(last);
            let y = self.floor(last);
            let blend = self.blend(last);
            let dropped = self.heights.pop().unwrap_or(0.0);
            self.heights.insert(0, 0.0);
            self.roll(true);
            return (dropped, x, y, blend);
        }
        let x = self.center_x(0);
        let y = self.floor(0);
        let blend = self.blend(0);
        let dropped = self.heights.remove(0);
        self.heights.push(0.0);
        self.roll(false);
        (dropped, x, y, blend)
    }

    /// Moves the packing, leaves and petals along with the heights.
    fn roll(&mut self, right: bool) {
        for values in [&mut self.packed, &mut self.leaves, &mut self.petals] {
            if right {
                values.pop();
                values.insert(0, 0.0);
            } else {
                values.remove(0);
                values.push(0.0);
            }
        }
    }

    pub fn fill(&self, i: usize) -> f32 {
        (1.0 - self.heights[i] / self.capacity[i]).max(0.0)
    }

    /// Snow taken off a column takes its leaves along in proportion; snow added to it is snow.
    pub fn set(&mut self, i: usize, height: f32) {
        let old = self.heights[i];
        self.heights[i] = height.max(0.0);
        self.packed[i] = self.packed[i].min(self.heights[i]);
        if self.heights[i] < old {
            let kept = self.heights[i] / old;
            self.leaves[i] *= kept;
            self.petals[i] *= kept;
        }
    }

    /// Takes snow alone off a column, leaving its leaves.
    pub fn remove_snow(&mut self, amount: f32, i: usize) {
        self.heights[i] = (self.heights[i] - amount).max(self.leaves[i]).max(0.0);
        self.packed[i] = self.packed[i].min(self.heights[i]);
    }

    pub fn replace(&mut self, fresh: Vec<f32>, leaves: Vec<f32>, petals: Vec<f32>) {
        let count = fresh.len();
        self.heights = fresh;
        self.leaves = leaves;
        self.petals = petals;
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
        let mut leaves = self.leaves.clone();
        let mut petals = self.petals.clone();
        fresh.resize(columns, 0.0);
        leaves.resize(columns, 0.0);
        petals.resize(columns, 0.0);
        self.replace(fresh, leaves, petals);
    }

    pub fn update_visibility(&mut self, screen_width: f32, blockers: &[[f32; 4]]) {
        for i in 0..self.heights.len() {
            let x = self.center_x(i);
            let y = self.origin.y + self.bed[i] + 1.0;
            self.visible[i] = x >= 0.0 && x <= screen_width && !blockers.iter().any(|b| x >= b[0] && x < b[2] && y >= b[1] && y < b[3]);
        }
    }

    /// Moves the layer and what it remembers of its window's path up by `by`, as the screen's coordinates shift under
    /// it, so it takes this for no motion of its window.
    pub fn raise(&mut self, by: f32) {
        self.origin.y += by;
        self.track.y += by;
        self.previous_base_y += by;
    }

    pub fn refresh_top(&mut self) {
        self.top = self.heights.iter().copied().fold(0.0, f32::max);
    }

    /// Adds snow, or what `blend` says, centred on `x` and returns the area that fell outside the layer.
    pub fn deposit(&mut self, area: f32, x: f32, wide: bool, blend: Blend) -> f32 {
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
                self.leaves[i as usize] += amount * weight * blend.leaves;
                self.petals[i as usize] += amount * weight * blend.petals;
            } else {
                missed += area * weight;
            }
        }
        missed
    }

    /// Piles snow against a wall just past column `edge`, as a plough heaps it: a wedge falling away from the wall
    /// at `slope` per column, the way `inward` points.
    pub fn heap(&mut self, area: f32, edge: usize, inward: i32, slope: f32, blend: Blend) {
        let amount = area / self.column_width;
        if amount <= 0.0 {
            return;
        }
        let mut low = self.bed[edge] + self.heights[edge];
        let mut high = low + amount;
        for _ in 0..20 {
            let middle = (low + high) / 2.0;
            if self.wedge(middle, edge, inward, slope, 0.0, blend) < amount {
                low = middle;
            } else {
                high = middle;
            }
        }
        let total = self.wedge(high, edge, inward, slope, 0.0, blend);
        self.wedge(high, edge, inward, slope, amount / total, blend);
    }

    /// The snow it takes to raise the surface to a line falling away from `level` at `edge`; adds `scale` of it.
    fn wedge(&mut self, level: f32, edge: usize, inward: i32, slope: f32, scale: f32, blend: Blend) -> f32 {
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
            self.leaves[i] += gap * scale * blend.leaves;
            self.petals[i] += gap * scale * blend.petals;
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
        // Snow clings to its bed: a layer may follow the bed down a rounded corner as far as it is no steeper than
        // `CLING_SLOPE`, on top of the angle of repose its own surface keeps.
        let steepest = CLING_SLOPE * self.column_width;
        let forward_first = self.parity;
        self.parity = !self.parity;
        let mut moved = false;
        let mut spilled_left = 0.0;
        let mut spilled_right = 0.0;
        let open = self.opens_at_edges;
        let [outside_left, outside_right] = self.outside;
        let h = &mut self.heights;
        let p = &mut self.packed;
        let l = &mut self.leaves;
        let q = &mut self.petals;
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
                let fall = b[i] - b[i + 1];
                let ahead = limit + fall.clamp(0.0, steepest);
                let behind = limit + (-fall).clamp(0.0, steepest);
                if difference > ahead + 0.01 {
                    let transfer = ((difference - ahead) * 0.5).min(h[i]);
                    shift(h, p, l, q, transfer, i, i + 1);
                    if transfer > 0.0 {
                        moved = true;
                    }
                } else if difference < -behind - 0.01 {
                    let transfer = ((-difference - behind) * 0.5).min(h[i + 1]);
                    shift(h, p, l, q, transfer, i + 1, i);
                    if transfer > 0.0 {
                        moved = true;
                    }
                }
            }
            if open {
                if !held[0] && h[0] - outside_left > limit + 0.01 {
                    let transfer = (h[0] - outside_left - limit) * 0.5;
                    spill(h, p, l, q, transfer, 0);
                    spilled_left += transfer;
                    moved = true;
                }
                if !held[count - 1] && h[count - 1] - outside_right > limit + 0.01 {
                    let transfer = (h[count - 1] - outside_right - limit) * 0.5;
                    spill(h, p, l, q, transfer, count - 1);
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
                shift(h, p, l, q, transfer, i, i + 1);
            } else {
                shift(h, p, l, q, -transfer, i + 1, i);
            }
            if transfer.abs() > 0.003 {
                moved = true;
            }
        }
        self.spill_left += spilled_left * self.column_width;
        self.spill_right += spilled_right * self.column_width;
        moved
    }

    /// Snow down to `floor` melts slowly; anything heaped above it settles back down over `settle` seconds. Leaves
    /// wither away to nothing: a column partly leaves keeps that much less of its floor.
    pub fn thaw(&mut self, dt: f32, floor: f32, rate: f32, settle: f32) -> bool {
        if self.top <= 0.0 {
            return false;
        }
        let mut fastest: f32 = 0.0;
        for i in 0..self.heights.len() {
            let h = self.heights[i];
            if h <= 0.0 {
                continue;
            }
            let kept = floor * (1.0 - (self.leaves[i] / h).min(1.0));
            let loss = h.min((rate + (h - kept).max(0.0) / settle) * dt);
            self.heights[i] = h - loss;
            self.packed[i] = self.packed[i].min(self.heights[i]);
            self.leaves[i] *= self.heights[i] / h;
            self.petals[i] *= self.heights[i] / h;
            fastest = fastest.max(loss);
        }
        // Sub-pixel melting is picked up by the idle glint refresh instead of forcing full-rate frames.
        fastest > 0.01
    }

    /// Writes the smoothed snow surface for each column, then the bed under it, the share of it that is leaves and
    /// the share of those that are petals.
    pub fn write_smoothed(&self, buffer: &mut [f32]) {
        let count = self.heights.len();
        let mut empty = true;
        for i in 0..count {
            let (l, r) = (i.saturating_sub(1), (i + 1).min(count - 1));
            let height = 0.25 * self.heights[l] + 0.5 * self.heights[i] + 0.25 * self.heights[r];
            let leafy = 0.25 * self.leaves[l] + 0.5 * self.leaves[i] + 0.25 * self.leaves[r];
            let petal = 0.25 * self.petals[l] + 0.5 * self.petals[i] + 0.25 * self.petals[r];
            buffer[i] = self.bed[i] + height;
            buffer[count + i] = self.bed[i];
            buffer[2 * count + i] = if height > 0.0 { (leafy / height).min(1.0) } else { -1.0 };
            buffer[3 * count + i] = if leafy > 0.0 { (petal / leafy).min(1.0) } else { 0.0 };
            if height > 0.0 {
                empty = false;
            }
        }
        // A bare column takes its neighbour's makeup, or the shader would blend a heap's end with snow that isn't there.
        let (shares, kinds) = buffer[2 * count..4 * count].split_at_mut(count);
        if empty {
            shares.fill(0.0);
            return;
        }
        for i in 1..count {
            if shares[i] < 0.0 && shares[i - 1] >= 0.0 {
                shares[i] = shares[i - 1];
                kinds[i] = kinds[i - 1];
            }
        }
        for i in (0..count.saturating_sub(1)).rev() {
            if shares[i] < 0.0 {
                shares[i] = shares[i + 1];
                kinds[i] = kinds[i + 1];
            }
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

/// Moves snow between columns, packed snow and leaves in proportion.
fn shift(h: &mut [f32], p: &mut [f32], l: &mut [f32], q: &mut [f32], amount: f32, from: usize, to: usize) {
    let share = if h[from] > 0.0 { p[from].min(amount * p[from] / h[from]) } else { 0.0 };
    let part = if h[from] > 0.0 { (amount / h[from]).min(1.0) } else { 0.0 };
    h[from] -= amount;
    h[to] += amount;
    p[from] -= share;
    p[to] += share;
    let (leaf, petal) = (l[from] * part, q[from] * part);
    l[from] -= leaf;
    l[to] += leaf;
    q[from] -= petal;
    q[to] += petal;
}

/// Spilled off an end, a column's leaves go with its snow.
fn spill(h: &mut [f32], p: &mut [f32], l: &mut [f32], q: &mut [f32], amount: f32, from: usize) {
    let part = if h[from] > 0.0 { (amount / h[from]).min(1.0) } else { 0.0 };
    h[from] -= amount;
    p[from] = p[from].min(h[from]);
    l[from] *= 1.0 - part;
    q[from] *= 1.0 - part;
}
