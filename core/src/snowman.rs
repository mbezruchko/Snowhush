//! Snowmen living in the wallpaper's picture, for a snowball fight: where they can walk, and the bot who walks there, as
//! the Mac's Snowman.swift has them. Sizes and places are in parts of a snowman's height, from his feet.

pub mod bot;
pub mod fight;
pub mod ground;

pub use bot::{Bot, Debris, Mat2, Placement};
pub use fight::{Drawing, Fight, Part, Picture};
pub use ground::Ground;

use crate::geometry::V2;

/// What each of his pictures covers: left, bottom, right, top.
pub const BODY_RECT: [f32; 4] = [-0.5, -0.06, 0.5, 0.84];
pub const ARM_RECT: [f32; 4] = [-0.03, -0.06, 0.34, 0.2];
pub const HAT_RECT: [f32; 4] = [-0.21, -0.05, 0.17, 0.27];
/// The right shoulder, where the throwing arm turns.
pub const SHOULDER: V2 = V2::new(0.125, 0.47);
/// The hand at the end of the throwing arm, from the shoulder, the arm held straight out.
pub const REACH: V2 = V2::new(0.29, 0.1);
/// Where the hat sits on the head.
pub const CROWN: V2 = V2::new(-0.005, 0.745);
/// The three balls: middle and radius.
pub const BALLS: [(V2, f32); 3] = [(V2::new(0.0, 0.19), 0.21), (V2::new(0.0, 0.448), 0.155), (V2::new(0.0, 0.66), 0.115)];
