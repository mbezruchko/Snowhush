//! Fireworks on the desktop through the New Year's season: rockets rise three a minute, more often the nearer the New
//! Year is, twenty a minute at most about midnight, and burst into sparks that droop and fade, the burst flashing and
//! lighting the snow in its colour. They are seen only where the desktop is, behind the windows. With a wallpaper whose
//! depth is known, most are let off far away in it, from behind whatever in the picture is nearer: the farther, the
//! smaller and lower. The Mac app has the same in Swift.

use std::f32::consts::{PI, TAU};

use super::Random;
use crate::geometry::V2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    Peony,
    Willow,
    Ring,
    Crackle,
}

const SHELLS: [Shell; 4] = [Shell::Peony, Shell::Willow, Shell::Ring, Shell::Crackle];

#[derive(Clone, Copy, Debug)]
pub struct Spark {
    pub position: V2,
    pub velocity: V2,
    pub color: [f32; 3],
    pub age: f32,
    pub life: f32,
    pub size: f32,
    drag: f32,
    gravity: f32,
    /// Crackles, flashing white, toward its end.
    pub crackles: bool,
    /// A rising rocket, which bursts at the top of its climb into this.
    pub shell: Option<Shell>,
    /// The sky lit up round a burst for a moment: drawn as a wide glow only.
    pub flash: bool,
    /// How near in the wallpaper's picture it is, hidden behind anything nearer; below zero in front of it all.
    pub near: f32,
    /// How big it looks for being that far off: 1 right in front.
    pub scale: f32,
}

impl Spark {
    #[allow(clippy::too_many_arguments)]
    fn new(position: V2, velocity: V2, color: [f32; 3], life: f32, size: f32, drag: f32, gravity: f32) -> Spark {
        Spark { position, velocity, color, age: 0.0, life, size, drag, gravity, crackles: false, shell: None, flash: false, near: -1.0, scale: 1.0 }
    }

    /// How brightly it shines now: `flicker`, picked at random from 0 to 1, makes a crackling one blink.
    pub fn brightness(&self, flicker: f32) -> f32 {
        let share = (self.age / self.life).min(1.0);
        let mut shine = 1.0 - share * share;
        if self.crackles && share > 0.6 {
            shine *= if flicker < 0.35 { 1.8 } else { 0.15 };
        }
        shine
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Flash {
    pub position: V2,
    pub color: [f32; 3],
    pub strength: f32,
}

const COLORS: [[f32; 3]; 8] =
    [[1.0, 0.25, 0.2], [0.3, 1.0, 0.45], [0.35, 0.55, 1.0], [1.0, 0.8, 0.35], [0.85, 0.4, 1.0], [1.0, 1.0, 1.0], [0.3, 0.95, 1.0], [1.0, 0.5, 0.8]];
pub const MAX_SPARKS: usize = 1600;

pub struct Fireworks {
    pub sparks: Vec<Spark>,
    pub flashes: Vec<Flash>,
    /// How near the farthest and the nearest of the wallpaper are, when its depth is known.
    pub span: Option<(f32, f32)>,
    width: f32,
    height: f32,
    /// Where rockets may rise: clear of the tree on the left.
    left: f32,
    clock: f32,
    next_launch: f32,
    random: Random,
}

impl Fireworks {
    pub fn new(width: f32, height: f32, left: f32) -> Fireworks {
        Fireworks {
            sparks: Vec::new(),
            flashes: Vec::new(),
            span: None,
            width,
            height,
            left: left.min(width * 0.5),
            clock: 0.0,
            next_launch: -1.0,
            random: Random::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sparks.is_empty()
    }

    /// Seconds to the next rocket at `per_minute` a minute, never less than three: twenty a minute at most.
    fn wait(&mut self, per_minute: f32) -> f32 {
        if per_minute <= 0.0 {
            return f32::INFINITY;
        }
        (60.0 / per_minute * self.random.range(0.6, 1.4)).max(3.0)
    }

    /// Moves them on by `dt`, letting off rockets `per_minute` on average, and one right away at `midnight`.
    /// `behind(x, near)` says where, from the screen's bottom, the nearest thing in the picture at least as near as
    /// `near` comes up to at `x`, for rockets let off far away in it.
    pub fn step(&mut self, dt: f32, per_minute: f32, midnight: bool, behind: impl Fn(f32, f32) -> Option<f32>) {
        self.clock += dt;
        if self.next_launch < 0.0 || midnight {
            self.next_launch = if midnight { self.clock } else { self.clock + self.wait(per_minute) };
        }
        if per_minute > 0.0 && self.clock >= self.next_launch && self.sparks.len() < MAX_SPARKS {
            self.launch(&behind);
            self.next_launch = self.clock + self.wait(per_minute);
        }

        let mut bursts: Vec<(V2, Shell, f32, f32)> = Vec::new();
        let mut i = self.sparks.len();
        while i > 0 {
            i -= 1;
            let mut spark = self.sparks[i];
            spark.age += dt;
            spark.velocity *= (-spark.drag * dt).exp();
            spark.velocity.y -= spark.gravity * dt;
            spark.position += spark.velocity * dt;
            if let Some(shell) = spark.shell {
                // A rocket leaves a short trail of embers as it climbs, and bursts as it slows.
                if self.sparks.len() < MAX_SPARKS && self.random.next() < dt * 50.0 {
                    let s = spark.scale;
                    let random = &mut self.random;
                    let mut ember = Spark::new(
                        spark.position,
                        V2::new(random.range(-12.0, 12.0), -20.0) * s,
                        [1.0, 0.7, 0.35],
                        random.range(0.25, 0.45),
                        1.1 * (0.55 + 0.45 * s),
                        3.0,
                        60.0 * s,
                    );
                    ember.near = spark.near;
                    ember.scale = s;
                    self.sparks.push(ember);
                }
                if spark.velocity.y < 40.0 * spark.scale || spark.age > 3.0 {
                    bursts.push((spark.position, shell, spark.near, spark.scale));
                    self.sparks.swap_remove(i);
                    continue;
                }
            } else if spark.age >= spark.life {
                self.sparks.swap_remove(i);
                continue;
            }
            self.sparks[i] = spark;
        }
        for (position, shell, near, scale) in bursts {
            self.burst(shell, position, near, scale);
        }
        for k in (0..self.flashes.len()).rev() {
            self.flashes[k].strength *= (-dt / 0.35).exp();
            if self.flashes[k].strength < 0.02 {
                self.flashes.remove(k);
            }
        }
    }

    fn shell(&mut self) -> Shell {
        SHELLS[((self.random.next() * SHELLS.len() as f32) as usize).min(SHELLS.len() - 1)]
    }

    fn color(&mut self) -> [f32; 3] {
        COLORS[((self.random.next() * COLORS.len() as f32) as usize).min(COLORS.len() - 1)]
    }

    fn launch(&mut self, behind: &impl Fn(f32, f32) -> Option<f32>) {
        // A third go up right in front, from behind the drifts; the rest somewhere in the picture's depth.
        if let Some((far, close)) = self.span
            && self.random.next() > 0.33
        {
            let random = &mut self.random;
            let along = random.range(0.0, 0.85);
            let near = far + (close - far) * along;
            let scale = 0.32 + 0.68 * along;
            let x = random.range(self.width * 0.04, self.width * 0.96);
            // From just below the ridge in front of it, so that it comes up from behind it.
            let base = behind(x, near).unwrap_or(0.0) - 24.0 * scale;
            let top = (base + 24.0 * scale + random.range(0.18, 0.42) * self.height * scale).min(self.height * 0.92);
            let rise = 240.0 * scale;
            let mut rocket = Spark::new(
                V2::new(x, base),
                V2::new(random.range(-35.0, 35.0) * scale, (2.0 * rise * (top - base).max(20.0)).sqrt() * 1.05),
                [1.0, 0.85, 0.6],
                4.0,
                1.6 * (0.55 + 0.45 * scale),
                0.0,
                rise,
            );
            rocket.shell = Some(self.shell());
            rocket.near = near;
            rocket.scale = scale;
            self.sparks.push(rocket);
            return;
        }
        let random = &mut self.random;
        let top = random.range(0.45, 0.85) * self.height;
        let rise = 240.0;
        let x = random.range(self.left.max(self.width * 0.08), self.width * 0.94);
        let mut rocket =
            Spark::new(V2::new(x, -4.0), V2::new(random.range(-35.0, 35.0), (2.0 * rise * top).sqrt() * 1.05), [1.0, 0.85, 0.6], 4.0, 1.6, 0.0, rise);
        rocket.shell = Some(self.shell());
        self.sparks.push(rocket);
    }

    fn burst(&mut self, shell: Shell, position: V2, near: f32, scale: f32) {
        let first = self.sparks.len();
        let color = self.color();
        let second = if self.random.next() < 0.5 { self.color() } else { color };
        let room = MAX_SPARKS.saturating_sub(self.sparks.len());
        match shell {
            Shell::Peony | Shell::Crackle => {
                let count = room.min(100);
                let speed = self.random.range(170.0, 230.0);
                for k in 0..count {
                    let random = &mut self.random;
                    let angle = k as f32 / count as f32 * TAU + random.range(-0.05, 0.05);
                    // Seen from the side, a sphere of sparks: those flying toward or away from the viewer seem slower.
                    let depth = random.range(-1.0, 1.0);
                    let along = speed * (1.0 - depth * depth).sqrt();
                    let life = random.range(1.4, 2.0);
                    let mut spark = Spark::new(
                        position,
                        V2::new(angle.cos(), angle.sin()) * along,
                        if k % 2 == 0 { color } else { second },
                        life,
                        2.6,
                        1.5,
                        90.0,
                    );
                    spark.crackles = shell == Shell::Crackle;
                    self.sparks.push(spark);
                }
            }
            Shell::Willow => {
                let count = room.min(64);
                for k in 0..count {
                    let random = &mut self.random;
                    let angle = k as f32 / count as f32 * TAU;
                    let speed = random.range(90.0, 170.0);
                    let life = random.range(2.6, 3.4);
                    self.sparks.push(Spark::new(position, V2::new(angle.cos(), angle.sin()) * speed, [1.0, 0.72, 0.32], life, 2.2, 2.2, 70.0));
                }
            }
            Shell::Ring => {
                let count = room.min(48);
                let tilt = self.random.range(0.0, PI);
                let squash = self.random.range(0.35, 0.8);
                for k in 0..count {
                    let angle = k as f32 / count as f32 * TAU;
                    let flat = V2::new(angle.cos(), angle.sin() * squash) * 190.0;
                    let velocity = V2::new(flat.x * tilt.cos() - flat.y * tilt.sin(), flat.x * tilt.sin() + flat.y * tilt.cos());
                    let life = self.random.range(1.5, 1.9);
                    self.sparks.push(Spark::new(position, velocity, color, life, 2.6, 1.4, 80.0));
                }
            }
        }
        let tone = if shell == Shell::Willow { [1.0, 0.72, 0.32] } else { color };
        self.flashes.push(Flash { position, color: tone, strength: scale });
        if self.sparks.len() < MAX_SPARKS {
            let mut glow = Spark::new(position, V2::ZERO, tone, 0.45, 70.0, 0.0, 0.0);
            glow.flash = true;
            self.sparks.push(glow);
        }
        // Far off, the same burst is smaller all through: slower, falling less, its sparks finer.
        if scale == 1.0 && near < 0.0 {
            return;
        }
        for spark in &mut self.sparks[first..] {
            spark.velocity *= scale;
            spark.size *= if spark.flash { scale } else { 0.55 + 0.45 * scale };
            spark.gravity *= scale;
            spark.near = near;
            spark.scale = scale;
        }
    }

    /// A sparkler held at `point`: a few tiny golden sparks spit out of it every frame.
    pub fn sparkle(&mut self, point: V2, dt: f32) {
        let count = (dt * 140.0 + self.random.next()) as usize;
        for _ in 0..count {
            if self.sparks.len() >= MAX_SPARKS {
                break;
            }
            let random = &mut self.random;
            let angle = random.range(0.0, TAU);
            let speed = random.range(40.0, 120.0);
            let life = random.range(0.15, 0.35);
            let mut spark = Spark::new(point, V2::new(angle.cos(), angle.sin()) * speed, [1.0, 0.85, 0.5], life, 0.9, 4.0, 120.0);
            spark.crackles = true;
            self.sparks.push(spark);
        }
    }

    /// The brightest flashes, at most three.
    pub fn lights(&self) -> Vec<Flash> {
        let mut flashes = self.flashes.clone();
        flashes.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        flashes.truncate(3);
        flashes
    }
}
