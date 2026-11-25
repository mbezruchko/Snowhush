//! The room of a snowball fight between friends, as the Mac's SnowballRoom keeps it: who else is there, what is left of
//! each snowman this round, the rounds each has won. The network itself is the net crate; the app hands what it hears
//! from there to `take`, and passes this player's hits on to it.

use std::collections::HashMap;

/// A snowman's hearts, and the hits each of them takes: fifteen hits before he falls apart.
pub const HEARTS: u32 = 5;
pub const HITS_PER_HEART: u32 = 3;
pub const STARTING_LIVES: i32 = (HEARTS * HITS_PER_HEART) as i32;
/// Seconds a won round shows before the next starts.
const BETWEEN_ROUNDS: f64 = 8.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub id: String,
    pub name: String,
}

/// Why there is no room after one was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Playing online is off, for everyone, as snowhush.app/relay.json says.
    Unavailable,
    Full,
    Closed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    None,
    Opening,
    Joining,
    /// In a room, as `me`.
    Open {
        me: String,
    },
    Failed(Failure),
}

/// What the network says happened, as the net crate's events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heard {
    /// Hosting: the room is open, and this is its invite.
    Room {
        ticket: String,
        me: String,
    },
    Joined {
        me: String,
    },
    Player {
        id: String,
        name: String,
    },
    Left {
        id: String,
    },
    /// `from` threw a snowball at `at` and hit.
    Throw {
        from: String,
        at: String,
    },
    /// The room is gone; `full` when it had no room for this player.
    Closed {
        full: bool,
    },
    Error,
}

#[derive(Clone, Debug)]
pub struct Room {
    pub state: State,
    /// Everyone else in the room.
    pub players: Vec<Player>,
    /// The invite to the room, when hosting it.
    pub invite: Option<String>,
    lives: HashMap<String, i32>,
    wins: HashMap<String, u32>,
    /// The one left standing, until the next round starts, and when that is.
    pub winner: Option<String>,
    next_round: Option<f64>,
    /// Counts up whenever anything about the room changes, for the menu and the board.
    pub version: u64,
}

impl Default for Room {
    fn default() -> Room {
        Room {
            state: State::None,
            players: Vec::new(),
            invite: None,
            lives: HashMap::new(),
            wins: HashMap::new(),
            winner: None,
            next_round: None,
            version: 0,
        }
    }
}

impl Room {
    pub fn me(&self) -> Option<&str> {
        match &self.state {
            State::Open { me } => Some(me),
            _ => None,
        }
    }

    pub fn lives(&self, player: &str) -> i32 {
        self.lives.get(player).copied().unwrap_or(STARTING_LIVES)
    }

    /// The hearts a player has left, a heart that has taken a hit or two still counting.
    pub fn hearts(&self, player: &str) -> u32 {
        (self.lives(player).max(0) as u32).div_ceil(HITS_PER_HEART)
    }

    pub fn wins(&self, player: &str) -> u32 {
        self.wins.get(player).copied().unwrap_or(0)
    }

    pub fn is_out(&self, player: &str) -> bool {
        self.lives(player) <= 0
    }

    /// A player's name; this player's is none, for the app to call "You".
    pub fn name(&self, player: &str) -> Option<&str> {
        self.players.iter().find(|p| p.id == player).map(|p| p.name.as_str())
    }

    /// Asked to open a room, or to join one: waiting for the network.
    pub fn opening(&mut self, joining: bool) {
        self.reset();
        self.state = if joining { State::Joining } else { State::Opening };
        self.version += 1;
    }

    /// Couldn't open or join one.
    pub fn fail(&mut self, failure: Failure) {
        self.reset();
        self.state = State::Failed(failure);
        self.version += 1;
    }

    pub fn leave(&mut self) {
        self.reset();
        self.version += 1;
    }

    /// Whether a hit from one player at another counts: in a round still going, both still standing.
    pub fn counts(&self, from: &str, at: &str) -> bool {
        self.winner.is_none() && !self.is_out(from) && !self.is_out(at)
    }

    /// This player hit `player`: it counts only while both are in the round. Returns whether it did, for the app to
    /// tell the room, which doesn't tell a player of its own throws.
    pub fn hit(&mut self, player: &str, now: f64) -> bool {
        let Some(me) = self.me().map(str::to_string) else { return false };
        if !self.counts(&me, player) {
            return false;
        }
        self.land(player, now);
        true
    }

    fn land(&mut self, player: &str, now: f64) {
        *self.lives.entry(player.to_string()).or_insert(STARTING_LIVES) -= 1;
        self.settle(now);
        self.version += 1;
    }

    /// Everyone's snowman whole again.
    fn start_round(&mut self) {
        self.next_round = None;
        self.lives.clear();
        self.winner = None;
    }

    /// With one left standing of two or more, he wins the round, and another starts a few seconds later.
    fn settle(&mut self, now: f64) {
        let Some(me) = self.me().map(str::to_string) else { return };
        if self.winner.is_some() {
            return;
        }
        let everyone: Vec<String> = std::iter::once(me).chain(self.players.iter().map(|p| p.id.clone())).collect();
        if everyone.len() < 2 {
            return;
        }
        let standing: Vec<&String> = everyone.iter().filter(|id| !self.is_out(id)).collect();
        if standing.is_empty() {
            self.start_round();
            return;
        }
        if standing.len() == 1 {
            let winner = standing[0].clone();
            *self.wins.entry(winner.clone()).or_insert(0) += 1;
            self.winner = Some(winner);
            self.next_round = Some(now + BETWEEN_ROUNDS);
        }
    }

    /// Starts the next round when its time comes.
    pub fn tick(&mut self, now: f64) {
        if self.next_round.is_some_and(|at| now >= at) {
            self.start_round();
            self.version += 1;
        }
    }

    fn reset(&mut self) {
        self.state = State::None;
        self.players.clear();
        self.invite = None;
        self.start_round();
        self.wins.clear();
    }

    /// Takes in what the network said happened. Returns a throw someone else made that counted, `from` and `at`, for the
    /// snowmen to show.
    pub fn take(&mut self, heard: Heard, now: f64) -> Option<(String, String)> {
        self.version += 1;
        match heard {
            Heard::Room { ticket, me } => {
                self.state = State::Open { me };
                self.invite = Some(ticket);
            }
            Heard::Joined { me } => self.state = State::Open { me },
            Heard::Player { id, name } => {
                if !self.players.iter().any(|player| player.id == id) {
                    self.players.push(Player { id, name });
                }
                // Someone new, or back after a sleep, comes into a fresh round, everyone whole.
                self.start_round();
            }
            Heard::Left { id } => {
                self.players.retain(|player| player.id != id);
                self.lives.remove(&id);
                self.wins.remove(&id);
                self.settle(now);
            }
            Heard::Throw { from, at } => {
                if Some(from.as_str()) == self.me() || !self.counts(&from, &at) {
                    return None;
                }
                self.land(&at, now);
                return Some((from, at));
            }
            Heard::Closed { full } => self.fail(if full { Failure::Full } else { Failure::Closed }),
            Heard::Error => self.fail(Failure::Failed),
        }
        None
    }
}

/// The link that brings a friend into the room.
pub fn link(invite: &str) -> String {
    format!("snowhush://join/{invite}")
}

/// Whether some text holds an invite.
pub fn holds_invite(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("snowhush://join/") && text.len() > 60
}
