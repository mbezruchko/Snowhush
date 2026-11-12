//! Which kinds fall: the one picked last, or two mixed, and the numbers the settings keep them under.

use crate::simulation::Mode;

/// The kinds by the number the settings keep them under. Saved settings depend on this order: only add to the end.
pub const SAVED: [Mode; 4] = [Mode::Snow, Mode::Autumn, Mode::Rain, Mode::Blossom];

/// The number a kind is saved under.
pub fn number(mode: Mode) -> u32 {
    SAVED.iter().position(|&m| m == mode).unwrap_or(0) as u32
}

/// The kind saved under a number, if any.
pub fn kind(number: u32) -> Option<Mode> {
    SAVED.get(number as usize).copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picked {
    /// What falls alone: the kind picked last.
    pub mode: Mode,
    /// Two kinds fall together: `mix`, the one picked last after the one picked before it.
    pub mixed: bool,
    pub mix: [Mode; 2],
}

impl Picked {
    /// From the saved `mode`, `autumn`, `mixed`, `mix0` and `mix1`. Before rain there was only `autumn`, a switch
    /// for leaves.
    pub fn load(mode: Option<u32>, autumn: Option<u32>, mixed: Option<u32>, mix: [Option<u32>; 2]) -> Picked {
        let mode = mode.and_then(kind).unwrap_or(if autumn.is_some_and(|v| v != 0) { Mode::Autumn } else { Mode::Snow });
        Picked { mode, mixed: mixed.is_some_and(|v| v != 0), mix: mix_of(mix[0], mix[1], mode) }
    }

    /// The kinds falling: the two mixed, or the one picked.
    pub fn modes(&self) -> Vec<Mode> {
        if self.mixed { self.mix.to_vec() } else { vec![self.mode] }
    }

    /// Picks a kind; mixed, a third kind picked takes the place of the one picked before the other. Whether anything
    /// changed.
    pub fn pick(&mut self, mode: Mode) -> bool {
        if self.mixed {
            if self.mix.contains(&mode) {
                return false;
            }
            self.mix = [self.mix[1], mode];
        } else if mode == self.mode {
            return false;
        }
        self.mode = mode;
        true
    }

    /// Mixed, the kind picked last falls along with the one picked before it; unmixed, it falls alone.
    pub fn toggle_mixed(&mut self) {
        self.mixed = !self.mixed;
        if self.mixed {
            let other = if self.mix.contains(&self.mode) { self.mix.iter().copied().find(|&m| m != self.mode) } else { Some(self.mix[1]) };
            self.mix = [other.unwrap_or(other_than(self.mode)), self.mode];
        }
    }
}

/// Two different kinds read from the settings, or `mode` and another when they don't make a pair.
fn mix_of(first: Option<u32>, second: Option<u32>, mode: Mode) -> [Mode; 2] {
    match (first.and_then(kind), second.and_then(kind)) {
        (Some(a), Some(b)) if a != b => [a, b],
        _ => [other_than(mode), mode],
    }
}

/// The kind to mix with `mode` until another is picked.
fn other_than(mode: Mode) -> Mode {
    if mode == Mode::Snow { Mode::Autumn } else { Mode::Snow }
}
