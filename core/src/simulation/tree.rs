//! The New Year's fir standing in the snow at the bottom of the screen. Its boughs hang on springs: the wind sways the
//! tree and the cursor brushes them aside, snow settles on them and comes off when they are shaken. Baubles swing
//! under them, a garland twinkles over them and presents sit in the drift at its foot, Santa's hand reaching out of
//! one for the cursor. The Mac app has the same in Swift; both grow the same tree.

use std::f32::consts::PI;

use super::Random;
use crate::geometry::V2;

/// From the foot of the trunk to the top of its leader; the star sits above.
pub const TREE_HEIGHT: f32 = 168.0;
/// Snow along a bough lies in this many cells, between these shares of its length.
pub const CELL_COUNT: usize = 8;
const SNOW_FROM: f32 = 0.08;
const SNOW_TO: f32 = 0.98;
/// The space it takes, from the foot of its trunk: left, bottom, right, top.
pub const TREE_EXTENT: [f32; 4] = [-120.0, -10.0, 120.0, 250.0];

/// Snow comes off boughs as it comes off windows, by the same gravity, but needles hold it far more loosely than a
/// window's top: it lifts off when the bough drops away faster than gravity and this; it is knocked off sideways when
/// the bough swings across harder than `BOUGH_GRIP` times what presses it down (this pressing on top of its weight); it
/// slides off the tip of a bough tipped down steeper than that grip lets it hold; and a bough shaken past
/// `BOUGH_SHAKING` gravities throws it off.
const GRAVITY: f32 = 2500.0;
const BOUGH_ADHESION: f32 = 500.0;
const BOUGH_HOLD: f32 = 300.0;
const BOUGH_GRIP: f32 = 0.8;
const BOUGH_SLIP: f32 = 0.6;
const BOUGH_SHAKING: f32 = 1.2;

/// Where the middle of each cell of snow is along a bough.
pub fn cell_along(k: usize) -> f32 {
    SNOW_FROM + (k as f32 + 0.5) / CELL_COUNT as f32 * (SNOW_TO - SNOW_FROM)
}

/// How far the needles reach either side of a bough's middle, as a share of half its width, `along` it.
pub fn profile(along: f32) -> f32 {
    let rise = (along / 0.25).clamp(0.0, 1.0);
    0.25 + 0.75 * rise * rise * (3.0 - 2.0 * rise) * (1.0 - along).max(0.0).powf(0.7)
}

/// How far a bough leaving the trunk at `y` reaches.
fn reach_at(y: f32) -> f32 {
    10.0 + 62.0 * (1.0 - (y - 14.0) / 134.0).clamp(0.0, 1.0).powf(1.1)
}

fn upward(reach: V2) -> V2 {
    let across = V2::new(-reach.y, reach.x).normalized();
    if across.y < 0.0 { -across } else { across }
}

#[derive(Clone, Copy, Debug)]
pub struct Bough {
    /// Around the trunk: zero faces the viewer.
    pub azimuth: f32,
    /// Where it leaves the trunk, above the tree's foot.
    pub height: f32,
    pub length: f32,
    /// Across its needles.
    pub width: f32,
    /// From where it leaves the trunk to its tip, as seen, at rest.
    pub reach: V2,
    /// Across it, on the side that faces up.
    pub up: V2,
    pub frequency: f32,
    /// The most snow it holds, in square points.
    pub capacity: f32,
    pub variant: f32,
    pub seed: f32,
    /// The trunk, drawn as a bough of bark.
    pub trunk: bool,
    /// The dark mass of needles deep inside the tree, drawn behind the boughs so nothing shows through it.
    pub core: bool,
    /// How far its tip is bent from rest, how fast it moves and how fast that changes.
    pub offset: V2,
    velocity: V2,
    jolt: V2,
    /// The snow lying along it, in square points, from near the trunk out to the tip.
    pub cells: [f32; CELL_COUNT],
    /// How far the snow has slid along it toward the tip since it last moved a cell, and how fast it slides.
    slide: f32,
    slide_velocity: f32,
    /// Shaking, as a window's snow feels it: back-and-forth swings of its tip, in gravities.
    agitation: f32,
    swing_peak: f32,
    previous_swing_peak: f32,
    swing_direction: V2,
}

impl Bough {
    #[allow(clippy::too_many_arguments)]
    fn new(azimuth: f32, height: f32, length: f32, width: f32, reach: V2, up: V2, frequency: f32, capacity: f32, variant: f32, seed: f32) -> Bough {
        Bough {
            azimuth,
            height,
            length,
            width,
            reach,
            up,
            frequency,
            capacity,
            variant,
            seed,
            trunk: false,
            core: false,
            offset: V2::ZERO,
            velocity: V2::ZERO,
            jolt: V2::ZERO,
            cells: [0.0; CELL_COUNT],
            slide: 0.0,
            slide_velocity: 0.0,
            agitation: 0.0,
            swing_peak: 0.0,
            previous_swing_peak: 0.0,
            swing_direction: V2::ZERO,
        }
    }

    pub fn depth(&self) -> f32 {
        self.azimuth.cos()
    }

    pub fn snow(&self) -> f32 {
        self.cells.iter().sum()
    }

    pub fn load(&self) -> f32 {
        if self.capacity > 0.0 { (self.snow() / self.capacity).min(1.0) } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bauble {
    pub bough: usize,
    pub along: f32,
    pub string: f32,
    pub radius: f32,
    pub color: [f32; 3],
    pub seed: f32,
    /// From straight down, positive with the ball to the right.
    pub angle: f32,
    spin: f32,
    pub pivot: V2,
    pivot_velocity: V2,
}

#[derive(Clone, Copy, Debug)]
pub struct Bulb {
    pub bough: usize,
    pub along: f32,
    /// Across the bough, toward its upper side.
    pub lift: f32,
    pub color: [f32; 3],
    phase: f32,
    rate: f32,
    pub brightness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandPhase {
    Shut,
    Rattling,
    Reaching,
    Holding,
    Retreating,
    Cheering,
}

#[derive(Clone, Copy, Debug)]
pub struct Present {
    /// From the trunk to its middle.
    pub offset: f32,
    pub width: f32,
    pub height: f32,
    pub style: f32,
    pub bottom: f32,
    /// How far its lid is lifted, in radians about its left hinge, and how fast.
    pub lid: f32,
    lid_velocity: f32,
    pub snow: f32,
    /// Santa's hand inside: what it is doing and for how long, how far its arm is out, where the hand is and how fast
    /// it moves, how tight it grips and how far it turns from the arm, for a shake or a wave.
    pub phase: HandPhase,
    since: f32,
    pub arm: f32,
    pub hand: V2,
    hand_velocity: V2,
    pub grip: f32,
    pub wave: f32,
    /// How far it rattles sideways before it opens.
    pub jiggle: f32,
    /// When it may open again.
    rested: f32,
}

impl Present {
    fn new(offset: f32, width: f32, height: f32, style: f32) -> Present {
        Present {
            offset,
            width,
            height,
            style,
            bottom: 0.0,
            lid: 0.0,
            lid_velocity: 0.0,
            snow: 0.0,
            phase: HandPhase::Shut,
            since: 0.0,
            arm: 0.0,
            hand: V2::ZERO,
            hand_velocity: V2::ZERO,
            grip: 0.0,
            wave: 0.0,
            jiggle: 0.0,
            rested: 0.0,
        }
    }

    pub fn capacity(&self) -> f32 {
        self.width * 4.5
    }
}

/// Snow let go off the tree: how much, where, how fast and over how wide a stretch.
#[derive(Clone, Copy, Debug)]
pub struct Shed {
    pub area: f32,
    pub position: V2,
    pub velocity: V2,
    pub spread: V2,
}

/// A fixed sequence, so the tree grows the same every time and on every platform: the Mac's `TreeRandom`, with Swift's
/// way of picking a number below a bound and of shuffling.
struct TreeRandom(u64);

impl TreeRandom {
    fn bits(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn next(&mut self) -> f32 {
        (self.bits() >> 40) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lower: f32, upper: f32) -> f32 {
        lower + (upper - lower) * self.next()
    }

    fn pick(&mut self, count: usize) -> usize {
        ((self.next() * count as f32) as usize).min(count - 1)
    }

    /// Below `bound`, as Swift's `RandomNumberGenerator.next(upperBound:)` picks it.
    fn below(&mut self, bound: u64) -> u64 {
        let mut m = self.bits() as u128 * bound as u128;
        if (m as u64) < bound {
            let t = 0u64.wrapping_sub(bound) % bound;
            while (m as u64) < t {
                m = self.bits() as u128 * bound as u128;
            }
        }
        (m >> 64) as u64
    }

    /// As Swift's `shuffled(using:)`.
    fn shuffle<T>(&mut self, items: &mut [T]) {
        let mut amount = items.len();
        let mut current = 0;
        while amount > 1 {
            let random = self.below(amount as u64) as usize;
            amount -= 1;
            items.swap(current, current + random);
            current += 1;
        }
    }
}

pub struct Tree {
    pub boughs: Vec<Bough>,
    pub baubles: Vec<Bauble>,
    pub bulbs: Vec<Bulb>,
    pub presents: Vec<Present>,
    /// The middle of the trunk's foot.
    pub base: V2,
    /// How far the top of the tree leans over, and how fast.
    pub sway: f32,
    sway_velocity: f32,
    sway_acceleration: f32,
    pub time: f32,
    /// Whether the cursor brushed anything on the last step.
    pub touched: bool,
    /// Whether anything still moves enough to be seen.
    pub moving: bool,
    /// Seconds past the New Year's midnight, from the countdown before it until the fireworks are over; none otherwise.
    pub celebration: Option<f32>,
    /// How brightly the star shines.
    pub star_glow: f32,
    /// Where the hands out of the presents hold sparklers, at midnight.
    pub sparklers: Vec<V2>,
    random: Random,
}

impl Default for Tree {
    fn default() -> Tree {
        Tree::new()
    }
}

impl Tree {
    pub fn new() -> Tree {
        let mut random = TreeRandom(0x9E37_79B9_7F4A_7C15);
        let mut boughs = Vec::new();
        // Whorls of boughs from the foot to the top: the low ones long and hanging, the high ones short and raised.
        let tiers = 14;
        for k in 0..tiers {
            let t = k as f32 / (tiers - 1) as f32;
            let y = 14.0 + 134.0 * t.powf(0.9);
            let length = reach_at(y);
            let count = ((11.0 - 6.0 * t).round() as usize).max(4);
            let turn = k as f32 * 2.39996;
            for j in 0..count {
                let azimuth = turn + j as f32 * 2.0 * PI / count as f32 + random.range(-0.3, 0.3);
                let l = length * random.range(0.8, 1.12);
                let droop = 0.42 * (1.0 - t) - 0.35 * t + random.range(-0.08, 0.08);
                let mut reach = V2::new(l * azimuth.sin() * droop.cos(), -l * droop.sin());
                let shown = reach.length();
                if shown < 0.5 * l {
                    reach *= 0.5 * l / shown;
                }
                let width = l * random.range(0.62, 0.74);
                let height = y + random.range(-2.5, 2.5);
                let frequency = (2.4 * (45.0 / l).sqrt()).clamp(1.8, 4.5);
                let variant = random.pick(3) as f32;
                let seed = random.next();
                boughs.push(Bough::new(azimuth, height, l, width, reach, upward(reach), frequency, l * width * 0.11, variant, seed));
            }
        }
        // The leader stands straight up at the top, with a few short shoots round its foot.
        for j in 0..5 {
            let azimuth = j as f32 * 1.256 + 0.4;
            let l = random.range(9.0, 13.0);
            let reach = V2::new(l * azimuth.sin() * 0.9f32.cos(), l * 0.9f32.sin());
            let height = 147.0 + random.range(0.0, 4.0);
            let variant = random.pick(3) as f32;
            let seed = random.next();
            boughs.push(Bough::new(azimuth, height, l, l * 0.6, reach, upward(reach), 4.5, l * l * 0.4 * 0.11, variant, seed));
        }
        boughs.push(Bough::new(0.2, 144.0, 30.0, 12.0, V2::new(0.5, 30.0), V2::new(-1.0, 0.0), 3.6, 0.0, 0.0, 0.5));
        // Back to front, so the nearer ones are drawn over the farther.
        boughs.sort_by(|a, b| a.depth().total_cmp(&b.depth()));
        let mut trunk = Bough::new(PI, -12.0, 150.0, 9.0, V2::new(0.0, 150.0), V2::new(-1.0, 0.0), 0.0, 0.0, 0.0, 0.3);
        trunk.trunk = true;
        let mut core = Bough::new(PI, 6.0, 146.0, 2.0 * reach_at(14.0), V2::new(0.0, 146.0), V2::new(-1.0, 0.0), 0.0, 0.0, 0.0, 0.7);
        core.core = true;
        boughs.splice(0..0, [trunk, core]);

        let baubles = Self::hang(&boughs, &mut random);
        let bulbs = Self::string(&boughs, &mut random);
        let presents = vec![Present::new(-38.0, 31.0, 23.0, 0.0), Present::new(33.0, 25.0, 19.0, 1.0), Present::new(2.0, 21.0, 27.0, 2.0)];
        let mut tree = Tree {
            boughs,
            baubles,
            bulbs,
            presents,
            base: V2::new(120.0, 0.0),
            sway: 0.0,
            sway_velocity: 0.0,
            sway_acceleration: 0.0,
            time: 0.0,
            touched: false,
            moving: true,
            celebration: None,
            star_glow: 1.0,
            sparklers: Vec::new(),
            random: Random::new(),
        };
        tree.settle_baubles();
        tree
    }

    // Shape

    /// How far the trunk is bent sideways at `y` above its foot.
    pub fn bend(&self, y: f32) -> f32 {
        let share = y.max(0.0) / TREE_HEIGHT;
        self.sway * share * share
    }

    pub fn root(&self, bough: &Bough) -> V2 {
        self.base + V2::new(self.bend(bough.height), bough.height)
    }

    /// The point `along` the bough's middle, from where it leaves the trunk to its tip.
    pub fn point(&self, i: usize, along: f32) -> V2 {
        let bough = &self.boughs[i];
        self.root(bough) + bough.reach * along + bough.offset * (along * along)
    }

    /// Where snow rests on the bough, `along` it.
    pub fn perch(&self, i: usize, along: f32) -> V2 {
        let bough = &self.boughs[i];
        self.point(i, along) + bough.up * (bough.width * 0.5 * profile(along) * 0.62)
    }

    fn hang(boughs: &[Bough], random: &mut TreeRandom) -> Vec<Bauble> {
        let colors = [[0.78, 0.07, 0.1], [0.93, 0.68, 0.2], [0.78, 0.8, 0.86], [0.1, 0.28, 0.72]];
        let mut baubles: Vec<Bauble> = Vec::new();
        let mut taken: Vec<V2> = Vec::new();
        let mut candidates: Vec<usize> =
            (0..boughs.len()).filter(|&i| !boughs[i].trunk && boughs[i].capacity > 0.0 && boughs[i].depth() > 0.2).collect();
        random.shuffle(&mut candidates);
        for i in candidates {
            if baubles.len() >= 17 {
                continue;
            }
            let bough = &boughs[i];
            let along = random.range(0.55, 0.85);
            let place = V2::new(0.0, bough.height) + bough.reach * along;
            if taken.iter().any(|&spot| spot.distance(place) < 15.0) {
                continue;
            }
            taken.push(place);
            let low = 1.0 - bough.height / TREE_HEIGHT;
            let string = random.range(4.0, 8.0);
            let radius = 3.0 + 2.0 * low * random.range(0.7, 1.0);
            let color = colors[baubles.len() % colors.len()];
            let seed = random.next();
            baubles.push(Bauble { bough: i, along, string, radius, color, seed, angle: 0.0, spin: 0.0, pivot: V2::ZERO, pivot_velocity: V2::ZERO });
        }
        baubles
    }

    /// A garland wound three and a half times around the tree, from its foot to under the leader: each bulb hangs on
    /// the bough nearest its place, so it moves with it.
    fn string(boughs: &[Bough], random: &mut TreeRandom) -> Vec<Bulb> {
        let colors = [[1.0, 0.22, 0.16], [1.0, 0.7, 0.2], [0.25, 0.95, 0.4], [0.3, 0.5, 1.0], [1.0, 0.35, 0.75]];
        let count = 46;
        let mut bulbs = Vec::new();
        for k in 0..count {
            let t = (k as f32 + 0.5) / count as f32;
            let y = 22.0 + t * 116.0;
            let azimuth = t * 3.5 * 2.0 * PI + 0.6;
            let radius = reach_at(y) * 0.7;
            let target = V2::new(radius * azimuth.sin(), y);
            let front = azimuth.cos() > 0.0;
            let mut best: Option<(usize, f32, f32)> = None;
            for (i, bough) in boughs.iter().enumerate() {
                if bough.trunk || bough.capacity <= 0.0 || (bough.depth() > 0.0) != front {
                    continue;
                }
                for along in [0.35, 0.5, 0.65, 0.8, 0.95] {
                    let place = V2::new(0.0, bough.height) + bough.reach * along;
                    let distance = place.distance(target);
                    if distance < best.map_or(f32::INFINITY, |b| b.2) {
                        best = Some((i, along, distance));
                    }
                }
            }
            let Some((index, along, _)) = best else { continue };
            let bough = &boughs[index];
            let lift = bough.width * 0.5 * profile(along) * random.range(-0.4, 0.4);
            let phase = random.range(0.0, 2.0 * PI);
            let rate = random.range(0.7, 1.9);
            bulbs.push(Bulb { bough: index, along, lift, color: colors[k % colors.len()], phase, rate, brightness: 1.0 });
        }
        bulbs
    }

    // Motion

    /// Moves the tree on by `dt` in a wind of `wind` points a second, brushed by the cursor; snow that comes off a
    /// bough is put in `shed`.
    pub fn step(&mut self, dt: f32, wind: f32, cursor: Option<V2>, cursor_velocity: V2, thaws: bool, shed: &mut Vec<Shed>) {
        self.time += dt;
        self.touched = false;
        let base = self.base;
        let brushing = cursor.filter(|point| {
            let local = *point - base;
            let inside = local.x > TREE_EXTENT[0] && local.x < TREE_EXTENT[2] && local.y > TREE_EXTENT[1] && local.y < TREE_HEIGHT + 20.0;
            inside && cursor_velocity.length() > 12.0
        });
        self.sway_trunk(dt, wind);
        let mut energy = self.sway_velocity.abs();
        for i in 0..self.boughs.len() {
            if self.boughs[i].trunk || self.boughs[i].core {
                continue;
            }
            energy = energy.max(self.boughs[i].velocity.length());
            self.move_bough(i, dt, wind, brushing, cursor_velocity, shed);
            self.hold_snow(i, dt, shed);
            if thaws {
                let melt = (-dt / 400.0).exp();
                for cell in &mut self.boughs[i].cells {
                    *cell *= melt;
                }
            }
        }
        self.swing_baubles(dt, brushing, cursor_velocity);
        self.twinkle();
        self.tend_presents(dt, cursor, shed);
        self.moving = energy > 0.4 || wind.abs() > 1.0 || self.touched;
    }

    fn sway_trunk(&mut self, dt: f32, wind: f32) {
        let t = self.time;
        let gusting = ((t * 1.31).sin() + 0.7 * (t * 2.13 + 1.1).sin() + 0.5 * (t * 3.7 + 2.3).sin()) / 2.2;
        let target = (wind * 0.16).clamp(-30.0, 30.0) + gusting * (1.6 + 0.05 * wind.abs().min(300.0));
        let omega = 2.0 * PI * 0.55;
        let acceleration = omega * omega * (target - self.sway) - 2.0 * 0.1 * omega * self.sway_velocity;
        self.sway_velocity += acceleration * dt;
        self.sway += self.sway_velocity * dt;
        self.sway_acceleration = acceleration;
    }

    fn move_bough(&mut self, i: usize, dt: f32, wind: f32, cursor: Option<V2>, cursor_velocity: V2, shed: &mut Vec<Shed>) {
        let mut bough = self.boughs[i];
        let root = self.root(&bough);
        let before = bough.velocity;
        let omega = 2.0 * PI * bough.frequency;
        let share = bough.height / TREE_HEIGHT;
        let flutter = 0.25 + 0.012 * wind.abs().min(400.0);
        let seed = bough.seed * 40.0;
        let time = self.time;
        let rest = V2::new(
            flutter * (time * (2.1 + bough.seed) + seed).sin(),
            -bough.load() * bough.length * 0.1 + flutter * 0.6 * (time * (2.9 + bough.seed) + seed * 1.7).sin(),
        );
        let mut acceleration = (rest - bough.offset) * (omega * omega) - bough.velocity * (2.0 * 0.09 * omega);
        // The trunk swings it along, and it lags behind.
        acceleration.x -= self.sway_acceleration * share * share * 0.6;
        bough.velocity += acceleration * dt;

        if let Some(cursor) = cursor
            && let Some((along, depth)) = contact(&bough, root, cursor)
        {
            self.touched = true;
            // The needles catch on the cursor: where it touches, the bough is dragged along with it.
            let mut target = cursor_velocity / (along * along).max(0.15) * 0.8;
            let speed = target.length();
            if speed > 1400.0 {
                target *= 1400.0 / speed;
            }
            let grip = (dt * 40.0 * depth).min(1.0) * ((along - 0.05) / 0.3).clamp(0.0, 1.0);
            bough.velocity += (target - bough.velocity) * grip;
            self.sway_velocity += cursor_velocity.x * 0.0015 * depth * share;
            // Where it pushes through the needles it knocks the snow off them, as it pushes a window's snow aside: the
            // faster it goes, the more, thrown along its way.
            let pace = cursor_velocity.length();
            let reach = 9.0 / bough.length.max(1.0) + 0.5 * (SNOW_TO - SNOW_FROM) / CELL_COUNT as f32;
            let knocked = (pace / 320.0).min(1.0) * (depth * 2.0).min(1.0) * (dt * 30.0).min(1.0);
            let line = (bough.reach + bough.offset + V2::new(0.0, 0.001)).normalized();
            let mut normal = V2::new(-line.y, line.x);
            if normal.dot(bough.up) < 0.0 {
                normal = -normal;
            }
            for k in 0..CELL_COUNT {
                if (cell_along(k) - along).abs() >= reach || bough.cells[k] <= 0.05 {
                    continue;
                }
                let amount = bough.cells[k] * knocked;
                if amount <= 0.05 {
                    continue;
                }
                let at_along = cell_along(k);
                let at = root
                    + bough.reach * at_along
                    + bough.offset * (at_along * at_along)
                    + normal * (bough.width * 0.5 * profile(at_along) * 0.62 + 1.0);
                let random = &mut self.random;
                let thrown = cursor_velocity * random.range(0.35, 0.7) + V2::new(random.range(-30.0, 30.0), random.range(10.0, 60.0));
                shed.push(Shed { area: amount, position: at, velocity: thrown, spread: V2::new(3.0, 1.5) });
                bough.cells[k] -= amount;
            }
        }

        bough.offset += bough.velocity * dt;
        let most = bough.length * 0.5;
        let bent = bough.offset.length();
        if bent > most {
            let direction = bough.offset / bent;
            bough.offset = direction * most;
            let outward = bough.velocity.dot(direction);
            if outward > 0.0 {
                bough.velocity -= direction * outward;
            }
        }

        // Smoothed over a few milliseconds: the snow feels the needles' push, not a single frame's step.
        let jolt = (bough.velocity - before) / dt.max(1.0 / 240.0);
        bough.jolt += (jolt - bough.jolt) * (dt / 0.02).min(1.0);
        self.boughs[i] = bough;
    }

    /// The snow on a bough, felt in the bough's own frame, as a window's snow is in its window's. Each cell is pressed
    /// on by gravity and by the bough's acceleration where it lies, which grows toward the tip. Where the bough drops
    /// away faster than that and the needles' hold, the snow flies free as it was moving. A side bough seen moving along
    /// its length is really swinging round the trunk, across itself: where that swing outdoes the snow's grip, it knocks
    /// the snow off. Tipped down steeply, the snow slides along it and off the tip. Shaken back and forth hard enough,
    /// it is thrown off in bits.
    fn hold_snow(&mut self, i: usize, dt: f32, shed: &mut Vec<Shed>) {
        let mut bough = self.boughs[i];
        let share = bough.height / TREE_HEIGHT;
        let root_acceleration = V2::new(self.sway_acceleration * share * share, 0.0);
        let root_velocity = V2::new(self.sway_velocity * share * share, 0.0);

        // Agitation is vibration: two consecutive back-and-forth swings, not a single start or stop.
        bough.agitation *= (-dt / 0.3).exp();
        let magnitude = bough.jolt.length();
        if magnitude > 1.0 && bough.jolt.dot(bough.swing_direction) < 0.0 {
            bough.agitation = bough.agitation.max(bough.previous_swing_peak.min(bough.swing_peak) / GRAVITY);
            bough.previous_swing_peak = bough.swing_peak;
            bough.swing_peak = magnitude;
            bough.swing_direction = bough.jolt / magnitude;
        } else {
            bough.swing_peak = bough.swing_peak.max(magnitude);
            if magnitude > 1.0 && bough.swing_direction == V2::ZERO {
                bough.swing_direction = bough.jolt / magnitude;
            }
        }
        if bough.snow() <= 0.05 {
            bough.slide = 0.0;
            bough.slide_velocity = 0.0;
            self.boughs[i] = bough;
            return;
        }

        let line = bough.reach + bough.offset * (2.0 * 0.6);
        let tangent = line.normalized();
        let mut normal = V2::new(-tangent.y, tangent.x);
        if normal.dot(bough.up) < 0.0 {
            normal = -normal;
        }
        let cell_length = (SNOW_TO - SNOW_FROM) * bough.reach.length() / CELL_COUNT as f32;
        let thickness = |cells: &[f32; CELL_COUNT], k: usize| cells[k] / cell_length.max(0.5);
        let perches: [V2; CELL_COUNT] = std::array::from_fn(|k| self.perch_of(&bough, cell_along(k)));
        let place = |cells: &[f32; CELL_COUNT], k: usize| perches[k] + normal * (thickness(cells, k) * 0.5);
        let spread = |cells: &[f32; CELL_COUNT], k: usize| V2::new(cell_length * 0.5 * tangent.x.abs() + 1.0, thickness(cells, k) * 0.5 + 0.5);

        // The bough's real slope: seen from the front it may point at the viewer, but its height along it is as seen.
        let rise = ((bough.reach.y + bough.offset.y) / bough.length).clamp(-1.0, 1.0);
        let level = (1.0 - rise * rise).sqrt();

        // Thrown free where the bough drops away faster than gravity and the needles' hold, or knocked off where it
        // swings across too hard: sideways on the screen is across the bough, round the trunk for one seen side on.
        for k in 0..CELL_COUNT {
            if bough.cells[k] <= 0.02 {
                continue;
            }
            let along = cell_along(k);
            let acceleration = root_acceleration + bough.jolt * (along * along);
            let pressing = level * (acceleration.y + GRAVITY);
            let across = acceleration.x.abs();
            let lifts = pressing < -BOUGH_ADHESION;
            let hold = BOUGH_GRIP * (pressing + BOUGH_HOLD).max(0.0);
            if !(lifts || across > hold) {
                continue;
            }
            let velocity = root_velocity + bough.velocity * (along * along) + tangent * bough.slide_velocity;
            // Knocked off, it doesn't all go at once: the harder the swing, the more of it.
            let share = if lifts { 1.0 } else { (across / hold.max(1.0) - 0.75).min(1.0) };
            let amount = bough.cells[k] * share;
            shed.push(Shed { area: amount, position: place(&bough.cells, k), velocity, spread: spread(&bough.cells, k) });
            bough.cells[k] -= amount;
        }

        // Sliding along the bough under its own weight, all of its snow together, as a window's heap slides.
        let middle = 0.6;
        let acceleration = root_acceleration + bough.jolt * (middle * middle);
        let pressure = (level * (acceleration.y + GRAVITY) + BOUGH_HOLD).max(0.0);
        let drive = -GRAVITY * rise;
        let mut slide_velocity = bough.slide_velocity;
        if !(slide_velocity == 0.0 && drive.abs() <= BOUGH_GRIP * pressure) {
            let direction = if slide_velocity != 0.0 {
                if slide_velocity > 0.0 { 1.0 } else { -1.0 }
            } else if drive > 0.0 {
                1.0
            } else {
                -1.0
            };
            let next = slide_velocity + (drive - direction * BOUGH_SLIP * pressure) * dt;
            slide_velocity = if pressure > 0.0 && next * direction <= 0.0 { 0.0 } else { next };
        }
        bough.slide += slide_velocity * dt;
        let carried = root_velocity + bough.velocity + tangent * slide_velocity;
        while bough.slide >= cell_length {
            // Off the tip.
            let last = CELL_COUNT - 1;
            if bough.cells[last] > 0.02 {
                shed.push(Shed {
                    area: bough.cells[last],
                    position: place(&bough.cells, last) + tangent * (cell_length * 0.5),
                    velocity: carried,
                    spread: spread(&bough.cells, last),
                });
            }
            for k in (1..=last).rev() {
                bough.cells[k] = bough.cells[k - 1];
            }
            bough.cells[0] = 0.0;
            bough.slide -= cell_length;
        }
        while bough.slide <= -cell_length {
            // Against the trunk it packs up and stops.
            bough.cells[0] += bough.cells[1];
            for k in 1..(CELL_COUNT - 1) {
                bough.cells[k] = bough.cells[k + 1];
            }
            bough.cells[CELL_COUNT - 1] = 0.0;
            bough.slide += cell_length;
            if bough.cells[0] > bough.capacity / CELL_COUNT as f32 * 1.6 {
                slide_velocity = 0.0;
            }
        }
        bough.slide_velocity = slide_velocity;

        // A granular bed shaken well past gravity throws bits off its top.
        if bough.agitation > BOUGH_SHAKING {
            let rate = (0.3 * (bough.agitation - BOUGH_SHAKING)).min(0.8);
            let picks = (CELL_COUNT as f32 * rate * dt * 2.0 + self.random.next()) as usize;
            for _ in 0..picks {
                let k = ((self.random.next() * CELL_COUNT as f32) as usize).min(CELL_COUNT - 1);
                let amount = bough.cells[k] * 0.5;
                if amount <= 0.1 {
                    continue;
                }
                let along = cell_along(k);
                let random = &mut self.random;
                let velocity = root_velocity + bough.velocity * (along * along) + V2::new(random.range(-80.0, 80.0), random.range(50.0, 200.0));
                shed.push(Shed { area: amount, position: place(&bough.cells, k), velocity, spread: spread(&bough.cells, k) });
                bough.cells[k] -= amount;
            }
        }
        if bough.snow() <= 0.05 {
            bough.slide = 0.0;
            bough.slide_velocity = 0.0;
        }
        self.boughs[i] = bough;
    }

    /// Where snow rests on `bough`, `along` it, as it is now rather than as stored.
    fn perch_of(&self, bough: &Bough, along: f32) -> V2 {
        self.root(bough) + bough.reach * along + bough.offset * (along * along) + bough.up * (bough.width * 0.5 * profile(along) * 0.62)
    }

    fn settle_baubles(&mut self) {
        for k in 0..self.baubles.len() {
            self.baubles[k].pivot = self.pivot(&self.baubles[k]);
        }
    }

    pub fn pivot(&self, bauble: &Bauble) -> V2 {
        let bough = &self.boughs[bauble.bough];
        self.point(bauble.bough, bauble.along) - bough.up * (bough.width * 0.5 * profile(bauble.along) * 0.25)
    }

    pub fn ball(&self, bauble: &Bauble) -> V2 {
        bauble.pivot + V2::new(bauble.angle.sin(), -bauble.angle.cos()) * (bauble.string + bauble.radius)
    }

    /// Each bauble swings on its string from a bough that moves: in the bough's frame gravity leans the other way.
    fn swing_baubles(&mut self, dt: f32, cursor: Option<V2>, cursor_velocity: V2) {
        let gravity = 900.0;
        for k in 0..self.baubles.len() {
            let mut bauble = self.baubles[k];
            let pivot = self.pivot(&bauble);
            let velocity = (pivot - bauble.pivot) / dt.max(1.0 / 240.0);
            let mut acceleration = (velocity - bauble.pivot_velocity) / dt.max(1.0 / 240.0);
            let jolt = acceleration.length();
            if jolt > 20000.0 {
                acceleration *= 20000.0 / jolt;
            }
            bauble.pivot = pivot;
            bauble.pivot_velocity = velocity;
            let reach = bauble.string + bauble.radius;
            let pull = V2::new(0.0, -gravity) - acceleration;
            bauble.spin += (pull.x * bauble.angle.cos() + pull.y * bauble.angle.sin()) / reach * dt;
            if let Some(cursor) = cursor
                && cursor.distance(self.ball(&bauble)) < bauble.radius + 4.0
            {
                self.touched = true;
                let along = V2::new(bauble.angle.cos(), bauble.angle.sin());
                bauble.spin += (cursor_velocity.dot(along) / reach - bauble.spin) * (dt * 20.0).min(1.0);
            }
            bauble.spin *= (-1.1 * dt).exp();
            bauble.angle = (bauble.angle + bauble.spin * dt).clamp(-2.6, 2.6);
            self.baubles[k] = bauble;
        }
    }

    /// The garland twinkles, each bulb in its own time. Over the last ten seconds of the year it flashes all at once
    /// every second, the star with it; from midnight lights chase up it.
    fn twinkle(&mut self) {
        self.star_glow = 1.0;
        if let Some(t) = self.celebration
            && t < 0.0
        {
            let tick = t - t.floor();
            let flash = 0.1 + 0.9 * (-tick * 4.0).exp();
            for bulb in &mut self.bulbs {
                bulb.brightness = flash;
            }
            self.star_glow = 1.0 + if t > -3.5 { 2.2 } else { 1.2 } * (-tick * 3.0).exp();
            return;
        }
        if let Some(t) = self.celebration
            && t < 60.0
        {
            let time = self.time;
            for (k, bulb) in self.bulbs.iter_mut().enumerate() {
                let chase = (time * 9.0 - k as f32 * 0.7).sin().max(0.0);
                bulb.brightness = if t < 1.5 { 1.0 } else { 0.25 + 0.75 * chase * chase };
            }
            self.star_glow = 1.4 + 2.0 * (-t * 1.5).exp();
            return;
        }
        let time = self.time;
        for bulb in &mut self.bulbs {
            let wave = (time * bulb.rate + bulb.phase).sin().max(0.0);
            bulb.brightness = 0.5 + 0.5 * wave.powf(1.5);
        }
    }

    pub fn bulb_position(&self, bulb: &Bulb) -> V2 {
        self.point(bulb.bough, bulb.along) + self.boughs[bulb.bough].up * bulb.lift
    }

    // Snow

    /// The bough or present whose snow a flake falling from `from` to `to` at `x` lands in, if any: the highest one
    /// crossed that has room, its index and how far along it. Presents count after the boughs. Needles let some through.
    pub fn catcher(&mut self, x: f32, from: f32, to: f32, chance: f32) -> Option<(usize, f32)> {
        let local = x - self.base.x;
        if local <= TREE_EXTENT[0] || local >= TREE_EXTENT[2] || to >= self.base.y + TREE_HEIGHT || from <= self.base.y - 10.0 {
            return None;
        }
        let mut best: Option<(usize, f32, f32)> = None;
        for i in 0..self.boughs.len() {
            if self.boughs[i].capacity <= 0.0 || self.boughs[i].snow() >= self.boughs[i].capacity {
                continue;
            }
            let Some((y, along)) = self.perch_height(i, x) else { continue };
            if y <= from && y >= to && y > best.map_or(f32::NEG_INFINITY, |b| b.2) {
                best = Some((i, along, y));
            }
        }
        for (k, present) in self.presents.iter().enumerate() {
            if present.snow >= present.capacity() || present.lid >= 0.05 {
                continue;
            }
            let middle = self.base.x + present.offset;
            if (x - middle).abs() >= present.width / 2.0 {
                continue;
            }
            let y = present.bottom + present.height + 5.0 + present.snow / present.width;
            if y <= from && y >= to && y > best.map_or(f32::NEG_INFINITY, |b| b.2) {
                best = Some((self.boughs.len() + k, 0.0, y));
            }
        }
        let (index, along, _) = best?;
        if self.random.next() >= chance {
            return None;
        }
        Some((index, along))
    }

    /// The height of a bough's snow over `x`, and how far along the bough that is.
    fn perch_height(&self, i: usize, x: f32) -> Option<(f32, f32)> {
        let (from, to) = (0.12, 0.97);
        let (mut a, mut b) = (self.perch(i, from), self.perch(i, to));
        let mut ends = (from, to);
        if a.x > b.x {
            std::mem::swap(&mut a, &mut b);
            ends = (to, from);
        }
        let least = self.boughs[i].width * 0.6;
        if b.x - a.x < least {
            // Seen end on, the bough catches across its width, all of it about midway along.
            let middle = (a + b) * 0.5;
            let top = a.y.max(b.y);
            a = V2::new(middle.x - least / 2.0, top);
            b = V2::new(middle.x + least / 2.0, top);
            ends = (0.55, 0.55);
        }
        if x < a.x || x > b.x {
            return None;
        }
        let t = (x - a.x) / (b.x - a.x);
        Some((a.y + (b.y - a.y) * t, ends.0 + (ends.1 - ends.0) * t))
    }

    /// Lays `area` of snow where `catcher` found, spilling over into the cells beside it when one is full; something
    /// heavy falling onto a bough knocks it down.
    pub fn add(&mut self, area: f32, index: usize, along: f32, velocity: V2) {
        if index >= self.boughs.len() {
            let present = &mut self.presents[index - self.boughs.len()];
            present.snow = (present.snow + area).min(present.capacity());
            return;
        }
        let bough = &mut self.boughs[index];
        let cell = ((((along - SNOW_FROM) / (SNOW_TO - SNOW_FROM)) * CELL_COUNT as f32) as isize).clamp(0, CELL_COUNT as isize - 1);
        let most = bough.capacity / CELL_COUNT as f32 * 1.6;
        let mut left = area;
        for step in 0..CELL_COUNT as isize {
            if left <= 0.0 {
                break;
            }
            let spots: &[isize] = if step == 0 { &[cell] } else { &[cell - step, cell + step] };
            for &k in spots {
                if k < 0 || k >= CELL_COUNT as isize {
                    continue;
                }
                let room = (most - bough.cells[k as usize]).max(0.0);
                let laid = room.min(left);
                bough.cells[k as usize] += laid;
                left -= laid;
            }
        }
        let heft = (area / (bough.capacity * 0.08).max(1.0)).min(1.0);
        bough.velocity += velocity * (heft * 0.25 * along * along * 2.0);
    }

    pub fn clear_snow(&mut self, shed: &mut Vec<Shed>) {
        for i in 0..self.boughs.len() {
            if self.boughs[i].snow() <= 0.3 {
                continue;
            }
            for k in 0..CELL_COUNT {
                if self.boughs[i].cells[k] > 0.02 {
                    shed.push(Shed {
                        area: self.boughs[i].cells[k],
                        position: self.perch(i, cell_along(k)),
                        velocity: V2::new(0.0, -20.0),
                        spread: V2::new(3.0, 1.0),
                    });
                }
            }
            self.boughs[i].cells = [0.0; CELL_COUNT];
        }
        for present in &mut self.presents {
            present.snow = 0.0;
        }
    }

    // Santa's hand

    /// Where Santa's arm comes out of a present: inside its top, right of the middle, as its lid hinges on the left.
    pub fn shoulder(&self, present: &Present) -> V2 {
        V2::new(self.base.x + present.offset + present.jiggle + present.width * 0.12, present.bottom + present.height - 1.0)
    }

    /// The cursor close above a present: its lid rattles and lifts, and Santa's hand comes out and reaches for the
    /// cursor. Caught, the cursor gets its hand shaken; then the hand waves and goes back in, and the lid drops shut.
    fn tend_presents(&mut self, dt: f32, cursor: Option<V2>, shed: &mut Vec<Shed>) {
        let reach = 80.0;
        let shown = self.base.y > -20.0;
        let base = self.base;
        let chosen = cursor.and_then(|point| {
            if !shown {
                return None;
            }
            (0..self.presents.len())
                .filter(|&k| {
                    let present = &self.presents[k];
                    let x = base.x + present.offset;
                    let top = present.bottom + present.height;
                    (point.x - x).abs() < present.width / 2.0 + 28.0 && point.y > top - 4.0 && point.y < top + 95.0
                })
                .min_by(|&a, &b| (point.x - base.x - self.presents[a].offset).abs().total_cmp(&(point.x - base.x - self.presents[b].offset).abs()))
        });
        let cheering = self.celebration.is_some_and(|t| (0.0..55.0).contains(&t));
        self.sparklers.clear();
        let time = self.time;
        for k in 0..self.presents.len() {
            let mut present = self.presents[k];
            present.since += dt;
            if cheering && shown && present.phase != HandPhase::Cheering {
                present.phase = HandPhase::Cheering;
                present.since = 0.0;
                if present.arm == 0.0 {
                    present.hand = self.shoulder(&present);
                }
            }
            let wanted = chosen == Some(k) && time >= present.rested;
            let shoulder = self.shoulder(&present);
            let mut target = shoulder + V2::new(0.0, 6.0);
            if let Some(cursor) = cursor {
                let mut toward = cursor - shoulder;
                toward.y = toward.y.max(8.0);
                let length = toward.length();
                if length > reach {
                    toward *= reach / length;
                }
                target = shoulder + toward;
            }
            let caught = cursor.map_or(f32::INFINITY, |point| point.distance(present.hand));
            let mut lid_target = 0.0;
            let mut stiffness = 14.0;
            match present.phase {
                HandPhase::Cheering => {
                    // Midnight: every hand is out, waving a sparkler.
                    lid_target = 0.62;
                    present.arm = (present.arm + dt * 2.6).min(1.0);
                    present.grip = (present.grip + dt * 4.0).min(0.85);
                    let k = k as f32;
                    target = shoulder + V2::new((time * 1.3 + k * 2.0).sin() * 12.0, 40.0 + 5.0 * (time * 2.1 + k).sin());
                    present.wave = (time * 9.0 + k * 1.7).sin() * 0.4;
                    if present.arm > 0.6 {
                        let direction = (present.hand - shoulder + V2::new(0.0, 0.01)).normalized();
                        self.sparklers.push(present.hand + direction * 2.0 + V2::new(0.0, 9.0));
                    }
                    if !cheering {
                        present.phase = HandPhase::Retreating;
                        present.since = 0.0;
                    }
                }
                HandPhase::Shut => {
                    present.jiggle = 0.0;
                    if wanted {
                        present.phase = HandPhase::Rattling;
                        present.since = 0.0;
                    }
                }
                HandPhase::Rattling => {
                    present.jiggle = (present.since * 55.0).sin() * 1.1 * (1.0 - present.since / 0.45).max(0.0);
                    lid_target = 0.08;
                    if !wanted {
                        present.phase = HandPhase::Shut;
                    } else if present.since > 0.45 {
                        present.phase = HandPhase::Reaching;
                        present.since = 0.0;
                        present.jiggle = 0.0;
                        present.hand = shoulder;
                        present.hand_velocity = V2::ZERO;
                        if present.snow > 0.3 {
                            shed.push(Shed {
                                area: present.snow,
                                position: shoulder + V2::new(present.width * 0.4, 6.0),
                                velocity: V2::new(70.0, 40.0),
                                spread: V2::new(present.width * 0.3, 2.0),
                            });
                            present.snow = 0.0;
                        }
                    }
                }
                HandPhase::Reaching => {
                    lid_target = 0.62;
                    present.arm = (present.arm + dt * 2.6).min(1.0);
                    present.grip = (present.grip - dt * 6.0).max(0.0);
                    present.wave *= (-dt * 6.0).exp();
                    if !wanted {
                        present.phase = HandPhase::Retreating;
                        present.since = 0.0;
                    } else if present.arm > 0.9 && caught < 7.0 {
                        present.phase = HandPhase::Holding;
                        present.since = 0.0;
                    }
                }
                HandPhase::Holding => {
                    lid_target = 0.62;
                    stiffness = 30.0;
                    present.grip = (present.grip + dt * 7.0).min(1.0);
                    // A handshake, up and down a few times.
                    let shaking = ((present.since - 0.25) / 1.1).clamp(0.0, 1.0);
                    present.wave = (present.since * 15.0).sin() * 0.28 * if shaking > 0.0 && shaking < 1.0 { 1.0 } else { 0.0 };
                    if caught > 13.0 {
                        // Slipped away: the hand shakes its fist after it and reaches again.
                        present.phase = HandPhase::Reaching;
                        present.since = 0.0;
                        present.wave = 0.6;
                    } else if present.since > 1.5 {
                        present.phase = HandPhase::Retreating;
                        present.since = 0.0;
                    }
                }
                HandPhase::Retreating => {
                    present.grip = (present.grip - dt * 6.0).max(0.0);
                    // A wave goodbye, then back into the box.
                    let waving = present.since < 0.9;
                    present.wave = if waving { (present.since * 12.0).sin() * 0.45 } else { present.wave * (-dt * 8.0).exp() };
                    target = if waving { present.hand } else { shoulder };
                    if !waving {
                        present.arm = (present.arm - dt * 2.4).max(0.0);
                    }
                    lid_target = if present.arm > 0.05 { 0.62 } else { 0.0 };
                    if present.arm <= 0.0 && present.lid < 0.03 {
                        present.phase = HandPhase::Shut;
                        present.rested = time + 1.5;
                        present.bottom += 1.5;
                    }
                }
            }
            if present.phase != HandPhase::Shut {
                self.touched = true;
            }

            let omega = stiffness;
            let damping = 2.0 * 0.75 * omega;
            present.hand_velocity += ((target - present.hand) * (omega * omega) - present.hand_velocity * damping) * dt;
            present.hand += present.hand_velocity * dt;
            if present.phase == HandPhase::Shut || present.phase == HandPhase::Rattling {
                present.hand = shoulder;
            }

            let lid_omega = 16.0;
            present.lid_velocity += (lid_omega * lid_omega * (lid_target - present.lid) - 2.0 * 0.3 * lid_omega * present.lid_velocity) * dt;
            present.lid += present.lid_velocity * dt;
            if present.lid < 0.0 {
                present.lid = 0.0;
                present.lid_velocity = -present.lid_velocity * 0.35;
            }
            self.presents[k] = present;
        }
    }

    /// Each present sits on the drift under it, sunk into it a little.
    pub fn settle_presents(&mut self, surface: impl Fn(f32) -> f32, dt: f32) {
        let base = self.base;
        for present in &mut self.presents {
            let x = base.x + present.offset;
            let half = present.width * 0.3;
            let level = (surface(x - half) + surface(x) + surface(x + half)) / 3.0;
            let target = base.y.max(base.y + level - present.height * 0.3);
            present.bottom += (target - present.bottom) * (dt * 5.0).min(1.0);
        }
    }
}

/// Where the cursor touches a bough, if it does: how far along it, and how deep into its needles.
fn contact(bough: &Bough, root: V2, cursor: V2) -> Option<(f32, f32)> {
    let tip = bough.reach + bough.offset;
    let local = cursor - root;
    let along = (local.dot(tip) / tip.length_squared()).clamp(0.0, 1.0);
    let reach = bough.width * 0.5 * profile(along) * 0.9 + 4.0;
    let distance = local.distance(tip * along);
    if distance >= reach {
        return None;
    }
    Some((along, 1.0 - distance / reach))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints the tree's layout as the Mac's harness does, to compare: cargo test dump_tree -- --ignored --nocapture
    #[test]
    #[ignore]
    fn dump_tree() {
        let tree = Tree::new();
        println!("boughs {}", tree.boughs.len());
        for (i, b) in tree.boughs.iter().enumerate() {
            println!(
                "b {} {:.5} {:.4} {:.4} {:.4} {:.4} {:.4} {:.0} {:.5}",
                i, b.azimuth, b.height, b.length, b.width, b.reach.x, b.reach.y, b.variant, b.seed
            );
        }
        for b in &tree.baubles {
            println!("ball {} {:.5} {:.4} {:.4}", b.bough, b.along, b.string, b.radius);
        }
        for b in &tree.bulbs {
            println!("bulb {} {:.2} {:.4}", b.bough, b.along, b.lift);
        }
    }

    /// Snow falling on the tree lies on its boughs, and a bough shaken hard enough throws it off.
    #[test]
    fn boughs_catch_and_shed_snow() {
        let mut tree = Tree::new();
        tree.base = V2::new(124.0, 0.0);
        let mut caught = 0;
        for k in 0..400 {
            let x = 124.0 - 60.0 + (k % 120) as f32;
            if let Some((index, along)) = tree.catcher(x, 260.0, 0.0, 1.0) {
                tree.add(4.0, index, along, V2::new(0.0, -100.0));
                caught += 1;
            }
        }
        assert!(caught > 300, "caught {caught}");
        let held: f32 = tree.boughs.iter().map(Bough::snow).sum::<f32>() + tree.presents.iter().map(|p| p.snow).sum::<f32>();
        assert!(held > 1000.0, "held {held}");
        // Calm, it keeps its snow.
        let mut shed = Vec::new();
        for _ in 0..120 {
            tree.step(1.0 / 60.0, 0.0, None, V2::ZERO, false, &mut shed);
        }
        let calm: f32 = shed.iter().map(|piece| piece.area).sum();
        assert!(calm < held * 0.2, "lost {calm} of {held} in a calm");
        // Swept with the cursor across its middle, it loses much of it.
        shed.clear();
        for frame in 0..60 {
            let x = 124.0 - 90.0 + frame as f32 * 3.0;
            tree.step(1.0 / 60.0, 0.0, Some(V2::new(x, 70.0)), V2::new(180.0, 0.0), false, &mut shed);
        }
        let swept: f32 = shed.iter().map(|piece| piece.area).sum();
        assert!(swept > 50.0, "swept {swept}");
    }
}
