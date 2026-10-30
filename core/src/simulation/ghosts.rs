//! Halloween's ghosts, made of the drifts' snow: a drift heaps up and the heap takes a ghost's shape. It says boo,
//! holds up a sign, waves or dabs, sometimes creeps along under the snow to come up again, and in the end settles
//! back into a heap. Every third one scared off by the cursor, the next one throws a snowball at the screen.
//! The same as the Mac app's.

use std::f32::consts::{PI, TAU};

use super::{Grain, MAX_GRAINS, MAX_MOTES, Mote, SignSprite, SnowSimulation, Sprite, WindowId};
use crate::geometry::{V2, smoothstep};
use crate::halloween::SIGN_CAPTIONS;
use crate::layer::SnowLayer;

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Rising,
    Standing {
        until: f32,
    },
    /// Slumping back into the drift, to creep under the snow toward `toward`.
    Sinking {
        toward: f32,
    },
    /// Creeping along under the snow toward `to`, a bulge running through the drift.
    Creeping {
        to: f32,
    },
    /// Going for good: it slumps into a heap that its snow is left as.
    Settling,
}

#[derive(Clone, Copy)]
struct Ghost {
    layer: WindowId,
    /// Where it is along its layer, from the layer's left end.
    x: f32,
    size: f32,
    seed: f32,
    /// Whether it comes up once more somewhere else before it settles.
    returns: bool,
    /// After a third ghost is scared off, it throws a snowball at the screen.
    avenges: bool,
    stage: Stage,
    surfaced: u32,
    age: f32,
    /// How far it has taken shape: the heap rises up to 0.5, and turns into the ghost from 0.4 to 1.
    form: f32,
    /// Snow it has drawn out of the drift around it, which goes back when it settles or crumbles.
    taken: f32,
    boo_at: f32,
    /// When it says boo or does something else unprompted, if the cursor hasn't come near enough first.
    next_boo: f32,
    sign: Option<usize>,
    sign_at: f32,
    wave_at: f32,
    dab_at: f32,
    stand_at: f32,
    threw: bool,
}

impl Ghost {
    fn new(layer: WindowId, x: f32, size: f32, seed: f32, returns: bool, avenges: bool) -> Ghost {
        Ghost {
            layer,
            x,
            size,
            seed,
            returns,
            avenges,
            stage: Stage::Rising,
            surfaced: 0,
            age: 0.0,
            form: 0.0,
            taken: 0.0,
            boo_at: -10.0,
            next_boo: f32::INFINITY,
            sign: None,
            sign_at: -10.0,
            wave_at: -10.0,
            dab_at: -10.0,
            stand_at: 0.0,
            threw: false,
        }
    }

    fn column(&self, layer: &SnowLayer) -> usize {
        layer.clamped_index(layer.base().x + self.x)
    }

    /// Its reach either side, in columns: its widest, which stays in the snow.
    fn reach(&self, layer: &SnowLayer) -> usize {
        (self.size * 0.4 / layer.column_width).ceil() as usize
    }

    /// How high it stands above the drift, as a share of its size: the heap's height, then the ghost's half.
    fn height(&self) -> f32 {
        let heap = 0.3 * smoothstep(0.0, 0.5, self.form);
        heap + (0.425 - heap) * smoothstep(0.4, 1.0, self.form)
    }

    /// The snow it holds once it has taken shape this far.
    fn bulk(&self) -> f32 {
        self.size * self.size * 0.04 * self.height() / 0.425
    }

    /// How far its paws are up: thrown up quickly, held a moment, lowered.
    fn paws(&self) -> f32 {
        let since = self.age - self.boo_at;
        if !(0.0..1.3).contains(&since) {
            return 0.0;
        }
        if since < 0.9 { smoothstep(0.0, 0.2, since) } else { 1.0 - smoothstep(0.9, 1.3, since) }
    }

    /// How far its sign is up out of the snow: pulled up, held a few seconds, put back.
    fn sign_up(&self) -> f32 {
        let since = self.age - self.sign_at;
        if self.sign.is_none() || !(0.0..4.2).contains(&since) {
            return 0.0;
        }
        if since < 3.8 { smoothstep(0.0, 0.45, since) } else { 1.0 - smoothstep(3.8, 4.2, since) }
    }

    /// How far its paw is up holding the sign; it reaches up a moment before the sign shows and lets go after.
    fn holding(&self) -> f32 {
        let since = self.age - self.sign_at;
        if self.sign.is_none() || !(0.0..4.4).contains(&since) {
            return 0.0;
        }
        if since < 3.9 { smoothstep(0.0, 0.25, since) } else { 1.0 - smoothstep(3.9, 4.4, since) }
    }

    /// How far its paws are spread for the wave, which runs along them for a few seconds.
    fn waving(&self) -> f32 {
        let since = self.age - self.wave_at;
        if !(0.0..2.8).contains(&since) {
            return 0.0;
        }
        if since < 2.4 { smoothstep(0.0, 0.3, since) } else { 1.0 - smoothstep(2.4, 2.8, since) }
    }

    /// How far into a dab it is: snapped into, held a moment, let go.
    fn dabbing(&self) -> f32 {
        let since = self.age - self.dab_at;
        if !(0.0..1.6).contains(&since) {
            return 0.0;
        }
        if since < 1.25 { smoothstep(0.0, 0.15, since) } else { 1.0 - smoothstep(1.25, 1.6, since) }
    }

    /// Its right paw winding far back over its head and swinging through, while it avenges itself: angle and weight.
    fn fling(&self) -> Option<(f32, f32)> {
        if !self.avenges {
            return None;
        }
        let since = self.age - self.stand_at;
        if since <= 0.4 || since >= 2.2 {
            return None;
        }
        Some(if since < 1.0 {
            (-1.35 + 3.1 * smoothstep(0.4, 1.0, since), smoothstep(0.4, 0.55, since))
        } else if since < 1.12 {
            (1.75 - 2.45 * smoothstep(1.0, 1.12, since), 1.0)
        } else {
            (-0.7, 1.0 - smoothstep(1.7, 2.2, since))
        })
    }
}

#[derive(Clone, Copy)]
struct FallingSign {
    sign: SignSprite,
    velocity: V2,
    spin: f32,
    age: f32,
}

#[derive(Clone, Copy)]
struct Snowball {
    from: V2,
    to: V2,
    age: f32,
}

/// Halloween's ghosts, and what they leave about: signs falling, snowballs flying, snow clinging to the screen.
pub(super) struct Haunting {
    pub(super) on: bool,
    ghosts: Vec<Ghost>,
    next_ghost: f64,
    falling_signs: Vec<FallingSign>,
    last_caption: Option<usize>,
    /// Ghosts crumbled by the cursor since the last revenge, whether or not others left on their own between; the
    /// third brings one out for revenge.
    scares: u32,
    revenge_due: bool,
    /// Where the third scared ghost crumbled: the avenger comes up out of its snow.
    revenge_at: Option<(WindowId, f32)>,
    snowballs: Vec<Snowball>,
    hit_at: f64,
    /// Snow a snowball left clinging to the screen, with how long each grain still holds on.
    clinging: Vec<(Grain, f32)>,
}

impl Haunting {
    pub(super) fn new() -> Haunting {
        Haunting {
            on: false,
            ghosts: Vec::new(),
            next_ghost: 0.0,
            falling_signs: Vec::new(),
            last_caption: None,
            scares: 0,
            revenge_due: false,
            revenge_at: None,
            snowballs: Vec::new(),
            hit_at: -10.0,
            clinging: Vec::new(),
        }
    }

    pub(super) fn sprite_count(&self) -> usize {
        self.ghosts.len() * 2 + self.snowballs.len() + self.clinging.len()
    }

    /// Whether anything of Halloween's is moving, besides the sprites counted above.
    pub(super) fn moving(&self) -> bool {
        !self.falling_signs.is_empty()
    }

    /// Whether a snowball is in flight or has just hit, which wants the full frame rate.
    pub(super) fn pushing(&self, clock: f64) -> bool {
        !self.snowballs.is_empty() || clock - self.hit_at < 0.6
    }
}

impl SnowSimulation {
    /// Turns Halloween's ghosts and pumpkins on or off; the first comes soon after, so it is seen before Halloween gets forgotten.
    pub fn set_halloween(&mut self, on: bool) {
        if on && !self.haunting.on {
            self.haunting.next_ghost = self.air.clock + self.air.random.range(3.0, 8.0) as f64;
        }
        if on != self.haunting.on {
            self.lanterns.reset();
        }
        self.haunting.on = on;
    }

    /// The signs ghosts are holding up, or letting fall.
    pub fn signs(&self) -> Vec<SignSprite> {
        let held = self.haunting.ghosts.iter().filter_map(|ghost| self.layer_of(ghost).and_then(|li| self.sign_of(ghost, li)));
        held.chain(self.haunting.falling_signs.iter().map(|falling| falling.sign)).collect()
    }

    /// How far everything is jolted, as if the screen had been hit by a snowball, in points.
    pub fn shake(&self) -> V2 {
        let since = (self.air.clock - self.haunting.hit_at) as f32;
        if since >= 0.5 {
            return V2::ZERO;
        }
        let amount = 7.0 * (-since * 9.0).exp();
        V2::new((since * 90.0).sin() * amount, (since * 77.0).cos() * amount * 0.6)
    }

    pub(super) fn write_haunting(&self, buffer: &mut [Sprite]) -> usize {
        let mut sprites = Vec::new();
        for ghost in &self.haunting.ghosts {
            let Some(li) = self.layer_of(ghost) else { continue };
            let (ground, center) = self.placement(ghost, li);
            let fling = ghost.fling();
            sprites.push(Sprite {
                position: center,
                size: ghost.size,
                alpha: 1.0,
                soft: 1.0,
                tint: [1.0; 3],
                ghost: [1.0 + ghost.seed, ground, smoothstep(0.85, 1.0, ghost.form), ghost.form],
                pose: [ghost.paws(), fling.map_or(ghost.holding(), |(_, weight)| weight), fling.map_or(0.35, |(angle, _)| angle), ghost.waving()],
                mood: [ghost.dabbing(), if ghost.avenges { 1.0 } else { 0.0 }],
                ..Sprite::default()
            });
            if let Some((angle, _)) = fling
                && !ghost.threw
            {
                sprites.push(Sprite { position: self.paw(ghost, li, angle), size: 7.0, alpha: 1.0, soft: 0.15, tint: [1.0; 3], ..Sprite::default() });
            }
        }
        for ball in &self.haunting.snowballs {
            let t = (ball.age / 0.5).min(1.0);
            let position = ball.from + (ball.to - ball.from) * (1.0 - (1.0 - t).powi(2)) + V2::new(0.0, (t * PI).sin() * 60.0);
            sprites.push(Sprite { position, size: 7.0 + 145.0 * t * t, alpha: 1.0, soft: 0.12, tint: [1.0; 3], ..Sprite::default() });
        }
        for (grain, _) in &self.haunting.clinging {
            sprites.push(Sprite { position: grain.position, size: grain.size, alpha: 0.95, soft: 0.3, tint: [1.0; 3], ..Sprite::default() });
        }
        let count = sprites.len().min(buffer.len());
        buffer[..count].copy_from_slice(&sprites[..count]);
        count
    }

    /// On Halloween a ghost grows out of a drift now and then; the cursor's touch crumbles it back into snow.
    pub(super) fn haunt(&mut self, dt: f32) {
        self.drop_signs(dt);
        self.throw_snowballs(dt);
        self.let_go(dt);
        let clock = self.air.clock;
        if self.haunting.on && self.air.gust.is_none() && clock >= self.haunting.next_ghost {
            let wait = if self.haunting.ghosts.len() >= 3 || self.raise_ghost() { self.air.random.range(20.0, 40.0) } else { 5.0 };
            self.haunting.next_ghost = clock + wait as f64;
        }
        let mut i = self.haunting.ghosts.len();
        while i > 0 {
            i -= 1;
            let mut ghost = self.haunting.ghosts[i];
            ghost.age += dt;
            let gone = match self.layer_of(&ghost) {
                None => true,
                // A window come in front of it: it ducks back into the drift, as the drift itself goes behind the window.
                Some(li) if self.covered(&ghost, li) => {
                    self.pour(&mut ghost, li, 0.0);
                    true
                }
                Some(li) if (ghost.threw || !ghost.avenges) && self.air.cursor.is_some_and(|cursor| self.touches(&ghost, li, cursor)) => {
                    if let Some(sign) = self.sign_of(&ghost, li) {
                        let random = &mut self.air.random;
                        let falling =
                            FallingSign { sign, velocity: V2::new(random.range(-30.0, 30.0), 60.0), spin: random.range(-2.5, 2.5), age: 0.0 };
                        self.haunting.falling_signs.push(falling);
                    }
                    self.crumble(&ghost, li, true);
                    self.haunting.scares += 1;
                    if self.haunting.scares >= 3 {
                        self.haunting.scares = 0;
                        self.haunting.revenge_due = true;
                        self.haunting.revenge_at = Some((ghost.layer, ghost.x));
                        self.haunting.next_ghost = self.haunting.next_ghost.min(clock + 0.4);
                    }
                    true
                }
                Some(li) => {
                    let alive = self.advance(&mut ghost, li, dt);
                    if alive && ghost.stage != Stage::Settling {
                        let bulk = ghost.bulk();
                        self.gather(&mut ghost, li, bulk);
                    }
                    !alive
                }
            };
            if gone {
                self.haunting.ghosts.swap_remove(i);
            } else {
                self.haunting.ghosts[i] = ghost;
            }
        }
    }

    /// Ghosts the front of the gust clearing the snow reaches come apart and go with the rest, and snow clinging to
    /// the screen is torn off.
    pub(super) fn sweep_haunting(&mut self, front: f32, direction: f32, calm: bool, finish: bool) {
        let mut i = self.haunting.ghosts.len();
        while i > 0 {
            i -= 1;
            let ghost = self.haunting.ghosts[i];
            let Some(li) = self.layer_of(&ghost) else { continue };
            let center = self.placement(&ghost, li).1;
            if finish || calm || (front - center.x) * direction > 0.0 {
                if !calm {
                    self.blow_apart(&ghost, li, direction);
                }
                self.haunting.ghosts.swap_remove(i);
            }
        }
        let mut i = self.haunting.clinging.len();
        while i > 0 {
            i -= 1;
            let (mut grain, _) = self.haunting.clinging[i];
            if !(finish || calm || (front - grain.position.x) * direction > 0.0) {
                continue;
            }
            self.haunting.clinging.swap_remove(i);
            if calm || self.air.grains.len() >= MAX_GRAINS {
                continue;
            }
            grain.blown = true;
            grain.area = 0.0;
            grain.velocity = V2::new(direction * self.air.random.range(800.0, 1600.0), self.air.random.range(60.0, 300.0));
            self.air.grains.push(grain);
        }
    }

    fn layer_of(&self, ghost: &Ghost) -> Option<usize> {
        self.ids.iter().position(|&id| id == ghost.layer)
    }

    /// Moves the ghost on through its stages; returns false once it has settled into a heap.
    fn advance(&mut self, ghost: &mut Ghost, li: usize, dt: f32) -> bool {
        match ghost.stage {
            Stage::Rising => {
                // Coming up again, or for revenge, it is quicker.
                let pace = if ghost.avenges {
                    1.2
                } else if ghost.surfaced == 0 {
                    3.0
                } else {
                    2.0
                };
                ghost.form = (ghost.form + dt / pace).min(1.0);
                if ghost.form > 0.0 && ghost.form < 1.0 && self.air.random.next() < dt * 12.0 {
                    self.stir(ghost, li);
                }
                if ghost.form >= 1.0 {
                    ghost.surfaced += 1;
                    let first = ghost.surfaced == 1;
                    ghost.stand_at = ghost.age;
                    let stay = if ghost.avenges {
                        3.0
                    } else if first {
                        self.air.random.range(4.0, 6.5)
                    } else {
                        self.air.random.range(2.5, 3.5)
                    };
                    ghost.stage = Stage::Standing { until: ghost.age + stay };
                    ghost.next_boo = ghost.age + if first { self.air.random.range(0.6, 2.0) } else { 0.3 };
                }
            }
            Stage::Standing { until } => {
                let mut until = until;
                if ghost.avenges {
                    self.avenge(ghost, li);
                } else if ghost.age - ghost.boo_at > 2.5 && ghost.holding() == 0.0 && ghost.waving() == 0.0 {
                    let center = self.placement(ghost, li).1;
                    let near = self.air.cursor.is_some_and(|cursor| (cursor - center).length() < ghost.size * 1.6);
                    if near {
                        ghost.boo_at = ghost.age;
                    } else if ghost.age >= ghost.next_boo {
                        let pick = self.air.random.next();
                        if ghost.sign.is_none() && pick < 0.3 {
                            ghost.sign = Some(self.next_caption());
                            ghost.sign_at = ghost.age;
                            until = until.max(ghost.age + 4.8);
                        } else if pick < 0.55 {
                            ghost.wave_at = ghost.age;
                            until = until.max(ghost.age + 3.2);
                        } else if pick < 0.75 {
                            ghost.dab_at = ghost.age;
                            until = until.max(ghost.age + 2.0);
                        } else {
                            ghost.boo_at = ghost.age;
                        }
                        ghost.next_boo = ghost.age + self.air.random.range(3.0, 6.0);
                    }
                }
                ghost.stage = Stage::Standing { until };
                if ghost.age >= until {
                    let target = if ghost.returns { self.creep_target(ghost, li) } else { None };
                    ghost.stage = match target {
                        Some(toward) => {
                            ghost.returns = false;
                            Stage::Sinking { toward }
                        }
                        None => Stage::Settling,
                    };
                }
            }
            Stage::Sinking { toward } => {
                ghost.form = (ghost.form - dt / 2.5).max(0.22);
                if ghost.form <= 0.22 {
                    ghost.stage = Stage::Creeping { to: toward };
                }
            }
            Stage::Creeping { to } => {
                let left = to - ghost.x;
                ghost.x += left.abs().min(70.0 * dt).copysign(left);
                if self.air.random.next() < dt * 14.0 {
                    self.stir(ghost, li);
                }
                if (to - ghost.x).abs() < 0.01 {
                    self.level(ghost, li);
                    ghost.stage = Stage::Rising;
                }
            }
            Stage::Settling => {
                ghost.form = (ghost.form - dt / 2.0).max(0.0);
                let keeping = if ghost.form > 0.0 { ghost.bulk() } else { 0.0 };
                self.pour(ghost, li, keeping);
                if ghost.form > 0.0 && self.air.random.next() < dt * 6.0 {
                    self.stir(ghost, li);
                }
                return ghost.form > 0.0;
            }
        }
        true
    }

    /// Picks a deep enough stretch of drift clear of other ghosts and levels the snow it will rise out of; an avenger
    /// comes up where the last scared ghost crumbled, if it can.
    fn raise_ghost(&mut self) -> bool {
        let size = self.air.random.range(56.0, 76.0);
        if self.haunting.revenge_due
            && let Some((id, x)) = self.haunting.revenge_at.take()
            && let Some(li) = self.ids.iter().position(|&other| other == id)
        {
            let layer = &self.layers[li];
            let reach = (size * 0.4 / layer.column_width).ceil() as usize;
            let column = layer.clamped_index(layer.base().x + x);
            if column >= reach * 3 && column + reach * 3 < layer.count() && layer.visible[column] {
                let ghost = Ghost::new(id, x, size, self.air.random.next(), false, true);
                self.level(&ghost, li);
                self.haunting.ghosts.push(ghost);
                self.haunting.revenge_due = false;
                return true;
            }
        }
        for _ in 0..16 {
            let li = ((self.air.random.next() * self.layers.len() as f32) as usize).min(self.layers.len() - 1);
            let layer = &self.layers[li];
            let reach = (size * 0.4 / layer.column_width).ceil() as usize;
            if layer.count() <= reach * 6 + 1 {
                continue;
            }
            let room = layer.count() - reach * 6;
            let column = reach * 3 + ((self.air.random.next() * room as f32) as usize).min(room - 1);
            let id = self.ids[li];
            if layer.heights[column] <= 6.0
                || !(column - reach..=column + reach).all(|j| layer.visible[j])
                || self.haunting.ghosts.iter().any(|other| other.layer == id && other.column(layer).abs_diff(column) < reach * 4)
            {
                continue;
            }
            let x = (column as f32 + 0.5) * layer.column_width;
            let avenges = self.haunting.revenge_due;
            let returns = !avenges && self.air.random.next() < 0.5;
            let ghost = Ghost::new(id, x, size, self.air.random.next(), returns, avenges);
            if self.covered(&ghost, li) {
                continue;
            }
            self.haunting.revenge_due = false;
            self.level(&ghost, li);
            self.haunting.ghosts.push(ghost);
            return true;
        }
        false
    }

    /// Evens out the snow the ghost will rise out of, flat under its middle and easing into the drift either side.
    fn level(&mut self, ghost: &Ghost, li: usize) {
        let layer = &mut self.layers[li];
        let (column, reach) = (ghost.column(layer), ghost.reach(layer));
        let under = column.saturating_sub(reach)..=(column + reach).min(layer.count() - 1);
        let weights: Vec<f32> = under.clone().map(|j| 1.0 - smoothstep(0.5, 1.0, j.abs_diff(column) as f32 / reach as f32)).collect();
        let total: f32 = weights.iter().sum();
        if total <= 0.0 {
            return;
        }
        // Their weighted mean, so evening them out neither adds snow nor takes it.
        let level = under.clone().zip(&weights).map(|(j, weight)| layer.heights[j] * weight).sum::<f32>() / total;
        for (j, weight) in under.zip(&weights) {
            let height = layer.heights[j];
            layer.set(j, height + (level - height) * weight);
        }
        self.air.heights_changed = true;
    }

    /// Draws snow out of the drift around the ghost, leaving what it stands in, or gives it back, until it holds `target`.
    fn gather(&mut self, ghost: &mut Ghost, li: usize, target: f32) {
        let delta = target - ghost.taken;
        if delta.abs() <= 0.01 {
            return;
        }
        let layer = &mut self.layers[li];
        let (column, reach, count) = (ghost.column(layer), ghost.reach(layer), layer.count());
        let ring: Vec<usize> = (column.saturating_sub(reach * 3)..column.saturating_sub(reach))
            .chain((column + reach + 1).min(count)..(column + reach * 3 + 1).min(count))
            .collect();
        // Most from the middle of the stretch either side, none at its ends, so the drift dips without steps.
        let weights: Vec<f32> = ring.iter().map(|&j| ((j.abs_diff(column) - reach) as f32 / (reach * 2) as f32 * PI).sin()).collect();
        let total = weights.iter().sum::<f32>() * layer.column_width;
        if total <= 0.0 {
            return;
        }
        let mut moved = 0.0;
        for (&j, weight) in ring.iter().zip(&weights) {
            let height = layer.heights[j];
            let each = delta * weight / total;
            let change = if delta > 0.0 { each.min(height * 0.5) } else { each };
            layer.set(j, height - change);
            moved += change * layer.column_width;
        }
        ghost.taken += moved;
        self.air.heights_changed = true;
    }

    /// Lets the snow it holds beyond `keeping` down onto the drift where it stands, heaped up in the middle.
    fn pour(&mut self, ghost: &mut Ghost, li: usize, keeping: f32) {
        let amount = ghost.taken - keeping;
        if amount <= 0.01 {
            return;
        }
        let layer = &mut self.layers[li];
        let (column, reach) = (ghost.column(layer), ghost.reach(layer));
        let under = column.saturating_sub(reach)..=(column + reach).min(layer.count() - 1);
        let weights: Vec<f32> = under
            .clone()
            .map(|j| {
                let off = (j as f32 - column as f32) / reach as f32;
                (1.0 - off * off).max(0.0)
            })
            .collect();
        let total = weights.iter().sum::<f32>() * layer.column_width;
        if total <= 0.0 {
            return;
        }
        for (j, weight) in under.zip(&weights) {
            let height = layer.heights[j];
            layer.set(j, height + amount * weight / total);
        }
        ghost.taken = keeping;
        self.air.heights_changed = true;
    }

    /// Somewhere a little way along the drift to come up again, through snow deep enough to creep under all the way.
    fn creep_target(&mut self, ghost: &Ghost, li: usize) -> Option<f32> {
        let distance = self.air.random.range(60.0, 160.0);
        let first = if self.air.random.next() < 0.5 { -1.0 } else { 1.0 };
        let layer = &self.layers[li];
        let (column, reach) = (ghost.column(layer), ghost.reach(layer));
        for direction in [first, -first] {
            let target = ghost.x + direction * distance;
            if target < 0.0 {
                continue;
            }
            let to = (target / layer.column_width) as usize;
            if to < reach * 3
                || to + reach * 3 >= layer.count()
                || !(column.min(to)..=column.max(to)).all(|j| layer.visible[j] && layer.heights[j] > 4.0)
                || self
                    .haunting
                    .ghosts
                    .iter()
                    .any(|other| other.layer == ghost.layer && other.x != ghost.x && other.column(layer).abs_diff(to) < reach * 4)
            {
                continue;
            }
            return Some((to as f32 + 0.5) * layer.column_width);
        }
        None
    }

    /// Where a ghost stands: the drift's surface below it, and the middle of its sprite.
    fn placement(&self, ghost: &Ghost, li: usize) -> (f32, V2) {
        let layer = &self.layers[li];
        let ground = layer.surface(ghost.column(layer));
        // Its head's top lies 0.39 of its size above the middle, and stands 0.425 above the drift once formed.
        (ground, V2::new(layer.base().x + ghost.x, ground + ghost.size * 0.035))
    }

    /// Whether a window in front of its drift reaches over where the ghost stands, its paws spread or its sign held up.
    fn covered(&self, ghost: &Ghost, li: usize) -> bool {
        let (ground, center) = self.placement(ghost, li);
        let mut area = [center.x - ghost.size * 0.4, ground - 2.0, center.x + ghost.size * 0.4, ground + ghost.size * 0.45];
        if ghost.sign.is_some() && ghost.holding() > 0.0 {
            let paw = self.paw(ghost, li, 0.35);
            area = [area[0].min(paw.x - 30.0), area[1], area[2].max(paw.x + 30.0), area[3].max(paw.y + 60.0)];
        }
        self.layers[li].occluders.iter().any(|o| o[0] < area[2] && o[2] > area[0] && o[1] < area[3] && o[3] > area[1])
    }

    /// The tip of its right paw, held out at `angle`.
    fn paw(&self, ghost: &Ghost, li: usize, angle: f32) -> V2 {
        self.placement(ghost, li).1 + V2::new(0.17 + 0.17 * angle.cos(), 0.1 + 0.17 * angle.sin()) * ghost.size
    }

    fn touches(&self, ghost: &Ghost, li: usize, point: V2) -> bool {
        let (ground, center) = self.placement(ghost, li);
        let top = ground + ghost.height() * ghost.size;
        ghost.form > 0.3 && (point.x - center.x).abs() < ghost.size * 0.3 && point.y > ground && point.y < top
    }

    /// Winds up and throws a snowball anywhere at the screen, then says boo.
    fn avenge(&mut self, ghost: &mut Ghost, li: usize) {
        let since = ghost.age - ghost.stand_at;
        if !ghost.threw && since >= 1.06 {
            ghost.threw = true;
            let to = V2::new(self.air.random.range(0.15, 0.85) * self.width, self.air.random.range(0.35, 0.85) * self.height);
            let from = self.paw(ghost, li, 0.6);
            self.haunting.snowballs.push(Snowball { from, to, age: 0.0 });
        }
        if ghost.threw && ghost.boo_at < ghost.stand_at && since >= 2.0 {
            ghost.boo_at = ghost.age;
        }
    }

    fn next_caption(&mut self) -> usize {
        let count = SIGN_CAPTIONS.len();
        let mut caption = ((self.air.random.next() * (count - 1) as f32) as usize).min(count - 2);
        if let Some(last) = self.haunting.last_caption
            && caption >= last
        {
            caption += 1;
        }
        self.haunting.last_caption = Some(caption);
        caption
    }

    /// The sign the ghost holds up, its stick in the raised paw, rising out of the snow beside it as it is pulled up.
    fn sign_of(&self, ghost: &Ghost, li: usize) -> Option<SignSprite> {
        let caption = ghost.sign?;
        let up = ghost.sign_up();
        if up <= 0.0 {
            return None;
        }
        let ground = self.placement(ghost, li).0;
        Some(SignSprite {
            caption,
            anchor: self.paw(ghost, li, 0.35) - V2::new(0.0, (1.0 - up) * 100.0),
            angle: -0.1 + 0.05 * (ghost.age * 2.3 + ghost.seed * 5.0).sin(),
            ground,
        })
    }

    fn drop_signs(&mut self, dt: f32) {
        for falling in &mut self.haunting.falling_signs {
            falling.age += dt;
            falling.velocity.y -= 900.0 * dt;
            falling.sign.anchor += falling.velocity * dt;
            falling.sign.angle += falling.spin * dt;
        }
        self.haunting.falling_signs.retain(|falling| falling.age <= 1.5);
    }

    /// Snowballs fly at the screen and burst against it.
    fn throw_snowballs(&mut self, dt: f32) {
        let mut i = self.haunting.snowballs.len();
        while i > 0 {
            i -= 1;
            self.haunting.snowballs[i].age += dt;
            if self.haunting.snowballs[i].age >= 0.5 {
                let to = self.haunting.snowballs.remove(i).to;
                self.shatter(to);
            }
        }
    }

    /// A snowball smacking into the screen: the scene jolts, the snowfall around is blown back, and the snow
    /// flattens against the glass and clings there a moment, crumbling from its edge, while the rest flies apart.
    fn shatter(&mut self, point: V2) {
        self.haunting.hit_at = self.air.clock;
        for flake in &mut self.air.flakes {
            let away = flake.position - point;
            let distance = away.length();
            if distance > 1.0 && distance < 300.0 {
                flake.velocity += away * ((1.0 - distance / 300.0) * 900.0 / distance);
            }
        }
        let reach = 65.0;
        let random = &mut self.air.random;
        for _ in 0..360 {
            let angle = random.range(0.0, TAU);
            let out = random.next().sqrt();
            let ragged = 1.0 + 0.18 * (angle * 5.0 + 1.3).sin() + 0.1 * (angle * 9.0).sin();
            let grain = Grain {
                position: point + V2::new(angle.cos(), angle.sin() * 0.85) * (out * reach * ragged),
                velocity: V2::ZERO,
                area: 0.7,
                size: random.range(2.4, 4.4),
                lump: 0.0,
                blown: false,
            };
            self.haunting.clinging.push((grain, 0.5 + (1.0 - out) * 1.6 + random.range(0.0, 0.4)));
        }
        for _ in 0..90 {
            if self.air.grains.len() >= MAX_GRAINS {
                break;
            }
            let random = &mut self.air.random;
            let angle = random.range(0.0, TAU);
            let direction = V2::new(angle.cos(), angle.sin());
            let grain = Grain {
                position: point + direction * random.range(reach * 0.6, reach),
                velocity: direction * random.range(150.0, 600.0) + V2::new(0.0, 120.0),
                area: random.range(0.6, 1.4),
                size: random.range(1.4, 3.2),
                lump: 0.0,
                blown: false,
            };
            self.air.grains.push(grain);
        }
        for _ in 0..5 {
            let random = &mut self.air.random;
            let angle = random.range(0.0, TAU);
            let direction = V2::new(angle.cos(), angle.sin());
            let (area, speed) = (random.range(6.0, 12.0), random.range(150.0, 350.0));
            self.air.throw_lump(area, point + direction * (reach * 0.8), direction * speed + V2::new(0.0, 150.0));
        }
        for _ in 0..24 {
            if self.air.motes.len() >= MAX_MOTES {
                break;
            }
            let random = &mut self.air.random;
            let angle = random.range(0.0, TAU);
            let direction = V2::new(angle.cos(), angle.sin());
            let mote = Mote {
                position: point + direction * random.range(0.0, reach),
                velocity: direction * random.range(60.0, 220.0),
                size: random.range(20.0, 45.0),
                alpha: random.range(0.12, 0.22),
                age: 0.0,
                life: random.range(0.8, 1.6),
            };
            self.air.motes.push(mote);
        }
    }

    /// Snow clinging to the screen sags a little, and each grain falls once it lets go.
    fn let_go(&mut self, dt: f32) {
        let mut i = self.haunting.clinging.len();
        while i > 0 {
            i -= 1;
            let (grain, hold) = &mut self.haunting.clinging[i];
            *hold -= dt;
            grain.position.y -= 5.0 * dt;
            if *hold > 0.0 {
                continue;
            }
            let grain = self.haunting.clinging.swap_remove(i).0;
            if self.air.grains.len() < MAX_GRAINS {
                self.air.grains.push(grain);
            }
        }
    }

    /// A little powder kicked up where the ghost breaks the drift's surface.
    fn stir(&mut self, ghost: &Ghost, li: usize) {
        if self.air.motes.len() >= MAX_MOTES {
            return;
        }
        let (ground, center) = self.placement(ghost, li);
        let random = &mut self.air.random;
        let side = if random.next() < 0.5 { -1.0 } else { 1.0 };
        let mote = Mote {
            position: V2::new(center.x + side * ghost.size * random.range(0.15, 0.3), ground),
            velocity: V2::new(side * random.range(10.0, 50.0), random.range(15.0, 60.0)),
            size: random.range(6.0, 12.0),
            alpha: random.range(0.08, 0.14),
            age: 0.0,
            life: random.range(0.6, 1.2),
        };
        self.air.motes.push(mote);
    }

    /// The ghost bursts into the snow it was made of when scared, which falls and heaps up on the drift.
    fn crumble(&mut self, ghost: &Ghost, li: usize, scared: bool) {
        let (ground, center) = self.placement(ghost, li);
        let height = ghost.height() * ghost.size;
        let mut left = ghost.taken;
        while left > 0.3 && self.air.grains.len() < MAX_GRAINS {
            let random = &mut self.air.random;
            let area = left.min(random.range(0.6, 1.6));
            left -= area;
            let up = random.next();
            let half = ghost.size * (0.12 + 0.18 * (1.0 - up));
            let velocity = if scared {
                V2::new(random.range(-40.0, 40.0), random.range(-20.0, 90.0))
            } else {
                V2::new(random.range(-12.0, 12.0), random.range(-30.0, 10.0))
            };
            let grain = Grain {
                position: V2::new(center.x + random.range(-half, half), ground + up * height),
                velocity,
                area,
                size: random.range(1.2, 2.6),
                lump: 0.0,
                blown: false,
            };
            self.air.grains.push(grain);
        }
        if left > 0.0 {
            self.layers[li].deposit(left, center.x, true);
        }
        for _ in 0..(if scared { 10 } else { 5 }) {
            if self.air.motes.len() >= MAX_MOTES {
                break;
            }
            let random = &mut self.air.random;
            let mote = Mote {
                position: V2::new(center.x + random.range(-0.25, 0.25) * ghost.size, ground + random.next() * height),
                velocity: V2::new(random.range(-60.0, 60.0), random.range(-10.0, 60.0)),
                size: random.range(8.0, 16.0),
                alpha: random.range(0.08, 0.15),
                age: 0.0,
                life: random.range(0.6, 1.3),
            };
            self.air.motes.push(mote);
        }
        self.air.heights_changed = true;
    }

    /// A ghost caught by the gust clearing the snow: it comes apart into powder that rides the wind away, sign and all.
    fn blow_apart(&mut self, ghost: &Ghost, li: usize, direction: f32) {
        if let Some(sign) = self.sign_of(ghost, li) {
            let random = &mut self.air.random;
            let velocity = V2::new(direction * random.range(500.0, 800.0), random.range(100.0, 250.0));
            let spin = direction * random.range(3.0, 6.0);
            self.haunting.falling_signs.push(FallingSign { sign, velocity, spin, age: 0.0 });
        }
        let (ground, center) = self.placement(ghost, li);
        let height = ghost.height() * ghost.size;
        for _ in 0..70 {
            let up = self.air.random.next();
            let half = ghost.size * (0.1 + 0.15 * (1.0 - up));
            let x = center.x + self.air.random.range(-half, half);
            self.air.lift(V2::new(x, ground + up * height), direction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::Snowfall;

    const DT: f32 = 1.0 / 60.0;
    const AWAY: V2 = V2::new(-10_000.0, -10_000.0);

    /// A haunted screen with a deep, even drift and no snowfall to muddle it.
    fn haunted() -> SnowSimulation {
        let mut simulation = SnowSimulation::new(1200.0, 800.0, Snowfall::Off);
        for i in 0..simulation.layers[0].count() {
            simulation.layers[0].set(i, 20.0);
        }
        simulation.set_halloween(true);
        simulation.haunting.next_ghost = 0.0;
        simulation
    }

    fn ground_snow(simulation: &SnowSimulation) -> f32 {
        simulation.layers[0].heights.iter().sum::<f32>() * simulation.layers[0].column_width
    }

    fn ghost_sprites(simulation: &SnowSimulation) -> Vec<Sprite> {
        let mut buffer = vec![Sprite::default(); simulation.sprite_count() + 16];
        let count = simulation.write_sprites(&mut buffer);
        buffer.truncate(count);
        buffer.into_iter().filter(|sprite| sprite.ghost[0] >= 1.0).collect()
    }

    #[test]
    fn a_ghost_takes_shape_out_of_a_drift() {
        let mut simulation = haunted();
        let before = ground_snow(&simulation);
        for _ in 0..(4.0 / DT) as usize {
            simulation.step(DT, AWAY);
        }
        let ghosts = ghost_sprites(&simulation);
        assert_eq!(ghosts.len(), 1);
        assert!(ghosts[0].ghost[3] >= 1.0, "formed, got {}", ghosts[0].ghost[3]);
        let held = simulation.haunting.ghosts[0].taken;
        assert!(held > 10.0, "it is made of the drift's snow, held {held}");
        let after = ground_snow(&simulation) + held;
        assert!((after - before).abs() < before * 0.01, "snow kept: {before} then {after}");
    }

    #[test]
    fn the_cursor_crumbles_a_ghost_back_into_snow() {
        let mut simulation = haunted();
        for _ in 0..(4.0 / DT) as usize {
            simulation.step(DT, AWAY);
        }
        let ghost = simulation.haunting.ghosts[0];
        let (ground, center) = simulation.placement(&ghost, 0);
        let touch = V2::new(center.x, ground + ghost.size * 0.2) - super::super::CURSOR_OFFSET;
        simulation.step(DT, touch);
        assert!(simulation.haunting.ghosts.is_empty());
        assert_eq!(simulation.haunting.scares, 1);
        assert!(!simulation.air.grains.is_empty());
    }

    #[test]
    fn a_window_in_front_sends_a_ghost_back_into_the_drift() {
        let mut simulation = haunted();
        let before = ground_snow(&simulation);
        for _ in 0..(4.0 / DT) as usize {
            simulation.step(DT, AWAY);
        }
        let ghost = simulation.haunting.ghosts[0];
        let (ground, center) = simulation.placement(&ghost, 0);
        simulation.layers[0].occluders = vec![[center.x - 10.0, ground + 5.0, center.x + 200.0, ground + 300.0]];
        simulation.step(DT, AWAY);
        assert!(simulation.haunting.ghosts.is_empty());
        let after = ground_snow(&simulation);
        assert!((after - before).abs() < before * 0.01, "its snow went back: {before} then {after}");
    }

    #[test]
    fn every_third_scare_brings_a_snowball() {
        let mut simulation = haunted();
        for scare in 0..3 {
            if scare == 1 {
                // One left alone in between goes back into the drift on its own, and the count carries on.
                simulation.haunting.next_ghost = 0.0;
                for _ in 0..(60.0 / DT) as usize {
                    simulation.step(DT, AWAY);
                    if simulation.haunting.ghosts.is_empty() {
                        break;
                    }
                }
                assert!(simulation.haunting.ghosts.is_empty() && !simulation.haunting.revenge_due);
            }
            simulation.haunting.next_ghost = 0.0;
            for _ in 0..(3.5 / DT) as usize {
                simulation.step(DT, AWAY);
            }
            let ghost = simulation.haunting.ghosts[0];
            let (ground, center) = simulation.placement(&ghost, 0);
            simulation.step(DT, V2::new(center.x, ground + ghost.size * 0.2) - super::super::CURSOR_OFFSET);
            for _ in 0..30 {
                simulation.step(DT, AWAY);
            }
        }
        assert!(simulation.haunting.revenge_due || simulation.haunting.ghosts.iter().any(|ghost| ghost.avenges));
        let mut hit = false;
        for _ in 0..(6.0 / DT) as usize {
            simulation.step(DT, AWAY);
            hit |= simulation.shake() != V2::ZERO;
        }
        assert!(hit, "the avenger's snowball hit the screen");
    }

    #[test]
    fn haunting_for_a_while_stays_sound() {
        let mut simulation = SnowSimulation::new(1200.0, 800.0, Snowfall::Heavy);
        for i in 0..simulation.layers[0].count() {
            simulation.layers[0].set(i, 25.0);
        }
        simulation.set_halloween(true);
        for frame in 0..(240.0 / DT) as usize {
            let t = frame as f32 * DT;
            let cursor = if ((t / 20.0) as usize).is_multiple_of(2) { AWAY } else { V2::new(600.0 + (t * 0.7).sin() * 500.0, 30.0) };
            simulation.step(DT, cursor);
            if frame == (200.0 / DT) as usize {
                simulation.blow_away(1.0, false);
            }
        }
        let mut buffer = vec![Sprite::default(); simulation.sprite_count() + 16];
        let count = simulation.write_sprites(&mut buffer);
        assert!(buffer[..count].iter().all(|sprite| sprite.position.x.is_finite() && sprite.position.y.is_finite()));
        assert!(simulation.layers[0].heights.iter().all(|height| height.is_finite()));
    }

    #[test]
    fn ghosts_hold_up_signs() {
        let mut simulation = haunted();
        let mut held = false;
        for _ in 0..(400.0 / DT) as usize {
            if simulation.haunting.ghosts.is_empty() {
                simulation.haunting.next_ghost = 0.0;
            }
            simulation.step(DT, AWAY);
            held = !simulation.signs().is_empty();
            if held {
                break;
            }
        }
        assert!(held, "a ghost held up a sign");
    }
}
