//! A snowball fight with snowmen living in the wallpaper's picture, as the Mac's SnowballFight: a bot while there is no
//! one to play with, who throws back at whoever hits him, never first. A click on the wallpaper throws a snowball into
//! the picture; the faster throws follow one another, the farther they stray. Each heart the bot loses, he breaks apart
//! and stands whole somewhere else; out of hearts, he is out for a few seconds.
//!
//! The snowmen stand on the display, in its points from its bottom left; the simulation may cover only part of it, from
//! `offset` up and across, and is handed points in its own.

use std::collections::HashMap;

use super::bot::{Bot, Whim};
use super::ground::Ground;
use super::{ARM_RECT, BODY_RECT, HAT_RECT};
use crate::geometry::V2;
use crate::simulation::SnowSimulation;

/// Hits the bot takes before he is out: two hearts' worth, moving elsewhere after the first.
pub const BOT_HITS: u32 = 6;
/// Hits a heart takes.
pub const HITS_PER_HEART: u32 = 3;
/// How long the bot is out before he comes back, in seconds.
const BOT_BACK: f64 = 5.0;
const BOT: &str = "bot";

/// Which of a snowman's pictures a part is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picture {
    Body,
    Arm,
    Hat,
}

/// One part of a snowman to draw: what of its picture it covers, in parts of his height, `rect`, of the picture's own
/// rect `whole`, turned and moved onto the display by `matrix` (its columns) and `offset`, in points; how much of it
/// shows, and a circle outside which nothing shows, middle, radius and one, for a ball cut out of the body.
#[derive(Clone, Copy, Debug)]
pub struct Part {
    pub picture: Picture,
    pub rect: [f32; 4],
    pub whole: [f32; 4],
    pub matrix: [f32; 4],
    pub offset: [f32; 2],
    pub alpha: f32,
    pub cut: [f32; 4],
}

/// One snowman as drawn: his parts back to front, how near he is, how near between the picture's farthest and nearest,
/// and the light he stands in. In front of the whole picture, as on the strip along the bottom, `near` is above one.
#[derive(Clone, Debug)]
pub struct Drawing {
    pub parts: Vec<Part>,
    pub near: f32,
    pub nearness: f32,
    pub tint: [f32; 3],
    /// The bounds of all he draws, left, bottom, right, top, in the display's points.
    pub reach: [f32; 4],
}

struct Figure {
    bot: Bot,
}

/// What a snowball in the air was thrown at, to tell when it comes down.
enum Aim {
    /// By this player from in front of the screen, at a point as near as `near`, `size` across there.
    Player { near: f32, size: f32 },
}

pub struct Fight {
    ground: Ground,
    figures: HashMap<String, Figure>,
    aims: HashMap<u64, Aim>,
    bot_lives: u32,
    bot_back_at: Option<f64>,
    /// How hot this player's arm is from throwing, and when it last threw.
    heat: f32,
    heat_at: f64,
    whim: Whim,
    stepped_at: Option<f64>,
    /// How many snowballs this player has thrown, to tell how to throw until a few have been.
    pub throws: u32,
}

impl Fight {
    /// A fight on `ground`, the bot standing somewhere on it; none if it has nowhere to stand.
    pub fn new(ground: Ground, now: f64, seed: u64) -> Option<Fight> {
        let bot = Bot::new(ground.clone(), now, true, seed)?;
        let mut figures = HashMap::new();
        figures.insert(BOT.to_string(), Figure { bot });
        Some(Fight {
            ground,
            figures,
            aims: HashMap::new(),
            bot_lives: BOT_HITS,
            bot_back_at: None,
            heat: 0.0,
            heat_at: now,
            whim: Whim::new(seed ^ 0x9E37_79B9),
            stepped_at: None,
            throws: 0,
        })
    }

    pub fn ground(&self) -> &Ground {
        &self.ground
    }

    /// Moves the snowmen on, with the simulation covering the display from `offset`: their throws and the snow they
    /// throw up go into it, and the snowballs that came down in it hit them or burst. `covers` are what stands in front
    /// of the picture, left, bottom, right, top in the display's points: the bot throws only where he can be seen.
    /// Returns whether any of them moved.
    pub fn step(&mut self, now: f64, simulation: &mut SnowSimulation, offset: V2, covers: &[[f32; 4]], visible: bool) -> bool {
        let dt = self.stepped_at.map_or(1.0 / 60.0, |at| (now - at).min(0.05) as f32);
        self.stepped_at = Some(now);
        if let Some(back) = self.bot_back_at
            && now >= back
            && let Some(figure) = self.figures.get_mut(BOT)
        {
            self.bot_back_at = None;
            self.bot_lives = BOT_HITS;
            figure.bot.come_back();
        }
        let mut moved = false;
        for figure in self.figures.values_mut() {
            let reach = figure.bot.reach();
            let covered = covers.iter().any(|c| c[0] < reach[2] && c[2] > reach[0] && c[1] < reach[3] && c[3] > reach[1]);
            let near = figure.bot.near();
            let mut throws = Vec::new();
            moved |= figure.bot.step(dt, now, visible && !covered, &mut |hand, size, target| throws.push((hand, size, target)));
            for (hand, size, _) in throws {
                simulation.throw_at_screen(hand - offset, size, Some(near));
            }
            for (point, size) in figure.bot.take_bursts() {
                simulation.crumble_snowman(point - offset, figure.bot.foot.y - offset.y, near, size);
            }
        }
        for arrival in simulation.take_arrivals() {
            let Some(aim) = self.aims.remove(&arrival.token) else { continue };
            let at = arrival.at + offset;
            let hit = match aim {
                Aim::Player { near, size } => self.land(at, near, size, now, simulation, offset),
            };
            if !hit {
                simulation.splat(arrival.at, arrival.near, arrival.size);
            }
        }
        moved
    }

    /// A snowball thrown by this player came down at `at`: it hits a snowman still there and about as near.
    fn land(&mut self, at: V2, near: f32, size: f32, now: f64, simulation: &mut SnowSimulation, offset: V2) -> bool {
        let Some((id, figure)) = self.figures.iter_mut().find(|(_, figure)| figure.bot.contains(at) && (figure.bot.near() - near).abs() < 0.08)
        else {
            return false;
        };
        figure.bot.hit(at, now);
        simulation.puff_at(at - offset, figure.bot.near(), size * 1.4);
        if id == BOT {
            self.bot_lives = self.bot_lives.saturating_sub(1);
            if self.bot_lives == 0 {
                figure.bot.knock_out();
                self.bot_back_at = Some(now + BOT_BACK);
            } else if self.bot_lives.is_multiple_of(HITS_PER_HEART) {
                figure.bot.move_away();
            }
        }
        true
    }

    /// The snowman at a point, and how near he is.
    pub fn figure_at(&self, point: V2) -> Option<f32> {
        self.figures.values().find(|figure| figure.bot.contains(point)).map(|figure| figure.bot.near())
    }

    /// How near the picture is at a point where a snowball might land: a snowman's nearness over him, else the ground's
    /// or the picture's own, `near` there.
    pub fn aim_near(&self, point: V2, near: f32) -> f32 {
        self.figure_at(point).unwrap_or(near)
    }

    /// Throws a snowball from in front of the screen at `point` in the picture, where it is `near`, on a display
    /// `screen_width` points across; it hits a snowman still there when it comes down. The first throw after a rest goes
    /// where it is aimed; the faster they follow, the farther they stray.
    pub fn throw_ball(&mut self, point: V2, near: f32, now: f64, simulation: &mut SnowSimulation, offset: V2) {
        let size = self.ground.height(near.max(self.ground.far)) * 0.1;
        let width = self.ground.screen.x;
        let start = V2::new(width * 0.5 + (point.x - width * 0.5) * 0.35, -30.0);
        self.throws += 1;
        self.heat *= (-((now - self.heat_at) as f32) / 1.5).exp();
        self.heat_at = now;
        let stray = self.heat.min(3.0);
        self.heat += 0.5;
        let u = self.whim.fraction().max(f32::EPSILON);
        let v = self.whim.fraction();
        let spread = (-2.0 * u.ln()).sqrt() * stray;
        let angle = 2.0 * std::f32::consts::PI * v;
        let landing = point + V2::new(angle.cos() * 0.3, angle.sin() * 0.25) * (spread * size * 10.0);
        let token = simulation.throw_into_picture(start - offset, landing - offset, near, size, 46.0, None);
        self.aims.insert(token, Aim::Player { near, size });
    }

    /// The snowmen as they are to be drawn.
    pub fn drawings(&self) -> Vec<Drawing> {
        self.figures.values().map(|figure| drawing(&figure.bot)).collect()
    }

    /// Hits the bot can still take before he is out.
    pub fn bot_lives(&self) -> u32 {
        self.bot_lives
    }

    /// Whether the bot can be seen to wander: there is something to keep drawing.
    pub fn awake(&self) -> bool {
        !self.figures.is_empty()
    }
}

fn part(picture: Picture, placement: super::Placement, rect: [f32; 4], whole: [f32; 4], cut: [f32; 4]) -> Part {
    let m = placement.matrix;
    Part { picture, rect, whole, matrix: [m.x.x, m.x.y, m.y.x, m.y.y], offset: [placement.offset.x, placement.offset.y], alpha: placement.alpha, cut }
}

/// A snowman's parts back to front: the throwing arm behind him while he winds up, in front as it comes over; broken
/// apart, each ball cut out of the body's picture, the arm, the hat flying off.
fn drawing(bot: &Bot) -> Drawing {
    let (body, arm, hat) = bot.placements();
    let mut parts = Vec::new();
    if bot.in_pieces() {
        for piece in bot.debris() {
            let (picture, whole) = if piece.arm { (Picture::Arm, ARM_RECT) } else { (Picture::Body, BODY_RECT) };
            parts.push(part(picture, piece.placement, piece.rect, whole, piece.cut));
        }
    } else {
        let behind = bot.arm_behind();
        if behind {
            parts.push(part(Picture::Arm, arm, ARM_RECT, ARM_RECT, [0.0; 4]));
        }
        parts.push(part(Picture::Body, body, BODY_RECT, BODY_RECT, [0.0; 4]));
        if !behind {
            parts.push(part(Picture::Arm, arm, ARM_RECT, ARM_RECT, [0.0; 4]));
        }
    }
    parts.push(part(Picture::Hat, hat, HAT_RECT, HAT_RECT, [0.0; 4]));
    parts.retain(|part| part.alpha > 0.001);
    let in_front = bot.ground.in_front;
    let near = bot.near();
    Drawing {
        parts,
        near: if in_front { 10.0 } else { near },
        nearness: if in_front { 1.0 } else { bot.ground.nearness(near) },
        tint: bot.tint(),
        reach: bot.reach(),
    }
}
