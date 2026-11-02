//! Rain: raindrops bursting on the windows and the bottom of the screen, and drops gathering on a window's top edge
//! to run down its glass. A running drop leaves a trail of beads, hangs under the window's bottom edge until it is
//! heavy enough to fall, and can be smeared or knocked off by the cursor or shaken off with its window.
//! The same as the Mac app's.

use std::collections::HashMap;

use super::{Air, CURSOR_RADIUS, Flake, SnowSimulation, Sprite, WindowId};
use crate::geometry::{V2, smoothstep};
use crate::layer::SnowLayer;

const RAIN_GRAVITY: f32 = 900.0;
const MAX_SPLASHES: usize = 1500;
/// The share of raindrops hitting a window's top edge that gather into a drop running on down its face.
const RUN_SHARE: f32 = 0.07;
const MAX_GLASS_DROPS: usize = 600;
const BEAD_DRYING: f32 = 0.3;
const SMALLEST_RUNNING: f32 = 1.6;
/// Drops on the glass are drawn this much bigger than the water they hold.
const GLASS_SCALE: f32 = 1.6;
/// A drop running down the glass is drawn bigger still than the beads it leaves and the drops hanging below; the
/// biggest no bigger than they were, or they would stand out.
fn running_scale(radius: f32) -> f32 {
    1.2 - 0.2 * smoothstep(3.0, 4.6, radius)
}
/// How hard a window has to be shaken back and forth, in its swings against gravity, to shake off a drop of radius
/// 2.4: far harder than its snow. A smaller one holds on harder, a hanging one less.
const DROP_SHAKING: f32 = 22.0;
/// A drop hanging under a window this big lets go; a smaller one clings on, waiting for more, and dries up.
const HEAVY_HANGING: f32 = 2.6;
/// However much runs together, a drop on the glass grows no bigger than this.
const LARGEST_DROP: f32 = 6.0;
const HANGING_DRYING: f32 = 0.1;
/// A hanging drop pinches off far faster than it fills: only a few frames show it stretch. Not all of it falls.
const PINCHING: f32 = 0.06;
const FALLING_SHARE: f32 = 0.6;

/// A droplet thrown up where a raindrop bursts on a window's edge or the bottom of the screen.
#[derive(Clone, Copy)]
struct Splash {
    position: V2,
    velocity: V2,
    size: f32,
    alpha: f32,
    age: f32,
    life: f32,
}

#[derive(Clone, Copy)]
struct GlassDrop {
    layer: WindowId,
    /// From the left end of the window's top edge.
    offset: V2,
    /// How far below the window's top edge its bottom edge is.
    face: f32,
    radius: f32,
    speed: f32,
    /// Sideways for each point down: a running drop wanders a little.
    slope: f32,
    age: f32,
    /// How far it has run since it last left a bead, and how far it runs before it leaves the next.
    travelled: f32,
    spacing: f32,
    /// Caught under the window's bottom edge, it clings there, taking in what runs down onto it, until it is heavy
    /// enough to let go: how long it has hung, how full it is, from a flat cap to a pear, and when it lets go, once
    /// it starts to.
    hanging: bool,
    hung: f32,
    fill: f32,
    lets_go: f32,
    /// Left behind by a running drop, or run out of water: it holds still and dries up.
    bead: bool,
}

impl GlassDrop {
    fn new(layer: WindowId, offset: V2, face: f32, radius: f32) -> GlassDrop {
        GlassDrop {
            layer,
            offset,
            face,
            radius,
            speed: 0.0,
            slope: 0.0,
            age: 0.0,
            travelled: 0.0,
            spacing: 0.0,
            hanging: false,
            hung: 0.0,
            fill: 0.0,
            lets_go: f32::INFINITY,
            bead: false,
        }
    }

    fn bead(layer: WindowId, offset: V2, face: f32, radius: f32) -> GlassDrop {
        GlassDrop { bead: true, ..GlassDrop::new(layer, offset, face, radius) }
    }

    /// How far along a hanging drop is, as the shader draws it: up to one it fills, from a flat cap under the edge to
    /// a pear; from one to two it lets go.
    fn stage(&self) -> f32 {
        self.fill + if self.lets_go.is_finite() { smoothstep(self.lets_go - PINCHING, self.lets_go, self.hung) } else { 0.0 }
    }

    /// How far it reaches along the edge each way while it hangs, as the shader draws it.
    fn spread(&self) -> f32 {
        (1.52 - 0.38 * self.fill.min(1.0)) * self.radius * GLASS_SCALE
    }
}

#[derive(Default)]
pub(super) struct Rain {
    splashes: Vec<Splash>,
    drops: Vec<GlassDrop>,
}

impl Rain {
    pub(super) fn sprite_count(&self) -> usize {
        self.splashes.len() + self.drops.len()
    }
}

impl SnowSimulation {
    /// A raindrop landing at `x` on layer `li`: it bursts into a droplet or two, and now and then, on a window's top
    /// edge, gathers into a drop that runs down its face. A drop come off the glass above, carrying `water`, always
    /// runs on down the window it lands on.
    pub(super) fn rain_landed(&mut self, li: usize, x: f32, surface: f32, depth: f32, water: f32) {
        self.splash(V2::new(x, surface), depth);
        if li > 0 && (water > 0.0 || self.air.random.next() < RUN_SHARE) {
            self.run_down(li, x, water);
        }
    }

    fn splash(&mut self, point: V2, depth: f32) {
        let random = &mut self.air.random;
        let count = 1 + (random.next() * (1.0 + depth * 2.0)) as usize;
        for _ in 0..count {
            if self.rain.splashes.len() >= MAX_SPLASHES {
                break;
            }
            let angle = random.range(0.3, 1.2) * if random.next() < 0.5 { -1.0 } else { 1.0 };
            let speed = random.range(45.0, 120.0) * (0.6 + 0.6 * depth);
            self.rain.splashes.push(Splash {
                position: point,
                velocity: V2::new(angle.sin(), angle.cos()) * speed,
                size: random.range(0.8, 1.4) * (0.7 + 0.5 * depth),
                alpha: 0.25 + 0.3 * depth,
                age: 0.0,
                life: random.range(0.2, 0.4),
            });
        }
    }

    fn run_down(&mut self, li: usize, x: f32, water: f32) {
        if self.rain.drops.len() >= MAX_GLASS_DROPS {
            return;
        }
        let ledge = &self.layers[li];
        let random = &mut self.air.random;
        let radius = if water > 0.0 {
            water.max(SMALLEST_RUNNING)
        } else {
            let u = random.next();
            if random.next() < 0.1 { random.range(3.8, 4.6) } else { 2.2 + 2.2 * u * u }
        };
        let along = x - ledge.origin.x;
        let mut drop = GlassDrop::new(self.ids[li], V2::new(along, -corner(ledge, along) - radius * 1.4), ledge.face, radius);
        drop.slope = random.range(-0.12, 0.12);
        drop.spacing = trail_spacing(random.range(4.0, 9.0), radius);
        self.rain.drops.push(drop);
    }

    pub(super) fn advance_splashes(&mut self, dt: f32) {
        self.rain.splashes.retain_mut(|splash| {
            splash.age += dt;
            splash.velocity.y -= RAIN_GRAVITY * dt;
            splash.position += splash.velocity * dt;
            splash.age < splash.life
        });
    }

    /// A drop runs down its window, hurrying over clean patches and hesitating on greasy ones, wandering a little
    /// from side to side. It leaves a trail of beads behind and shrinks with each, until too little is left to run
    /// and it holds still as a bead itself; beads dry up. A running drop swallows the beads in its way.
    pub(super) fn run_glass_drops(&mut self, dt: f32) {
        if self.rain.drops.is_empty() {
            return;
        }
        let shown: HashMap<WindowId, usize> = self.ids.iter().enumerate().skip(1).map(|(li, &id)| (id, li)).collect();
        let shaking = self.air.clock - self.space_changed_at > 0.6;
        let air = &mut self.air;
        let drops = &mut self.rain.drops;
        let mut i = drops.len();
        while i > 0 {
            i -= 1;
            let mut drop = drops[i];
            // Taken in by another drop since it last ran: nothing is left of it. Run on, it would take its water back.
            if drop.radius <= 0.0 {
                drops.swap_remove(i);
                continue;
            }
            let Some(&li) = shown.get(&drop.layer) else {
                drops.swap_remove(i);
                continue;
            };
            let layer = &self.layers[li];
            drop.age += dt;
            if !drop.hanging
                && let Some(point) = air.cursor
                && wipes(air, point, &drop, layer, dt)
            {
                if drop.radius >= 1.0 {
                    smear(air, &drop, layer, point, drops);
                }
                drop.radius = 0.0;
            } else if shaking && shaken(air, &drop, layer, dt) {
                // It flies on as the window was going.
                let below = if drop.hanging { drop.radius * GLASS_SCALE } else { 0.0 };
                fall(air, &drop, layer, 1.0, below, Throw::Shaken(layer.track_velocity));
                drop.radius = 0.0;
            } else if drop.hanging {
                cling(air, &mut drop, i, layer, dt, drops);
            } else if drop.bead {
                drop.radius -= BEAD_DRYING * dt;
            } else {
                run(air, &mut drop, i, layer, dt, drops);
            }
            let gone = drop.radius < 0.25 || (!drop.hanging && -drop.offset.y > drop.face) || drop.offset.x < 0.0 || drop.offset.x > layer.width();
            if gone {
                drops.swap_remove(i);
            } else {
                drops[i] = drop;
            }
        }
    }

    pub(super) fn rain_sprites(&self) -> impl Iterator<Item = Sprite> + '_ {
        let splashes = self.rain.splashes.iter().map(|splash| Sprite {
            position: splash.position,
            velocity: splash.velocity,
            size: splash.size,
            alpha: splash.alpha * (1.0 - splash.age / splash.life),
            soft: 0.5,
            rain: [1.0, 0.0],
            ..Sprite::default()
        });
        let shown: HashMap<WindowId, usize> = self.ids.iter().enumerate().skip(1).map(|(li, &id)| (id, li)).collect();
        let drops = self.rain.drops.iter().filter_map(move |drop| {
            let layer = &self.layers[*shown.get(&drop.layer)?];
            let position = layer.origin + drop.offset;
            // A window in front of the one it runs down hides it.
            if hidden(layer, position) {
                return None;
            }
            // It swells out of nothing as it gathers on the edge.
            let grown = if drop.bead { 1.0 } else { (drop.age / 0.15).sqrt().min(1.0) };
            Some(Sprite {
                position,
                velocity: if drop.hanging || drop.bead { V2::ZERO } else { V2::new(drop.slope, -1.0) * drop.speed },
                size: drop.radius.max(1.2) * 2.0 * GLASS_SCALE * if drop.hanging || drop.bead { 1.0 } else { running_scale(drop.radius) } * grown,
                // Drying up, a hanging drop fades rather than shrinking past the smallest drawn.
                alpha: (drop.radius / if drop.hanging { 1.2 } else { 0.35 }).min(1.0),
                soft: if self.shelf == Some(drop.layer) { 1.0 } else { 0.0 },
                rain: if drop.hanging { [3.0, drop.stage()] } else { [2.0, 0.0] },
                ..Sprite::default()
            })
        });
        splashes.chain(drops)
    }
}

fn run(air: &mut Air, drop: &mut GlassDrop, index: usize, layer: &SnowLayer, dt: f32, drops: &mut Vec<GlassDrop>) {
    let random = &mut air.random;
    let excess = (drop.radius - 1.2).max(0.0);
    drop.speed += ((60.0 + 45.0 * excess.powf(1.4)).min(700.0) * grip(drop.offset) - drop.speed) * (dt * 5.0).min(1.0);
    // A window dragged along under it leaves it trailing a little behind; its own wandering stays within less.
    let trail = -layer.track_velocity.x * 0.0006;
    let wander = (drop.slope + (random.range(-1.0, 1.0) * 1.6 + trail) * dt) * (1.0 - dt * 0.8);
    drop.slope = wander.clamp(-0.45, 0.45);
    let heading = V2::new(drop.slope, -1.0) / (1.0 + drop.slope * drop.slope).sqrt();
    let advance = drop.speed * dt;
    drop.offset += V2::new(drop.slope, -1.0) * advance;

    drop.travelled += advance;
    if drop.travelled >= drop.spacing {
        drop.travelled = 0.0;
        drop.spacing = trail_spacing(random.range(4.0, 9.0), drop.radius);
        let left = drop.radius * random.range(0.15, 0.25);
        drop.radius = (drop.radius * drop.radius - left * left).max(0.0).sqrt();
        if drops.len() < MAX_GLASS_DROPS {
            drops.push(GlassDrop::bead(drop.layer, drop.offset - heading * (drop.radius * 0.95 + left * 0.4), drop.face, left));
        }
    }

    // What lies ahead on the same window joins it; its own trail is behind it.
    let mut hanging: Option<GlassDrop> = None;
    for (j, other) in drops.iter_mut().enumerate() {
        if j == index || other.layer != drop.layer || other.radius <= 0.0 {
            continue;
        }
        let offset = other.offset - drop.offset;
        if offset.dot(heading) <= -drop.radius * 0.2 || offset.length() >= (drop.radius + other.radius) * 0.9 {
            continue;
        }
        if other.hanging {
            hanging = Some(*other);
        }
        drop.radius = (drop.radius * drop.radius + other.radius * other.radius).sqrt().min(LARGEST_DROP);
        other.radius = 0.0;
    }
    // Run into a drop hanging under the edge, it flows down into it.
    if let Some(hanging) = hanging {
        drop.offset = hanging.offset;
        hang(air, drop, layer, hanging.fill);
    } else if -drop.offset.y >= bottom(drop, layer) {
        hang(air, drop, layer, 0.35);
    } else if drop.radius < SMALLEST_RUNNING {
        drop.bead = true;
    }
}

/// Run down to the window's bottom edge, a drop wraps round it and hangs under it, settling into the shape its water
/// makes there. Joining one already hanging there, it hangs as that one did.
fn hang(air: &mut Air, drop: &mut GlassDrop, layer: &SnowLayer, fill: f32) {
    let speed = drop.speed;
    drop.hanging = true;
    drop.speed = 0.0;
    drop.slope = 0.0;
    drop.offset.y = -bottom(drop, layer);
    drop.hung = 0.0;
    drop.fill = fill;
    drop.lets_go = f32::INFINITY;
    // More water than the edge can hold runs straight on over it and falls; the edge keeps what it can.
    if drop.radius > HEAVY_HANGING {
        let kept = HEAVY_HANGING * 0.8;
        let share = 1.0 - (kept * kept) / (drop.radius * drop.radius);
        fall(air, drop, layer, share, drop.radius * GLASS_SCALE, Throw::Flicked(V2::new(0.0, -2.0 * speed.max(150.0))));
        drop.radius = kept;
    }
}

/// A hanging drop sags further the more water it holds. Heavy enough, it pinches off in an instant: the lower part
/// falls and a smaller drop stays on the edge to fill again. Until then it dries up slowly, and runs together with any
/// other hanging beside it close enough to touch.
fn cling(air: &mut Air, drop: &mut GlassDrop, index: usize, layer: &SnowLayer, dt: f32, drops: &mut [GlassDrop]) {
    drop.hung += dt;
    // Where the bottom edge rounds up into a corner, it is too steep to hang from: the drop runs along it, down to
    // where it levels out.
    let x = drop.offset.x;
    let steepness = (corner(layer, x + 1.0) - corner(layer, x - 1.0)) / 2.0;
    if steepness.abs() > 0.3 {
        drop.offset.x += if steepness > 0.0 { -70.0 } else { 70.0 } * dt;
        drop.offset.y = -bottom(drop, layer);
    }
    // The cursor brushing it knocks it off, all of it, flicked along the way the cursor goes.
    if let Some(point) = air.cursor
        && touches(point, drop, layer)
    {
        let below = (0.6 + 1.2 * drop.fill) * drop.radius * GLASS_SCALE;
        let flick = air.cursor_velocity;
        fall(air, drop, layer, 1.0, below, Throw::Flicked(flick));
        drop.radius = 0.0;
        return;
    }
    // It stays a flat cap for most of its filling and sags only as it gets close to letting go.
    let full = ((drop.radius - 1.4) / (HEAVY_HANGING - 1.4)).clamp(0.0, 1.0);
    drop.fill += (full * full * full - drop.fill) * (dt * 6.0).min(1.0);
    if drop.lets_go.is_finite() {
        if drop.hung < drop.lets_go {
            return;
        }
        fall(air, drop, layer, FALLING_SHARE, 2.6 * drop.radius * GLASS_SCALE, Throw::Dropped);
        drop.radius *= (1.0 - FALLING_SHARE).sqrt();
        drop.lets_go = f32::INFINITY;
        return;
    }
    if drop.radius >= HEAVY_HANGING {
        drop.lets_go = drop.hung + PINCHING;
        return;
    }
    drop.radius -= HANGING_DRYING * dt;
    for (j, other) in drops.iter_mut().enumerate() {
        if j == index || !other.hanging || other.layer != drop.layer || other.radius <= 0.0 || other.lets_go.is_finite() {
            continue;
        }
        if (other.offset.x - drop.offset.x).abs() >= (drop.spread() + other.spread()) * 0.7 {
            continue;
        }
        let area = drop.radius * drop.radius;
        let added = other.radius * other.radius;
        drop.offset.x = (drop.offset.x * area + other.offset.x * added) / (area + added);
        drop.radius = (area + added).sqrt().min(LARGEST_DROP);
        drop.fill = drop.fill.max(other.fill);
        other.radius = 0.0;
    }
}

/// Whether shaking its window back and forth shakes a drop off the glass this step: the harder the shaking and the
/// bigger the drop, the likelier. A single start or stop doesn't.
fn shaken(air: &mut Air, drop: &GlassDrop, layer: &SnowLayer, dt: f32) -> bool {
    let hold = DROP_SHAKING * 2.4 / drop.radius.max(0.3) * if drop.hanging { 0.7 } else { 1.0 };
    layer.agitation > hold && air.random.next() < (0.3 * (layer.agitation - hold)).min(0.9) * dt * 10.0
}

/// Whether the cursor, moving over the glass, passed over a drop on it this step where it shows.
fn wipes(air: &Air, point: V2, drop: &GlassDrop, layer: &SnowLayer, dt: f32) -> bool {
    // What it has just smeared out it passes on over.
    let velocity = air.cursor_velocity;
    if velocity.length() <= 40.0 || (drop.bead && drop.age <= 0.3) {
        return false;
    }
    let position = layer.origin + drop.offset;
    let from = point - velocity * dt;
    let path = point - from;
    let along = ((position - from).dot(path) / path.length_squared().max(0.0001)).clamp(0.0, 1.0);
    let reach = CURSOR_RADIUS * 0.8 + drop.radius.max(1.2) * GLASS_SCALE;
    (position - (from + path * along)).length() < reach && !hidden(layer, position)
}

/// The cursor drags a drop's water along with it, up to where it is now, in a streak that at once beads up into a
/// line of droplets, biggest where the drop was; some of the water it takes away. The tiniest it just wipes off.
fn smear(air: &mut Air, drop: &GlassDrop, layer: &SnowLayer, point: V2, drops: &mut Vec<GlassDrop>) {
    let velocity = air.cursor_velocity;
    let direction = velocity / velocity.length();
    let across = V2::new(-direction.y, direction.x);
    let length = (point - (layer.origin + drop.offset)).dot(direction).clamp(6.0, 45.0);
    let count = ((length / 5.0) as usize).clamp(2, 9);
    let random = &mut air.random;
    let spots: Vec<f32> = (0..count).map(|k| (k as f32 + random.range(0.2, 0.8)) / count as f32).collect();
    let total: f32 = spots.iter().map(|spot| 1.15 - spot).sum();
    let water = drop.radius * drop.radius * 0.7;
    for spot in spots {
        if drops.len() >= MAX_GLASS_DROPS {
            break;
        }
        let offset = drop.offset + direction * length * spot + across * random.range(-1.2, 1.2);
        drops.push(GlassDrop::bead(drop.layer, offset, drop.face, (water * (1.15 - spot) / total).sqrt().max(0.3)));
    }
}

/// The part of a drop that lets go falls like any raindrop, to burst on whatever is below, and runs on down a window
/// it lands on.
fn fall(air: &mut Air, drop: &GlassDrop, layer: &SnowLayer, share: f32, below: f32, throw: Throw) {
    let position = layer.origin + drop.offset - V2::new(0.0, below);
    if !air.rain || position.y <= 2.0 {
        return;
    }
    let (velocity, fastest, start) = match throw {
        // Free of the edge, it is falling fast within a few hundredths of a second.
        Throw::Dropped => (V2::ZERO, 600.0, -300.0),
        Throw::Flicked(velocity) => (velocity * 0.5, 600.0, -120.0),
        Throw::Shaken(velocity) => (velocity, 1200.0, -60.0),
    };
    let speed = velocity.length();
    let velocity = if speed > fastest { velocity * (fastest / speed) } else { velocity };
    let fall_speed = air.random.range(850.0, 1100.0);
    air.flakes.push(Flake {
        position,
        velocity: velocity + V2::new(0.0, start),
        fall_speed,
        size: 1.4 + drop.radius * share.sqrt() * 0.25,
        depth: 1.0,
        alpha: 0.45,
        soft: 0.6,
        phase: 0.0,
        sway_rate: 0.0,
        sway_amount: 0.0,
        settles: true,
        mass: 1.0,
        tint: [1.0; 3],
        tint_amount: 0.0,
        blown: 0.0,
        leaf: 0.0,
        rain: true,
        water: drop.radius * share.sqrt(),
    });
}

/// How a drop comes off the glass: letting go of an edge, knocked off by the cursor, or shaken off its window.
#[derive(Clone, Copy)]
enum Throw {
    Dropped,
    Flicked(V2),
    Shaken(V2),
}

fn trail_spacing(random: f32, radius: f32) -> f32 {
    random * radius / 4.0
}

/// How far the window's rounded corners bring its edges in at `x` along it: its top edge down, its bottom edge up.
fn corner(layer: &SnowLayer, x: f32) -> f32 {
    let last = layer.count() - 1;
    let f = (x / layer.column_width - 0.5).clamp(0.0, last as f32);
    let i = f as usize;
    let j = (i + 1).min(last);
    -(layer.bed[i] + (layer.bed[j] - layer.bed[i]) * (f - i as f32))
}

/// How far below the window's top edge its bottom edge is, where a drop is on it.
fn bottom(drop: &GlassDrop, layer: &SnowLayer) -> f32 {
    drop.face - corner(layer, drop.offset.x)
}

/// Whether a window in front of the one a drop is on covers `position`.
fn hidden(layer: &SnowLayer, position: V2) -> bool {
    layer.face_occluders.iter().any(|o| o[0] < position.x && o[2] > position.x && o[1] < position.y && o[3] > position.y)
}

/// Whether the cursor, at `point`, brushes a hanging drop where it shows.
fn touches(point: V2, drop: &GlassDrop, layer: &SnowLayer) -> bool {
    let cling = layer.origin + drop.offset;
    let depth = (1.0 + 1.2 * drop.fill) * drop.radius * GLASS_SCALE;
    (point.x - cling.x).abs() < drop.spread() + CURSOR_RADIUS * 0.7
        && point.y < cling.y + CURSOR_RADIUS * 0.5
        && point.y > cling.y - depth - CURSOR_RADIUS
        && !hidden(layer, cling - V2::new(0.0, 1.0))
}

/// Clean and greasy patches on a window: drops hurry across some and hesitate on others.
fn grip(point: V2) -> f32 {
    let q = point * 0.045;
    let base = V2::new(q.x.floor(), q.y.floor());
    let f = q - base;
    let u = V2::new(f.x * f.x * (3.0 - 2.0 * f.x), f.y * f.y * (3.0 - 2.0 * f.y));
    let hash = |x: f32, y: f32| {
        let mut n = (x as i64 as u32).wrapping_mul(374_761_393).wrapping_add((y as i64 as u32).wrapping_mul(668_265_263));
        n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
        (n & 0xFFFF) as f32 / 65535.0
    };
    let bottom = hash(base.x, base.y) + (hash(base.x + 1.0, base.y) - hash(base.x, base.y)) * u.x;
    let top = hash(base.x, base.y + 1.0) + (hash(base.x + 1.0, base.y + 1.0) - hash(base.x, base.y + 1.0)) * u.x;
    0.35 + 0.65 * (bottom + (top - bottom) * u.y)
}
