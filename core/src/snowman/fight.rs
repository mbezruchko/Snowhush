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
use super::room::Room;
use super::{ARM_RECT, BODY_RECT, HAT_RECT};
use crate::geometry::V2;
use crate::simulation::SnowSimulation;

/// Hits the bot takes before he is out: two hearts' worth, moving elsewhere after the first.
pub const BOT_HITS: u32 = 6;
pub use super::room::HITS_PER_HEART;
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
    /// A friend's name over his snowman, and what is left of him; none for the bot, or for one out of the round.
    pub label: Option<(String, i32)>,
    /// Where his label goes, just over his hat, in the display's points.
    pub head: [f32; 2],
}

struct Figure {
    bot: Bot,
    /// A friend's name; none for the bot.
    name: Option<String>,
    /// Hearts he had as last seen; and since when he owes a move elsewhere for one lost, until the snowball that cost it
    /// is seen to land.
    hearts: u32,
    move_owed: Option<f64>,
}

impl Figure {
    /// He took the snowball that cost him a heart, seen to land: he moves elsewhere.
    fn move_if_owed(&mut self) {
        if self.move_owed.take().is_some() {
            self.bot.move_away();
        }
    }
}

/// What a snowball in the air was thrown at, to tell when it comes down.
enum Aim {
    /// By this player from in front of the screen, at a point as near as `near`, `size` across there.
    Player { near: f32, size: f32 },
    /// By one friend's snowman at another's, across the picture.
    Friend { target: String },
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
    /// The friends this player hit, for the room to be told.
    hits: Vec<String>,
}

impl Fight {
    /// A fight on `ground`, the bot standing somewhere on it; none if it has nowhere to stand.
    pub fn new(ground: Ground, now: f64, seed: u64) -> Option<Fight> {
        let bot = Bot::new(ground.clone(), now, true, seed)?;
        let mut figures = HashMap::new();
        figures.insert(BOT.to_string(), Figure { bot, name: None, hearts: super::room::HEARTS, move_owed: None });
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
            hits: Vec::new(),
        })
    }

    pub fn ground(&self) -> &Ground {
        &self.ground
    }

    /// The snowmen as the room has them: the friends in it, each as he stands in the round, or the bot while there is no
    /// room. Only the main display shows the friends, so that each throws once.
    pub fn follow(&mut self, room: &Room, main: bool, now: f64) {
        let mut wanted: HashMap<String, Option<String>> = HashMap::new();
        if room.me().is_some() {
            if main {
                for player in &room.players {
                    wanted.insert(player.id.clone(), Some(player.name.clone()));
                }
            }
        } else {
            wanted.insert(BOT.to_string(), None);
        }
        self.figures.retain(|id, _| wanted.contains_key(id));
        for (id, name) in wanted {
            if self.figures.contains_key(&id) {
                continue;
            }
            let seed = id.bytes().fold(now.to_bits(), |hash, byte| hash.rotate_left(5) ^ byte as u64);
            if let Some(bot) = Bot::new(self.ground.clone(), now, id == BOT, seed) {
                if id == BOT {
                    self.bot_lives = BOT_HITS;
                    self.bot_back_at = None;
                }
                self.figures.insert(id, Figure { bot, name, hearts: room.hearts(""), move_owed: None });
            }
        }
        for (id, figure) in self.figures.iter_mut().filter(|(id, _)| id.as_str() != BOT) {
            Self::note_hearts(figure, room, id, now);
            if room.is_out(id) {
                figure.bot.knock_out();
            } else {
                figure.bot.come_back();
            }
        }
    }

    /// A friend's snowman that has lost a heart is owed a move elsewhere; one out of the round, or whole again in a new
    /// one, none.
    fn note_hearts(figure: &mut Figure, room: &Room, id: &str, now: f64) {
        let hearts = room.hearts(id);
        if room.is_out(id) || hearts > figure.hearts {
            figure.move_owed = None;
        } else if hearts < figure.hearts && figure.move_owed.is_none() {
            figure.move_owed = Some(now);
        }
        figure.hearts = hearts;
    }

    /// Someone in the room hit someone, `me` being this player. Hit by a friend, the friend's snowman throws at the
    /// screen; one friend hitting another, the one throws at the other across the picture. With no snowman of his to
    /// throw, the snowball comes at the screen all the same.
    pub fn thrown(&mut self, from: &str, at: &str, me: &str, simulation: &mut SnowSimulation) {
        if let Some(figure) = self.figures.get_mut(from) {
            figure.bot.throw_now((at != me).then(|| at.to_string()));
        } else if at == me {
            let point = V2::new(simulation.width() * self.whim.range(0.3, 0.7), simulation.height() * 0.4);
            simulation.throw_at_screen(point, 6.0, None);
        }
    }

    /// Moves the snowmen on, with the simulation covering the display from `offset`: their throws and the snow they
    /// throw up go into it, and the snowballs that came down in it hit them or burst. `covers` are what stands in front
    /// of the picture, left, bottom, right, top in the display's points: the bot throws only where he can be seen.
    /// Returns whether any of them moved.
    pub fn step(&mut self, now: f64, simulation: &mut SnowSimulation, offset: V2, covers: &[[f32; 4]], visible: bool, room: &mut Room) -> bool {
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
        let mut across = Vec::new();
        for figure in self.figures.values_mut() {
            // The snowball that cost him a heart may never be seen to land, its thrower gone: he moves all the same.
            if figure.move_owed.is_some_and(|owed| now - owed > 3.0) {
                figure.move_if_owed();
            }
            let reach = figure.bot.reach();
            let covered = covers.iter().any(|c| c[0] < reach[2] && c[2] > reach[0] && c[1] < reach[3] && c[3] > reach[1]);
            let near = figure.bot.near();
            let mut throws = Vec::new();
            moved |= figure.bot.step(dt, now, visible && !covered, &mut |hand, size, target| throws.push((hand, size, target)));
            for (hand, size, target) in throws {
                match target {
                    Some(target) => across.push((hand, size, near, target)),
                    None => simulation.throw_at_screen(hand - offset, size, Some(near)),
                }
            }
            for (point, size) in figure.bot.take_bursts() {
                simulation.crumble_snowman(point - offset, figure.bot.foot.y - offset.y, near, size);
            }
        }
        // At a friend's snowman, across the picture.
        for (hand, size, near, target) in across {
            let Some(other) = self.figures.get(&target) else { continue };
            let bot = &other.bot;
            let token = simulation.throw_into_picture(hand - offset, bot.middle() - offset, bot.near(), bot.height() * 0.1, size, Some(near));
            self.aims.insert(token, Aim::Friend { target });
        }
        for arrival in simulation.take_arrivals() {
            let Some(aim) = self.aims.remove(&arrival.token) else { continue };
            let at = arrival.at + offset;
            let hit = match aim {
                Aim::Player { near, size } => self.land(at, near, size, now, simulation, offset, room),
                Aim::Friend { target } => match self.figures.get_mut(&target) {
                    Some(struck) => {
                        struck.bot.hit(at, now);
                        simulation.puff_at(arrival.at, struck.bot.near(), struck.bot.height() * 0.14);
                        struck.move_if_owed();
                        true
                    }
                    None => false,
                },
            };
            if !hit {
                simulation.splat(arrival.at, arrival.near, arrival.size);
            }
        }
        moved
    }

    /// The friends this player hit since this was last asked, for the room to be told.
    pub fn take_hits(&mut self) -> Vec<String> {
        std::mem::take(&mut self.hits)
    }

    /// A snowball thrown by this player came down at `at`: it hits a snowman still there and about as near. A hit on a
    /// friend counts only in a round still going, both standing; one that doesn't bursts on by.
    #[allow(clippy::too_many_arguments)]
    fn land(&mut self, at: V2, near: f32, size: f32, now: f64, simulation: &mut SnowSimulation, offset: V2, room: &mut Room) -> bool {
        let Some((id, figure)) = self.figures.iter_mut().find(|(_, figure)| figure.bot.contains(at) && (figure.bot.near() - near).abs() < 0.08)
        else {
            return false;
        };
        if id != BOT {
            if !room.hit(id, now) {
                return false;
            }
            self.hits.push(id.clone());
            figure.bot.hit(at, now);
            simulation.puff_at(at - offset, figure.bot.near(), size * 1.4);
            Self::note_hearts(figure, room, id, now);
            figure.move_if_owed();
            return true;
        }
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

    /// The snowmen as they are to be drawn, a friend's with his name and what is left of him in `room`.
    pub fn drawings(&self, room: &Room) -> Vec<Drawing> {
        self.figures
            .iter()
            .map(|(id, figure)| {
                let mut drawn = drawing(&figure.bot);
                drawn.label = figure.name.as_ref().filter(|_| !room.is_out(id)).map(|name| (name.clone(), room.lives(id)));
                drawn
            })
            .collect()
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
        label: None,
        head: [bot.foot.x, bot.foot.y + bot.height() * 1.02 + 9.0],
    }
}
