//! A snowman living in the wallpaper's picture, as the Mac's SnowmanBot: he wanders its ground, winds up and throws,
//! reels when hit and loses his hat, breaks apart when out or when a heart is lost, and rises whole elsewhere.

use super::ground::Ground;
use super::{ARM_RECT, BALLS, CROWN, REACH, SHOULDER};
use crate::geometry::{V2, smoothstep};

/// A 2 by 2 matrix by its columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat2 {
    pub x: V2,
    pub y: V2,
}

impl Mat2 {
    pub const IDENTITY: Mat2 = Mat2 { x: V2::new(1.0, 0.0), y: V2::new(0.0, 1.0) };

    pub fn rotation(angle: f32) -> Mat2 {
        Mat2 { x: V2::new(angle.cos(), angle.sin()), y: V2::new(-angle.sin(), angle.cos()) }
    }

    pub fn scale(x: f32, y: f32) -> Mat2 {
        Mat2 { x: V2::new(x, 0.0), y: V2::new(0.0, y) }
    }

    pub fn apply(self, v: V2) -> V2 {
        self.x * v.x + self.y * v.y
    }

    pub fn times(self, other: Mat2) -> Mat2 {
        Mat2 { x: self.apply(other.x), y: self.apply(other.y) }
    }

    pub fn scaled(self, by: f32) -> Mat2 {
        Mat2 { x: self.x * by, y: self.y * by }
    }

    pub fn inverse(self) -> Mat2 {
        let det = self.x.x * self.y.y - self.y.x * self.x.y;
        let inv = if det.abs() < 1e-12 { 0.0 } else { 1.0 / det };
        Mat2 { x: V2::new(self.y.y * inv, -self.x.y * inv), y: V2::new(-self.y.x * inv, self.x.x * inv) }
    }
}

/// Each part's place: what its picture covers, in parts of his height, turned and moved onto the screen as `matrix`
/// times that, plus `offset`, in points; and how much of it shows.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub matrix: Mat2,
    pub offset: V2,
    pub alpha: f32,
}

/// Each piece of him as he breaks apart, and what of the pictures it is: the body's, cut to one ball, or the arm's.
#[derive(Clone, Copy, Debug)]
pub struct Debris {
    pub placement: Placement,
    pub arm: bool,
    /// What of the body's picture the piece is, in parts of his height: left, bottom, right, top.
    pub rect: [f32; 4],
    /// The ball's middle and radius, outside which nothing of the body's picture shows, and one; zeros for the arm.
    pub cut: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
enum Act {
    Idle,
    WindingUp(f32),
    Throwing(f32),
    Reeling(f32),
}

#[derive(Clone, Copy, Debug)]
enum Hat {
    Worn { grow: f32 },
    Flying { position: V2, velocity: V2, turn: f32, spin: f32 },
    Lying { position: V2, turn: f32, since: f32 },
}

/// A piece of him flying apart: one of his balls, 0 to 2 from the bottom up, or his arm, 3.
#[derive(Clone, Copy, Debug)]
struct Piece {
    kind: usize,
    center: V2,
    velocity: V2,
    angle: f32,
    spin: f32,
    resting: bool,
}

/// A small generator of his whims, xorshift.
#[derive(Clone, Debug)]
pub struct Whim(u64);

impl Whim {
    pub fn new(seed: u64) -> Whim {
        Whim(seed | 1)
    }

    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next()
    }

    pub fn below(&mut self, count: usize) -> usize {
        ((self.next() * count as f32) as usize).min(count.saturating_sub(1))
    }
}

pub struct Bot {
    pub ground: Ground,
    /// Where his feet are, in the screen's points from its bottom left.
    pub foot: V2,
    path: Vec<V2>,
    pub facing: f32,
    stride: f32,
    rest_until: f64,
    next_throw: f64,
    whim: Whim,
    /// Throws back of his own when hit, as a bot does, never first; a friend's throws only when the friend does.
    pub throws_on_his_own: bool,
    /// Snowballs he owes for hits, to throw back one after another; three at most.
    owed: u32,
    /// Whom the snowball being wound up is for: a player in the picture, or none for the screen.
    target: Option<String>,
    /// Throws asked for while one is under way, each to follow in turn.
    queued: Vec<Option<String>>,
    /// Seconds the wind-up takes: shorter for throws following one after another.
    wind_up: f32,
    act: Act,
    arm_angle: f32,
    lean: f32,
    bounce: f32,
    hat: Hat,
    hat_fade: f32,
    /// Out of the round: he breaks apart and is gone until he comes back.
    pub knocked_out: bool,
    /// How far he still is from standing as he comes back, 1 under the snow to 0 up.
    melt: f32,
    /// Since when he has been on his way elsewhere for a heart lost.
    moving_for: Option<f32>,
    pieces: Vec<Piece>,
    broken_for: f32,
    /// Where snow bursts as he breaks or a piece of him strikes the ground, and how big a burst.
    bursts: Vec<(V2, f32)>,
}

impl Bot {
    /// One standing at `foot`.
    pub fn at(ground: Ground, foot: V2, now: f64, throws_on_his_own: bool, seed: u64) -> Bot {
        let mut whim = Whim::new(seed);
        let next_throw = now + whim.range(4.0, 8.0) as f64;
        Bot {
            ground,
            foot,
            path: Vec::new(),
            facing: 1.0,
            stride: 0.0,
            rest_until: now + 1.5,
            next_throw,
            whim,
            throws_on_his_own,
            owed: 0,
            target: None,
            queued: Vec::new(),
            wind_up: 0.5,
            act: Act::Idle,
            arm_angle: -0.45,
            lean: 0.0,
            bounce: 0.0,
            hat: Hat::Worn { grow: 1.0 },
            hat_fade: 1.0,
            knocked_out: false,
            melt: 0.0,
            moving_for: None,
            pieces: Vec::new(),
            broken_for: 0.0,
            bursts: Vec::new(),
        }
    }

    /// One standing somewhere on `ground`, neither far off nor close; none if it has nowhere.
    pub fn new(ground: Ground, now: f64, throws_on_his_own: bool, seed: u64) -> Option<Bot> {
        let mut whim = Whim::new(seed ^ 0x5DEE_CE66_D1CE);
        let start = ground.anywhere(&mut |count| whim.below(count), true, None)?;
        let foot = ground.point_of(start);
        Some(Bot::at(ground, foot, now, throws_on_his_own, seed))
    }

    pub fn take_bursts(&mut self) -> Vec<(V2, f32)> {
        std::mem::take(&mut self.bursts)
    }

    /// Winds up and throws at once, even reeling: at the screen, or at the player `target` in the picture. While a throw
    /// is under way, this one follows it, up to eight waiting.
    pub fn throw_now(&mut self, target: Option<String>) {
        match self.act {
            Act::Idle | Act::Reeling(_) => self.start(target, false),
            Act::WindingUp(_) | Act::Throwing(_) => {
                if self.queued.len() < 8 {
                    self.queued.push(target);
                }
            }
        }
    }

    fn start(&mut self, target: Option<String>, quickly: bool) {
        self.target = target;
        self.path.clear();
        self.wind_up = if quickly { 0.25 } else { 0.5 };
        self.act = Act::WindingUp(0.0);
    }

    /// The middle of him, where a snowball thrown at him goes.
    pub fn middle(&self) -> V2 {
        self.foot + V2::new(0.0, self.height() * 0.4)
    }

    /// His height where he stands, in points.
    pub fn height(&self) -> f32 {
        self.ground.height(self.near())
    }

    /// How near he is in the picture.
    pub fn near(&self) -> f32 {
        self.ground.near_at(self.foot)
    }

    /// The light he stands in.
    pub fn tint(&self) -> [f32; 3] {
        self.ground.tint_at(self.foot)
    }

    /// The bounds of all he draws, his hat flying off included, in the screen's points: left, bottom, right, top.
    pub fn reach(&self) -> [f32; 4] {
        let h = self.height();
        [self.foot.x - 0.8 * h, self.foot.y - 0.35 * h, self.foot.x + 0.8 * h, self.foot.y + 1.25 * h]
    }

    fn mirror(&self) -> Mat2 {
        Mat2::scale(self.facing, 1.0)
    }

    /// His body, his throwing arm, and his hat.
    pub fn placements(&self) -> (Placement, Placement, Placement) {
        let h = self.height();
        let mirror = self.mirror();
        let lean = Mat2::rotation(self.lean * self.facing);
        // Slumping, he sinks into the snow and squats down as he fades.
        let squat = Mat2::scale(1.0 + 0.15 * self.melt, 1.0 - 0.45 * self.melt);
        let body = lean.times(mirror).times(squat).scaled(h);
        let base = self.foot + V2::new(0.0, self.bounce * h - 0.35 * self.melt * h);
        let shown = 1.0 - self.melt;
        let shoulder = base + body.apply(SHOULDER);
        let arm = lean.times(mirror).times(Mat2::rotation(self.arm_angle)).scaled(h);
        let hat = match self.hat {
            Hat::Worn { grow } => {
                let matrix = lean.times(mirror).times(Mat2::rotation(0.08)).scaled(h * grow);
                Placement { matrix, offset: base + body.apply(CROWN), alpha: shown }
            }
            Hat::Flying { position, turn, .. } => Placement { matrix: Mat2::rotation(turn).times(mirror).scaled(h), offset: position, alpha: shown },
            Hat::Lying { position, turn, .. } => {
                Placement { matrix: Mat2::rotation(turn).times(mirror).scaled(h), offset: position, alpha: self.hat_fade * shown }
            }
        };
        (Placement { matrix: body, offset: base, alpha: shown }, Placement { matrix: arm, offset: shoulder, alpha: shown }, hat)
    }

    /// His throwing arm is raised behind him, winding up.
    pub fn arm_behind(&self) -> bool {
        matches!(self.act, Act::WindingUp(_))
    }

    /// The tip of his throwing arm, in points.
    pub fn hand(&self) -> V2 {
        let arm = self.placements().1;
        arm.offset + arm.matrix.apply(REACH)
    }

    /// Whether he is in pieces: out of the round, or on his way elsewhere.
    pub fn in_pieces(&self) -> bool {
        !self.pieces.is_empty()
    }

    /// Each piece of him as he breaks apart.
    pub fn debris(&self) -> Vec<Debris> {
        let h = self.height();
        let mirror = self.mirror();
        self.pieces
            .iter()
            .map(|piece| {
                let fade = if piece.kind == 0 { 1.0 - smoothstep(0.35, 1.2, self.broken_for) } else { 1.0 - smoothstep(1.0, 1.6, self.broken_for) };
                let matrix = Mat2::rotation(piece.angle).times(mirror).scaled(h);
                if piece.kind == 3 {
                    let middle = REACH * 0.5;
                    return Debris {
                        placement: Placement { matrix, offset: piece.center - matrix.apply(middle), alpha: fade },
                        arm: true,
                        rect: ARM_RECT,
                        cut: [0.0; 4],
                    };
                }
                let (middle, radius) = BALLS[piece.kind];
                // The bottom ball cracks and sinks into the snow where it stood.
                let squat = if piece.kind == 0 {
                    let broken = self.broken_for.min(1.0);
                    Mat2::scale(1.0 + 0.2 * broken, 1.0 - 0.45 * broken)
                } else {
                    Mat2::IDENTITY
                };
                let m = matrix.times(squat);
                let reach = radius * 1.25;
                Debris {
                    placement: Placement { matrix: m, offset: piece.center - m.apply(middle), alpha: fade },
                    arm: false,
                    rect: [middle.x - reach, middle.y - reach, middle.x + reach, middle.y + reach],
                    cut: [middle.x, middle.y, radius * 1.06, 1.0],
                }
            })
            .collect()
    }

    /// Whether a point is on him.
    pub fn contains(&self, point: V2) -> bool {
        if self.knocked_out || self.moving_for.is_some() {
            return false;
        }
        let body = self.placements().0;
        let local = body.matrix.inverse().apply(point - body.offset);
        let on_hat = matches!(self.hat, Hat::Worn { .. }) && local.x.abs() < 0.13 && local.y > 0.72 && local.y < 0.95;
        on_hat || BALLS.iter().any(|(middle, radius)| (local - *middle).length() < radius * 1.1)
    }

    /// Moves him on by `dt`. `can_throw` says whether he can be seen to throw now; `throw_ball` sends a snowball from his
    /// hand, as big across as it is there, at the player it is for, or at the screen for none. Returns whether anything of
    /// him moved.
    pub fn step(&mut self, dt: f32, now: f64, can_throw: bool, throw_ball: &mut dyn FnMut(V2, f32, Option<String>)) -> bool {
        let mut moved = self.step_hat(dt);
        if self.knocked_out {
            return self.break_apart(dt) || moved;
        }
        if let Some(before) = self.moving_for {
            // The hit's reel shows for a moment; then he breaks apart, and once his pieces are gone he rises whole
            // elsewhere.
            let since = before + dt;
            self.moving_for = Some(since);
            if since >= 0.35 {
                if before < 0.35 {
                    self.break_up();
                }
                if !self.break_apart(dt) {
                    self.rise_elsewhere(now);
                }
                return true;
            }
        }
        if self.melt > 0.0 {
            self.melt = (self.melt - dt / 0.5).max(0.0);
            moved = true;
        }
        match self.act {
            Act::Idle => {
                if !self.queued.is_empty() && self.melt == 0.0 {
                    let next = self.queued.remove(0);
                    self.start(next, true);
                    return true;
                }
                if self.throws_on_his_own && self.owed > 0 && can_throw && now >= self.next_throw {
                    self.wind_up = 0.5;
                    self.act = Act::WindingUp(0.0);
                    self.path.clear();
                    return true;
                }
                if !self.path.is_empty() {
                    self.walk(dt, now);
                    moved = true;
                } else {
                    if self.lean != 0.0 || self.bounce != 0.0 || self.arm_angle != -0.45 {
                        self.lean *= (1.0 - dt * 10.0).max(0.0);
                        self.bounce *= (1.0 - dt * 10.0).max(0.0);
                        self.arm_angle += (-0.45 - self.arm_angle) * (dt * 8.0).min(1.0);
                        if self.lean.abs() < 0.001 {
                            self.lean = 0.0;
                        }
                        if self.bounce < 0.0005 {
                            self.bounce = 0.0;
                        }
                        if (self.arm_angle + 0.45).abs() < 0.002 {
                            self.arm_angle = -0.45;
                        }
                        moved = true;
                    }
                    if now >= self.rest_until {
                        self.wander(now);
                    }
                }
            }
            Act::WindingUp(t) => {
                let t = t + dt / self.wind_up;
                let ease = smoothstep(0.0, 1.0, t);
                self.arm_angle = -0.45 + (2.35 + 0.45) * ease;
                self.lean = -0.07 * ease;
                self.act = if t >= 1.0 { Act::Throwing(0.0) } else { Act::WindingUp(t) };
                moved = true;
            }
            Act::Throwing(t) => {
                if t == 0.0 {
                    throw_ball(self.hand(), self.height() * 0.1, self.target.take());
                    self.owed = self.owed.saturating_sub(1);
                    self.next_throw = now + self.whim.range(0.6, 1.2) as f64;
                }
                let t = t + dt / 0.16;
                self.arm_angle = 2.35 - (2.35 + 1.0) * t.min(1.0);
                self.lean = -0.07 + 0.17 * t.min(1.0);
                self.act = if t >= 1.0 { Act::Idle } else { Act::Throwing(t) };
                if t >= 1.0 {
                    self.rest_until = now + self.whim.range(0.8, 2.0) as f64;
                    if !self.queued.is_empty() {
                        let next = self.queued.remove(0);
                        self.start(next, true);
                    }
                }
                moved = true;
            }
            Act::Reeling(t) => {
                let t = t + dt;
                self.lean = 0.28 * (t * 17.0).sin() * (-t * 3.6).exp();
                self.arm_angle = -0.45 + 0.9 * (t * 11.0).sin() * (-t * 3.0).exp();
                if t >= 1.1 {
                    self.act = Act::Idle;
                    self.rest_until = now + 0.4;
                } else {
                    self.act = Act::Reeling(t);
                }
                moved = true;
            }
        }
        moved
    }

    fn wander(&mut self, now: f64) {
        let start = self.ground.cell_at(self.foot);
        for _ in 0..6 {
            let whim = &mut self.whim;
            let Some(goal) = self.ground.anywhere(&mut |count| whim.below(count), false, Some(start)) else { return };
            let distance = (((goal.0 - start.0).pow(2) + (goal.1 - start.1).pow(2)) as f32).sqrt();
            if distance <= 12.0 || distance >= 90.0 {
                continue;
            }
            let found = self.ground.path(start, goal);
            if !found.is_empty() {
                self.path = found;
                return;
            }
        }
        self.rest_until = now + 2.0;
    }

    fn walk(&mut self, dt: f32, now: f64) {
        let Some(&next) = self.path.first() else { return };
        // Seen in perspective, he covers less of the screen the farther off he is.
        let speed = 0.42 * self.height();
        let way = next - self.foot;
        let distance = way.length();
        if way.x.abs() > 1.0 {
            self.facing = if way.x > 0.0 { 1.0 } else { -1.0 };
        }
        if distance <= speed * dt {
            self.foot = next;
            self.path.remove(0);
            if self.path.is_empty() {
                self.rest_until = now + self.whim.range(1.5, 5.0) as f64;
            }
        } else {
            self.foot = self.foot + way * (speed * dt / distance);
        }
        // He waddles: rocks from side to side and bobs with each step.
        self.stride += dt * 9.0;
        self.lean = self.stride.sin() * 0.06;
        self.bounce = self.stride.sin().abs() * 0.022;
    }

    /// Out of the round: he breaks apart, and does nothing more until he comes back.
    pub fn knock_out(&mut self) {
        if self.knocked_out {
            return;
        }
        self.knocked_out = true;
        let broken = self.moving_for.is_some() && self.in_pieces();
        self.moving_for = None;
        self.melt = 0.0;
        self.path.clear();
        self.queued.clear();
        self.owed = 0;
        self.target = None;
        self.act = Act::Idle;
        if !broken {
            self.break_up();
        }
    }

    fn break_up(&mut self) {
        // He comes apart where he stands: his head pops up and off, his middle topples, his arm flies, his bottom cracks
        // where it is; all to one side.
        let h = self.height();
        let (body, arm, hat) = self.placements();
        let side = if self.whim.next() < 0.5 { 1.0 } else { -1.0 };
        let whim = &mut self.whim;
        let mut spread = || whim.range(0.8, 1.2);
        let angle = self.lean * self.facing;
        self.pieces = vec![
            Piece { kind: 0, center: body.offset + body.matrix.apply(BALLS[0].0), velocity: V2::ZERO, angle, spin: 0.0, resting: true },
            Piece {
                kind: 1,
                center: body.offset + body.matrix.apply(BALLS[1].0),
                velocity: V2::new(side * 0.45 * spread(), 0.55 * spread()) * h,
                angle,
                spin: -side * 3.2 * spread(),
                resting: false,
            },
            Piece {
                kind: 2,
                center: body.offset + body.matrix.apply(BALLS[2].0),
                velocity: V2::new(-side * 0.35 * spread(), 1.35 * spread()) * h,
                angle,
                spin: side * 7.0 * spread(),
                resting: false,
            },
            Piece {
                kind: 3,
                center: arm.offset + arm.matrix.apply(REACH * 0.5),
                velocity: V2::new(side * 0.9 * spread(), 1.05 * spread()) * h,
                angle: self.arm_angle + angle,
                spin: -side * 11.0 * spread(),
                resting: false,
            },
        ];
        self.broken_for = 0.0;
        self.bursts.push((body.offset + body.matrix.apply(BALLS[1].0), 0.5 * h));
        if matches!(self.hat, Hat::Worn { .. }) {
            self.hat = Hat::Flying { position: hat.offset, velocity: V2::new(side * 0.4, 1.7) * h, turn: 0.08 * self.facing, spin: side * -6.0 };
        }
    }

    /// The pieces fly, strike the ground and bounce, then roll to a stop and melt away. Returns whether any moved.
    fn break_apart(&mut self, dt: f32) -> bool {
        if self.broken_for >= 1.7 {
            return false;
        }
        self.broken_for += dt;
        let h = self.height();
        let foot = self.foot;
        for piece in self.pieces.iter_mut().filter(|piece| !piece.resting || piece.kind != 0) {
            let radius = if piece.kind == 3 { 0.02 } else { BALLS[piece.kind].1 };
            let floor = foot.y + radius * h * 0.85;
            if !piece.resting {
                piece.velocity.y -= 5.0 * h * dt;
                piece.center = piece.center + piece.velocity * dt;
                piece.angle += piece.spin * dt;
                if piece.center.y < floor && piece.velocity.y < 0.0 {
                    piece.center.y = floor;
                    if -piece.velocity.y > 0.5 * h {
                        // A good thump: it bounces, and snow flies where it struck.
                        self.bursts.push((V2::new(piece.center.x, foot.y), if piece.kind == 3 { 0.12 } else { radius * 1.6 } * h));
                        piece.velocity.y *= -0.3;
                        piece.velocity.x *= 0.7;
                    } else {
                        piece.velocity.y = 0.0;
                        piece.resting = true;
                    }
                }
            } else if piece.kind != 3 {
                // Rolling to a stop along the ground.
                piece.velocity.x *= (1.0 - 2.2 * dt).max(0.0);
                piece.center.x += piece.velocity.x * dt;
                piece.angle -= piece.velocity.x / (radius * h).max(1.0) * dt;
            }
        }
        true
    }

    /// Back for the next round: he rises out of the snow somewhere else, whole, his hat on.
    pub fn come_back(&mut self) {
        if !self.knocked_out {
            return;
        }
        self.knocked_out = false;
        self.moving_for = None;
        self.pieces.clear();
        let whim = &mut self.whim;
        if let Some(cell) = self.ground.anywhere(&mut |count| whim.below(count), true, None) {
            self.foot = self.ground.point_of(cell);
        }
        self.path.clear();
        self.melt = 1.0;
        self.hat_fade = 1.0;
        self.hat = Hat::Worn { grow: 0.0 };
    }

    /// A heart less: once the hit's reel has shown, he breaks apart as when he is out, and once his pieces are gone he
    /// rises out of the snow somewhere else, well away. Nothing hits him until he is up there.
    pub fn move_away(&mut self) {
        if self.knocked_out || self.moving_for.is_some() {
            return;
        }
        self.moving_for = Some(0.0);
        self.path.clear();
    }

    fn rise_elsewhere(&mut self, now: f64) {
        self.moving_for = None;
        self.pieces.clear();
        let from = self.foot;
        let mut farthest: Option<(V2, f32)> = None;
        for _ in 0..10 {
            let whim = &mut self.whim;
            let Some(cell) = self.ground.anywhere(&mut |count| whim.below(count), true, None) else { break };
            let point = self.ground.point_of(cell);
            let distance = (point - from).length();
            if farthest.is_none_or(|(_, far)| distance > far) {
                farthest = Some((point, distance));
            }
            if distance > self.ground.screen.x * 0.3 {
                break;
            }
        }
        if let Some((point, _)) = farthest {
            self.foot = point;
        }
        // A throw he was winding up for a friend is still owed.
        if matches!(self.act, Act::WindingUp(_)) && !self.throws_on_his_own {
            self.queued.insert(0, self.target.clone());
        }
        self.act = Act::Idle;
        self.path.clear();
        self.arm_angle = -0.45;
        self.lean = 0.0;
        self.bounce = 0.0;
        self.melt = 1.0;
        self.rest_until = now + 1.2;
        self.bursts.push((self.foot, 0.25 * self.height()));
    }

    /// Hit by a snowball: he reels, and his hat flies off.
    pub fn hit(&mut self, point: V2, now: f64) {
        // A throw he was winding up for a friend is still owed: every hit the friend scores is thrown.
        if matches!(self.act, Act::WindingUp(_)) && !self.throws_on_his_own && self.queued.len() < 8 {
            self.queued.insert(0, self.target.clone());
        }
        self.act = Act::Reeling(0.0);
        self.path.clear();
        if self.throws_on_his_own {
            if self.owed == 0 {
                self.next_throw = now + self.whim.range(1.4, 2.4) as f64;
            }
            self.owed = (self.owed + 1).min(3);
        }
        if matches!(self.hat, Hat::Worn { .. }) {
            let place = self.placements().2;
            let h = self.height();
            let away = if point.x < self.foot.x { 1.0 } else { -1.0 };
            let velocity = V2::new(away * self.whim.range(0.5, 1.0), self.whim.range(1.4, 2.0)) * h;
            let spin = away * -self.whim.range(5.0, 9.0);
            self.hat = Hat::Flying { position: place.offset, velocity, turn: 0.08 * self.facing, spin };
        }
    }

    fn step_hat(&mut self, dt: f32) -> bool {
        let h = self.height();
        match self.hat {
            Hat::Worn { grow } => {
                if grow >= 1.0 {
                    return false;
                }
                self.hat = Hat::Worn { grow: (grow + dt / 0.35).min(1.0) };
                true
            }
            Hat::Flying { mut position, mut velocity, mut turn, spin } => {
                velocity.y -= 5.5 * h * dt;
                position = position + velocity * dt;
                turn += spin * dt;
                let floor = self.foot.y + 0.01 * h;
                if position.y <= floor && velocity.y < 0.0 {
                    position.y = floor;
                    let turn = (turn % (2.0 * std::f32::consts::PI)) * 0.3 + if spin > 0.0 { 0.4 } else { -0.4 };
                    self.hat = Hat::Lying { position, turn, since: 0.0 };
                } else {
                    self.hat = Hat::Flying { position, velocity, turn, spin };
                }
                true
            }
            Hat::Lying { position, turn, since } => {
                let since = since + dt;
                self.hat_fade = if since < 3.5 { 1.0 } else { (1.0 - (since - 3.5) / 0.6).max(0.0) };
                if since >= 4.1 {
                    self.hat_fade = 1.0;
                    self.hat = Hat::Worn { grow: 0.0 };
                } else {
                    self.hat = Hat::Lying { position, turn, since };
                }
                since >= 3.5
            }
        }
    }
}
