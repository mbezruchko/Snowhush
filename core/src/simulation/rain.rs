//! Rain: raindrops bursting on the windows and the bottom of the screen, spattering spray on the glass under a window's
//! top edge, and drops gathering on that edge to run down its glass. A running drop catches on specks and takes in the
//! spray it touches, leaves a film and a trail of beads, hangs under the window's bottom edge until it is heavy enough
//! to fall, and can be smeared or knocked off by the cursor or shaken off with its window.
//! The same as the Mac app's.

use std::collections::HashMap;

use super::{Air, CURSOR_RADIUS, Flake, Mode, SnowSimulation, Sprite, WindowId};
use crate::geometry::{V2, smoothstep};
use crate::layer::{Droplet, FilmPiece, SnowLayer};

const RAIN_GRAVITY: f32 = 900.0;
const MAX_SPLASHES: usize = 1500;
/// The share of raindrops hitting a window's top edge that gather into a drop running on down its face.
const RUN_SHARE: f32 = 0.07;
const MAX_GLASS_DROPS: usize = 600;
const BEAD_DRYING: f32 = 0.3;
const SMALLEST_RUNNING: f32 = 1.6;
/// Drops on the glass are drawn this much bigger than the water they hold.
const GLASS_SCALE: f32 = 1.6;
/// Drawn as glass, lit along its rim and with a shadow below, a running drop or a bead looks bigger than drawn flat:
/// it is drawn this much smaller, and a running drop reaches no further each way than the widest.
const GLASSY: f32 = 0.75;
const WIDEST_RUNNING: f32 = 3.5;
/// A drop running down the glass is drawn bigger still than the beads it leaves and the drops hanging below; the
/// biggest no bigger than they were, or they would stand out.
fn running_scale(radius: f32) -> f32 {
    1.2 - 0.2 * smoothstep(3.0, 4.6, radius)
}
/// How far a drop of `radius` reaches each way as drawn: one hanging, if `still`, or one running down the glass.
fn half_width(radius: f32, still: bool) -> f32 {
    let flat = radius.max(1.2) * GLASS_SCALE;
    if still {
        return flat;
    }
    // Past a size, bigger ones grow ever less, nearing the widest but still each its own size.
    const KNEE: f32 = 2.5;
    let half = flat * running_scale(radius) * GLASSY;
    if half <= KNEE {
        return half;
    }
    KNEE + (WIDEST_RUNNING - KNEE) * (1.0 - (-(half - KNEE) / (WIDEST_RUNNING - KNEE) * 0.8).exp())
}
/// How far a bead reaches each way as drawn: a small one bigger than its water, or it would be lost, the smallest
/// smaller still and a big one bigger, up to the size of a drop of its water.
fn bead_width(radius: f32) -> f32 {
    radius.max((radius * 2.4).min(1.2 + (radius - 0.5) * 0.5)) * GLASS_SCALE * GLASSY
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

/// Droplets of spray on all the windows at most, and on one for each point of its top edge.
const MAX_SPRAY: usize = 6000;
const SPRAY_ALONG: f32 = 3.0;
/// How far below a window's top edge its spray lands: most of it within the first, none past the second.
const SPRAY_REACH: f32 = 9.0;
const SPRAY_DEEPEST: f32 = 36.0;
const LARGEST_SPRAY: f32 = 0.9;
const SPRAY_DRYING: f32 = 0.003;
/// How much of the spray a running drop runs into it keeps: the rest it leaves on the glass behind it.
const SPRAY_KEPT: f32 = 0.35;
/// A running drop lays down its film in pieces this long, as wide as this share of the drop, that dry in a few seconds.
const FILM_PIECE: f32 = 4.0;
const FILM_SHARE: f32 = 0.4;
const FILM_LIFE: (f32, f32) = (2.0, 3.5);
const MAX_FILM: usize = 4000;
/// Specks that catch a running drop for a moment: one patch of glass this big in this many has one.
const SPECK_SPACING: f32 = 6.0;
const SPECK_SHARE: f32 = 0.05;
/// How far a running drop turns toward the water that joins it from one side, for the share of it that water is.
const TURNING: f32 = 1.0;

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
pub(super) struct GlassDrop {
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
    /// Where the film it leaves behind as it runs was last laid down to.
    mark: V2,
    /// Caught on a speck on the glass: how much longer it holds there, and the patch of glass it last ran onto.
    held: f32,
    patch: (i32, i32),
    /// Picks its outline: where its edge clings to the glass, it is never quite round.
    seed: f32,
    /// How hard it squashed and stretched when it last ran into more water or broke free, and when.
    jiggle: f32,
    jolted: f32,
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
            mark: offset,
            held: 0.0,
            patch: (i32::MIN, i32::MIN),
            seed: 0.0,
            jiggle: 0.0,
            jolted: 0.0,
        }
    }

    fn bead(layer: WindowId, offset: V2, face: f32, radius: f32, seed: f32) -> GlassDrop {
        GlassDrop { bead: true, seed, ..GlassDrop::new(layer, offset, face, radius) }
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
    /// Takes the drops on a window's glass off, to go along with the window to another display.
    pub(super) fn take(&mut self, id: WindowId) -> Vec<GlassDrop> {
        let (taken, kept) = self.drops.drain(..).partition(|drop| drop.layer == id);
        self.drops = kept;
        taken
    }

    /// Puts drops taken off a window's glass on another display onto it here; `scale` takes that display's pixels to
    /// these.
    pub(super) fn adopt(&mut self, drops: Vec<GlassDrop>, scale: f32) {
        self.drops.retain(|drop| !drops.iter().any(|other| other.layer == drop.layer));
        self.drops.extend(drops.into_iter().map(|mut drop| {
            drop.offset *= scale;
            drop.face *= scale;
            drop
        }));
    }

    pub(super) fn sprite_count(&self) -> usize {
        self.splashes.len() + self.drops.len()
    }
}

impl SnowSimulation {
    /// Whether any drop runs down the glass, or any bead lies on it: those are drawn as glass, from a field the
    /// renderer adds them up into.
    pub fn has_glass_drops(&self) -> bool {
        self.rain.drops.iter().any(|drop| !drop.hanging)
    }

    /// A raindrop landing at `x` on layer `li`: it bursts into a droplet or two, spatters a window's glass with spray
    /// and now and then, on a window's top edge, gathers into a drop that runs down its face. A drop come off the glass
    /// above, carrying `water`, always runs on down the window it lands on.
    pub(super) fn rain_landed(&mut self, li: usize, x: f32, surface: f32, depth: f32, water: f32) {
        self.splash(V2::new(x, surface), depth);
        if li > 0 {
            self.spray(li, x, depth);
            if water > 0.0 || self.air.random.next() < RUN_SHARE {
                self.run_down(li, x, water);
            }
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

    /// Rain bursting on a window's top edge spatters its face with spray: most right under the edge, less and less
    /// further down. A droplet landing on one already there runs together with it; on a window with all the spray it
    /// holds, it takes the place of another.
    fn spray(&mut self, li: usize, x: f32, depth: f32) {
        let total: usize = self.layers.iter().map(|layer| layer.spray.len()).sum();
        let random = &mut self.air.random;
        let count = (random.next() * (4.0 + 16.0 * depth)) as usize;
        if count == 0 {
            return;
        }
        let ledge = &mut self.layers[li];
        let width = ledge.width();
        let deepest = SPRAY_DEEPEST.min(ledge.face * 0.4);
        let room = total < MAX_SPRAY && (ledge.spray.len() as f32) < width * SPRAY_ALONG;
        for _ in 0..count {
            let along = x - ledge.origin.x + random.range(-1.0, 1.0) * (8.0 + 16.0 * depth);
            let below = -(1.0 - random.next() * 0.999).ln() * SPRAY_REACH;
            if along <= 2.0 || along >= width - 2.0 || below >= deepest {
                continue;
            }
            let u = random.next();
            let radius = 0.3 + 0.5 * u * u;
            let offset = V2::new(along, -corner(ledge, along) - below - radius * GLASS_SCALE);
            if let Some(near) = ledge.spray.iter_mut().find(|near| (near.offset - offset).length() < (near.radius + radius) * GLASS_SCALE) {
                let area = near.radius * near.radius;
                let added = radius * radius;
                near.offset = (near.offset * area + offset * added) / (area + added);
                near.radius = (area + added).sqrt().min(LARGEST_SPRAY);
            } else if room {
                ledge.spray.push(Droplet { offset, radius });
            } else if !ledge.spray.is_empty() {
                let k = (random.next() * ledge.spray.len() as f32) as usize % ledge.spray.len();
                ledge.spray[k] = Droplet { offset, radius };
            }
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
        }
        .min(largest_running(ledge.face));
        let along = x - ledge.origin.x;
        // It gathers right under the edge: its top, once it has swelled to its size, at the edge.
        let edge = -corner(ledge, along);
        let mut drop = GlassDrop::new(self.ids[li], V2::new(along, edge - half_width(radius, false)), ledge.face, radius);
        drop.slope = random.range(-0.12, 0.12);
        drop.spacing = trail_spacing(random.next(), radius);
        drop.mark = V2::new(along, edge);
        drop.seed = random.next().max(0.001);
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
        let mut film: usize = self.layers.iter().map(|layer| layer.film.len()).sum();
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
            let layer = &mut self.layers[li];
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
                run(air, &mut drop, i, layer, dt, drops, &mut film);
            }
            let smallest = if drop.bead { 0.15 } else { 0.25 };
            let gone =
                drop.radius < smallest || (!drop.hanging && -drop.offset.y > drop.face) || drop.offset.x < 0.0 || drop.offset.x > layer.width();
            if gone {
                drops.swap_remove(i);
            } else {
                drops[i] = drop;
            }
        }
    }

    /// Spray dries up slowly in the rain and soon once it stops, a running drop's film in a few seconds, and both fast
    /// once the rain stops. The cursor moving over the glass wipes them off where it passes, as a finger would.
    pub(super) fn tend_glass(&mut self, dt: f32) {
        let hurry = if self.air.raining() { 1.0 } else { 12.0 };
        let velocity = self.air.cursor_velocity;
        let wiping = self.air.cursor.filter(|_| velocity.length() > 40.0).map(|point| (point - velocity * dt, point));
        let reach = CURSOR_RADIUS * 0.8;
        for ledge in self.layers.iter_mut().skip(1) {
            if ledge.spray.is_empty() && ledge.film.is_empty() {
                continue;
            }
            let (width, face, origin) = (ledge.width(), ledge.face, ledge.origin);
            let near = wiping.filter(|&(from, to)| {
                from.y.min(to.y) - reach < origin.y
                    && from.y.max(to.y) + reach > origin.y - face
                    && from.x.min(to.x) - reach < origin.x + width
                    && from.x.max(to.x) + reach > origin.x
            });
            let covers = &ledge.face_occluders;
            let off = |offset: V2| offset.x < 0.0 || offset.x > width || -offset.y > face;
            let wiped = |offset: V2, margin: f32| {
                let Some((from, to)) = near else { return false };
                let position = origin + offset;
                let path = to - from;
                let along = ((position - from).dot(path) / path.length_squared().max(0.0001)).clamp(0.0, 1.0);
                (position - (from + path * along)).length() < reach + margin
                    && !covers.iter().any(|o| o[0] < position.x && o[2] > position.x && o[1] < position.y && o[3] > position.y)
            };
            let drying = SPRAY_DRYING * dt * hurry;
            ledge.spray.retain_mut(|droplet| {
                droplet.radius -= drying;
                droplet.radius >= 0.2 && !off(droplet.offset) && !wiped(droplet.offset, 0.0)
            });
            ledge.film.retain_mut(|piece| {
                piece.age += dt * hurry;
                let middle = (piece.from + piece.to) * 0.5;
                piece.age < piece.life && !off(middle) && !wiped(middle, piece.width * 0.5)
            });
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
        let wet = self.layers.iter().zip(&self.ids).skip(1).flat_map(move |(layer, &id)| {
            let soft = if self.shelf == Some(id) { 1.0 } else { 0.0 };
            // It dries from its edges in, thinning as it fades.
            let film = layer.film.iter().filter_map(move |piece: &FilmPiece| {
                let position = layer.origin + (piece.from + piece.to) * 0.5;
                if hidden(layer, position) {
                    return None;
                }
                let dry = piece.age / piece.life;
                Some(Sprite {
                    position,
                    velocity: piece.to - piece.from,
                    size: piece.width * (1.0 - 0.45 * dry),
                    alpha: (1.0 - dry).powf(1.5),
                    soft,
                    rain: [5.0, 0.0],
                    ..Sprite::default()
                })
            });
            let spray = layer.spray.iter().filter_map(move |droplet: &Droplet| {
                let position = layer.origin + droplet.offset;
                if hidden(layer, position) {
                    return None;
                }
                Some(Sprite {
                    position,
                    size: droplet.radius * 2.0 * GLASS_SCALE,
                    alpha: 0.8 * ((droplet.radius - 0.2) / 0.15).min(1.0),
                    soft,
                    rain: [4.0, 0.0],
                    ..Sprite::default()
                })
            });
            film.chain(spray)
        });
        let shown: HashMap<WindowId, usize> = self.ids.iter().enumerate().skip(1).map(|(li, &id)| (id, li)).collect();
        let drops = self.rain.drops.iter().filter_map(move |drop| {
            let layer = &self.layers[*shown.get(&drop.layer)?];
            let position = layer.origin + drop.offset;
            // A window in front of the one it runs down hides it.
            if hidden(layer, position) {
                return None;
            }
            // It swells out of nothing as it gathers on the edge, down from the edge, its top staying where it gathers.
            let still = drop.hanging || drop.bead;
            let grown = if drop.bead { 1.0 } else { (drop.age / 0.15).sqrt().min(1.0) };
            let half = if drop.bead { bead_width(drop.radius) } else { half_width(drop.radius, still) };
            let swelling = if still { 0.0 } else { half * (1.0 - grown) };
            // Held back, a running drop's water piles up in its front; jolted, it squashes and stretches for a moment.
            let since = drop.age - drop.jolted;
            let bulge = if still { 0.0 } else { 0.16 * (1.0 - smoothstep(0.0, 40.0, drop.speed)) };
            let jiggle = if still { 0.0 } else { drop.jiggle * (-since / 0.15).exp() * (since * std::f32::consts::TAU * 11.0).cos() };
            Some(Sprite {
                position: position + V2::new(0.0, swelling),
                velocity: if still { V2::ZERO } else { V2::new(drop.slope, -1.0) * drop.speed },
                size: half * 2.0 * grown,
                // Drying up, a hanging drop fades rather than shrinking past the smallest drawn.
                alpha: (drop.radius / if drop.hanging { 1.2 } else { 0.35 }).min(1.0),
                soft: if self.shelf == Some(drop.layer) { 1.0 } else { 0.0 },
                rain: if drop.hanging { [3.0, drop.stage()] } else { [2.0, 0.0] },
                drop: [drop.seed, drop.offset.y * 0.06, bulge, jiggle],
                ..Sprite::default()
            })
        });
        splashes.chain(wet).chain(drops)
    }
}

/// The way a drop running with `slope` goes, one point long.
fn heading(slope: f32) -> V2 {
    V2::new(slope, -1.0) / (1.0 + slope * slope).sqrt()
}

fn run(air: &mut Air, drop: &mut GlassDrop, index: usize, layer: &mut SnowLayer, dt: f32, drops: &mut Vec<GlassDrop>, film: &mut usize) {
    let excess = (drop.radius - 1.2).max(0.0);
    let pull = (60.0 + 45.0 * excess.powf(1.4)).min(700.0) * grip(drop.offset);
    if drop.held > 0.0 {
        // Caught on a speck, its front holds while its water piles up behind it, until it breaks free with a lurch,
        // leaving a lump of it behind where its rear was held.
        drop.held -= dt;
        drop.speed *= (1.0 - dt * 25.0).max(0.0);
        if drop.held <= 0.0 {
            drop.speed = pull * 1.3;
            drop.jiggle = 0.12;
            drop.jolted = drop.age;
            let left = drop.radius * air.random.range(0.22, 0.32);
            leave(air, drop, left, heading(drop.slope), drops);
        }
    } else {
        drop.speed += (pull - drop.speed) * (dt * 5.0).min(1.0);
    }
    // A window dragged along under it leaves it trailing a little behind; its own wandering stays within less.
    let trail = -layer.track_velocity.x * 0.0006;
    let wander = (drop.slope + (air.random.range(-1.0, 1.0) * 1.6 + trail) * dt) * (1.0 - dt * 0.8);
    drop.slope = wander.clamp(-0.45, 0.45);
    let heading = heading(drop.slope);
    let advance = drop.speed * dt;
    let from = drop.offset;
    drop.offset += V2::new(drop.slope, -1.0) * advance;
    gather(drop, from, layer);
    catches(drop);

    // The film behind it breaks up into beads at no even pace: mostly small, now and then a big one, bunched up in
    // places and gone for a stretch in others. A fast drop leaves fewer and smaller, its film too thin for more.
    drop.travelled += advance;
    if drop.travelled >= drop.spacing {
        drop.travelled = 0.0;
        let haste = smoothstep(80.0, 450.0, drop.speed);
        drop.spacing = trail_spacing(air.random.next(), drop.radius) * (1.0 + 0.6 * haste);
        if air.random.next() < 0.9 {
            let kind = air.random.next();
            let share = if kind < 0.2 {
                air.random.range(0.06, 0.12)
            } else if kind < 0.9 {
                air.random.range(0.13, 0.24)
            } else {
                air.random.range(0.26, 0.34)
            };
            leave(air, drop, drop.radius * share * (1.2 - 0.5 * haste), heading, drops);
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
        // Joined, the water gathers round the middle of the two and runs on as one: it sets the other's water moving,
        // or carries on with it if it was running too, and the drop turns a little its way.
        let area = drop.radius * drop.radius;
        let added = other.radius * other.radius;
        let share = added / (area + added);
        drop.offset += offset * share;
        drop.jiggle = (0.6 * share).min(0.25);
        drop.jolted = drop.age;
        let carried = if other.bead || other.hanging { 0.0 } else { other.speed };
        drop.speed = drop.speed * (1.0 - share) + carried * share;
        drop.slope = (drop.slope + offset.x / (drop.radius + other.radius) * share * TURNING).clamp(-0.45, 0.45);
        drop.radius = (area + added).sqrt().min(LARGEST_DROP);
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
    // Behind it the glass stays wet: a film down to where it is now, or to where it came to rest.
    let stopped = drop.hanging || drop.bead;
    if (drop.mark - drop.offset).length() >= if stopped { 0.5 } else { FILM_PIECE } {
        if *film < MAX_FILM {
            layer.film.push(FilmPiece {
                from: drop.mark,
                to: drop.offset,
                width: half_width(drop.radius, false) * 2.0 * FILM_SHARE,
                age: 0.0,
                life: air.random.range(FILM_LIFE.0, FILM_LIFE.1),
            });
            *film += 1;
        }
        drop.mark = drop.offset;
    }
}

/// A bead of `left` a running drop leaves behind it, from the rear of the drop, off to one side or the other.
fn leave(air: &mut Air, drop: &mut GlassDrop, left: f32, heading: V2, drops: &mut Vec<GlassDrop>) {
    drop.radius = (drop.radius * drop.radius - left * left).max(0.0).sqrt();
    if drops.len() >= MAX_GLASS_DROPS || left < 0.15 {
        return;
    }
    let across = V2::new(-heading.y, heading.x) * (air.random.range(-0.4, 0.4) * half_width(drop.radius, false));
    let seed = air.random.next().max(0.001);
    drops.push(GlassDrop::bead(drop.layer, drop.offset - heading * (drop.radius * 0.95 + left * 0.4) + across, drop.face, left, seed));
}

/// Spray in a running drop's way joins it as the drop touches it: the drop takes in its water and gathers round the
/// middle of it all, so it lurches toward where it joined, slows for the moment it takes to set the water moving, and
/// turns a little that way. The more water it gathers, the faster it runs; behind it the glass is clear.
fn gather(drop: &mut GlassDrop, from: V2, layer: &mut SnowLayer) {
    let reach = half_width(drop.radius, false);
    if -drop.offset.y - reach >= SPRAY_DEEPEST + 2.0 || layer.spray.is_empty() {
        return;
    }
    let path = drop.offset - from;
    let length = path.length_squared().max(0.0001);
    let start = drop.radius * drop.radius;
    let mut area = start;
    let mut turn = 0.0;
    let mut shift = 0.0;
    layer.spray.retain(|droplet| {
        let along = ((droplet.offset - from).dot(path) / length).clamp(0.0, 1.0);
        let toward = droplet.offset - (from + path * along);
        if toward.length() >= reach + droplet.radius * GLASS_SCALE * 0.5 {
            return true;
        }
        let added = droplet.radius * droplet.radius * SPRAY_KEPT;
        turn += toward.x / reach * added;
        shift += toward.x * added;
        area += added;
        false
    });
    if area <= start {
        return;
    }
    drop.offset.x += shift / area;
    drop.speed *= start / area;
    drop.slope = (drop.slope + turn / area * TURNING).clamp(-0.45, 0.45);
    drop.radius = drop.radius.max(area.sqrt().min(largest_running(layer.face)));
}

/// A running drop come onto a patch of glass with a speck of dust or grease on it is caught there for a moment: the
/// smaller the drop, the longer.
fn catches(drop: &mut GlassDrop) {
    let patch = ((drop.offset.x / SPECK_SPACING).floor() as i32, (drop.offset.y / SPECK_SPACING).floor() as i32);
    if patch == drop.patch {
        return;
    }
    drop.patch = patch;
    let mut n = (patch.0 as u32).wrapping_mul(2_654_435_761).wrapping_add((patch.1 as u32).wrapping_mul(2_246_822_519));
    n = (n ^ (n >> 15)).wrapping_mul(3_266_489_917);
    let speck = ((n ^ (n >> 13)) & 0xFFFF) as f32 / 65536.0;
    if speck >= SPECK_SHARE {
        return;
    }
    let hold = speck / SPECK_SHARE * 0.5 * (HEAVY_HANGING / drop.radius).powi(2);
    if hold > 0.05 {
        drop.held = hold.min(0.7);
    }
}

/// The biggest drop a window's face takes running down it: a short one, like the Dock's or a taskbar's, smaller ones,
/// or the biggest would loom over it.
fn largest_running(face: f32) -> f32 {
    (face / 32.0).clamp(HEAVY_HANGING, 4.6)
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
        let seed = random.next().max(0.001);
        drops.push(GlassDrop::bead(drop.layer, offset, drop.face, (water * (1.15 - spot) / total).sqrt().max(0.3), seed));
    }
}

/// The part of a drop that lets go falls like any raindrop, to burst on whatever is below, and runs on down a window
/// it lands on.
fn fall(air: &mut Air, drop: &GlassDrop, layer: &SnowLayer, share: f32, below: f32, throw: Throw) {
    let position = layer.origin + drop.offset - V2::new(0.0, below);
    if !air.raining() || position.y <= 2.0 {
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
        kind: Mode::Rain,
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

/// How far a running drop of `radius` goes before it leaves its next bead, for `u` drawn evenly from zero to one: as
/// often a short way as a long one, now and then far.
fn trail_spacing(u: f32, radius: f32) -> f32 {
    (2.0 - 4.0 * (1.0 - u * 0.98).ln()) * radius / 4.0
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
