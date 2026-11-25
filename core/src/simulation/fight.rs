//! The snowball fight's side of the simulation, as the Mac's SnowSimulation has it: snowballs thrown into the wallpaper's
//! picture, shrinking as they go off and hidden behind what is nearer, bursting where they come down; snowballs thrown
//! out of it at the screen; and the snow a snowman throws up as he breaks apart.

use std::f32::consts::PI;

use super::{MAX_MOTES, Mote, SnowSimulation, Sprite};
use crate::geometry::{V2, smoothstep};
use crate::wallpaper::GROUND;

/// A snowball thrown into the wallpaper's picture: it flies off from in front of the screen, or from someone in the
/// picture, in an arc, shrinking as it goes, to come down at `to`, where the picture is `near`.
#[derive(Clone, Copy, Debug)]
struct PictureBall {
    token: u64,
    from: V2,
    to: V2,
    near: f32,
    start: f32,
    end: f32,
    duration: f32,
    /// Thrown from in front of the screen: on its way into the picture it passes where the windows stand.
    from_screen: bool,
    age: f32,
    /// Gone past where the windows stand, into the picture: behind them from then on.
    crossed: bool,
}

impl PictureBall {
    /// How far along it is, 0 to 1.
    fn flown(&self) -> f32 {
        (self.age / self.duration).min(1.0)
    }

    /// How far along its way across the screen it is, slowing into the distance.
    fn along(&self) -> f32 {
        1.0 - (1.0 - self.flown()).powf(1.7)
    }

    /// Where it is, how big, and how near in the picture.
    fn placement(&self) -> (V2, f32, f32) {
        let t = self.flown();
        // Seen in perspective it covers most of the way across the screen early on, and slows into the distance.
        let along = self.along();
        let rise = (self.to - self.from).length() * 0.22 + 40.0;
        let position = self.from + (self.to - self.from) * along + V2::new(0.0, (along * PI).sin() * rise * (1.0 - 0.4 * t));
        let size = 1.0 / (1.0 / self.start + (1.0 / self.end - 1.0 / self.start) * along);
        // In front of the whole picture while it is near the screen, at its own depth by the time it comes down.
        let scene = 1.0 - (1.0 - self.near) * smoothstep(0.35, 1.0, along);
        (position, size, scene)
    }
}

/// A snowball come down in the picture: the one `throw_into_picture` gave, where it came down, how near the picture is
/// there and how big it was. Whoever threw it says whether it hit something; if not, `splat` bursts it there.
#[derive(Clone, Copy, Debug)]
pub struct Arrival {
    pub token: u64,
    pub at: V2,
    pub near: f32,
    pub size: f32,
}

#[derive(Default)]
pub(super) struct Fight {
    balls: Vec<PictureBall>,
    arrived: Vec<Arrival>,
    next: u64,
    /// Something in the picture wants the snow going, as a figure walking in it: the simulation does not rest.
    pub keeps_awake: bool,
}

impl Fight {
    pub fn busy(&self) -> bool {
        !self.balls.is_empty() || self.keeps_awake
    }

    pub fn sprite_count(&self) -> usize {
        self.balls.len()
    }
}

impl SnowSimulation {
    /// Something in the picture wants the snow going, as a snowman walking in it.
    pub fn set_keeps_awake(&mut self, awake: bool) {
        self.fight.keeps_awake = awake;
    }

    /// Throws a snowball from `from` into the picture, to come down at `to` where the picture is `near`, `size` across
    /// when it gets there and `start_size` as it sets off: from in front of the screen, or, with `origin`, from someone in
    /// the picture as near as that. The farther it has to go, the longer it flies. Returns what tells it apart when it
    /// comes down.
    pub fn throw_into_picture(&mut self, from: V2, to: V2, near: f32, size: f32, start_size: f32, origin: Option<f32>) -> u64 {
        let far = self.picture.farness(near);
        let duration = match origin {
            Some(origin) => 0.45 + 1.1 * (self.picture.farness(origin) - far).abs() + 0.6 * far + (to - from).length() / 2400.0,
            None => 0.45 + 1.4 * far.powf(1.2),
        };
        self.fight.next += 1;
        let token = self.fight.next;
        self.fight.balls.push(PictureBall {
            token,
            from,
            to,
            near,
            start: start_size,
            end: size,
            duration,
            from_screen: origin.is_none(),
            age: 0.0,
            crossed: false,
        });
        token
    }

    /// Throws a snowball out of the picture at the screen, from `point` where it is `size` across, to burst against the
    /// glass about the middle of the screen: from `near` in the picture, the farther off, the longer it flies.
    pub fn throw_at_screen(&mut self, point: V2, size: f32, near: Option<f32>) {
        let random = &mut self.air.random;
        let target = V2::new(random.range(0.25, 0.75) * self.width, random.range(0.3, 0.75) * self.height);
        let flight = near.map_or(0.5, |near| 0.5 + 1.3 * self.picture.farness(near).powf(1.2));
        self.throw_snowball(point, target, size, flight);
    }

    /// The snowballs come down in the picture since this was last asked.
    pub fn take_arrivals(&mut self) -> Vec<Arrival> {
        std::mem::take(&mut self.fight.arrived)
    }

    /// Whether a window or the panel stands in front of the desktop at a point.
    pub fn desktop_covered(&self, point: V2) -> bool {
        let panel = self.layers[0].base().y;
        point.y < panel || self.covers.iter().any(|frame| point.x >= frame[0] && point.x <= frame[2] && point.y >= frame[1] && point.y <= frame[3])
    }

    pub(super) fn fly_picture_balls(&mut self, dt: f32) {
        let mut i = self.fight.balls.len();
        while i > 0 {
            i -= 1;
            self.fight.balls[i].age += dt;
            let ball = self.fight.balls[i];
            // Thrown from the screen, it passes where the windows stand a third of the way: one there stops it, and it
            // bursts on its glass.
            if ball.from_screen && !ball.crossed && ball.along() >= 0.3 {
                self.fight.balls[i].crossed = true;
                let (position, size, _) = ball.placement();
                if self.desktop_covered(position) {
                    self.fight.balls.remove(i);
                    self.shatter(position, (size / 70.0).clamp(0.3, 0.6), false);
                    continue;
                }
            }
            if ball.age >= ball.duration {
                self.fight.balls.remove(i);
                if ball.near > GROUND {
                    self.fight.arrived.push(Arrival { token: ball.token, at: ball.to, near: ball.near, size: ball.end });
                }
            }
        }
    }

    /// A snowball come down in the picture without hitting anything: snow lies where it burst, and a puff goes up.
    pub fn splat(&mut self, at: V2, near: f32, size: f32) {
        if self.picture_frost {
            self.picture.splat(at, size, self.height);
        }
        self.puff_at(at, near, size);
    }

    /// A snowman breaking apart, or a piece of him striking the ground: chunks of snow fly up out of `point` and drop to
    /// `floor`, powder with them, all at its nearness in the picture.
    pub fn crumble_snowman(&mut self, point: V2, floor: f32, near: f32, size: f32) {
        for _ in 0..(10.0 + size * 0.5) as usize {
            if self.air.motes.len() >= MAX_MOTES {
                break;
            }
            let random = &mut self.air.random;
            let angle = random.range(0.2, PI - 0.2);
            let speed = random.range(0.6, 1.6) * size * 3.0;
            let offset = V2::new(random.range(-0.4, 0.4), random.range(-0.2, 0.3)) * size;
            let mote = Mote {
                position: point + offset,
                velocity: V2::new(angle.cos(), angle.sin()) * speed,
                size: random.range(0.06, 0.16) * size,
                alpha: random.range(0.8, 1.0),
                life: random.range(1.0, 1.8),
                scene: Some((near, 1.0)),
                chunk: Some(floor + random.range(-0.05, 0.05) * size),
                ..Default::default()
            };
            self.air.motes.push(mote);
        }
        self.puff_at(point, near, size * 0.5);
    }

    /// Powder thrown up where a snowball burst in the picture, at its nearness there.
    pub fn puff_at(&mut self, point: V2, near: f32, size: f32) {
        for _ in 0..14 {
            if self.air.motes.len() >= MAX_MOTES {
                break;
            }
            let random = &mut self.air.random;
            let angle = random.range(0.15, PI - 0.15);
            let speed = random.range(0.6, 1.6) * size * 2.2;
            let offset = V2::new(random.range(-0.3, 0.3), random.range(0.0, 0.3)) * size;
            let mote = Mote {
                position: point + offset,
                velocity: V2::new(angle.cos(), angle.sin()) * speed,
                size: random.range(0.25, 0.5) * size,
                alpha: random.range(0.5, 0.8),
                life: random.range(0.5, 0.9),
                scene: Some((near, 1.0)),
                ..Default::default()
            };
            self.air.motes.push(mote);
        }
    }

    pub(super) fn write_fight(&self, buffer: &mut [Sprite]) -> usize {
        let mut count = 0;
        for ball in &self.fight.balls {
            if count >= buffer.len() {
                break;
            }
            let (position, size, scene) = ball.placement();
            // Once in the picture, it is behind the windows.
            if (ball.crossed || !ball.from_screen) && self.desktop_covered(position) {
                continue;
            }
            buffer[count] =
                Sprite { position, size, alpha: 1.0, soft: 0.14, tint: [1.0; 3], scene, scene_light: 1.0 - ball.flown(), ..Sprite::default() };
            count += 1;
        }
        count
    }
}
