//! The wind left to wander from the menu, as the Mac's `Wind.wandering` has it.

use crate::geometry::V2;

const PERIOD: f64 = 45.0;

/// The wind left to wander, x to the right and y up, at most 1 long: read off a clock every display shares, with one
/// `seed` for them all, it blows alike on each. Every 45 seconds a new one eases in over the last, mostly sideways:
/// now a lull, mostly a steady wind, now and then a gale. It keeps to one side for three in a row, and comes and goes
/// a little in gusts all along.
pub fn wandering(seconds: f64, seed: u64) -> V2 {
    let k = (seconds / PERIOD).floor() as i64;
    let into = seconds - k as f64 * PERIOD;
    let (from, to) = (target(k - 1, seed), target(k, seed));
    let t = (into / (12.0 + 8.0 * unit(k, 5, seed))).min(1.0);
    let ease = t * t * t * (t * (6.0 * t - 15.0) + 10.0);
    let gusts = 1.0 + 0.12 * (seconds * 0.83).sin() * (seconds * 0.29 + 1.3).sin();
    let x = (from.0 + (to.0 - from.0) * ease) * gusts;
    let y = (from.1 + (to.1 - from.1) * ease) * gusts;
    let length = x.hypot(y);
    let scale = if length > 1.0 { 1.0 / length } else { 1.0 };
    V2::new((x * scale) as f32, (y * scale) as f32)
}

fn target(k: i64, seed: u64) -> (f64, f64) {
    let roll = unit(k, 0, seed);
    let spread = unit(k, 1, seed);
    let strength = if roll < 0.2 {
        0.03 + 0.12 * spread
    } else if roll < 0.75 {
        0.2 + 0.3 * spread
    } else {
        0.55 + 0.3 * spread
    };
    let side = if unit(k.div_euclid(3), 2, seed) < 0.5 { 1.0 } else { -1.0 };
    // Degrees below level, or above for less than zero; steep only now and then.
    let tilt = (if unit(k, 3, seed) < 0.8 { 30.0 } else { 70.0 }) * (2.0 * unit(k, 4, seed) - 1.0);
    let angle = (90.0_f64 + tilt).to_radians();
    (side * strength * angle.sin(), strength * angle.cos())
}

/// A number from 0 to 1 for `k` and `salt`, the same for every display.
fn unit(k: i64, salt: u64, seed: u64) -> f64 {
    let mut z = seed.wrapping_add((k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)).wrapping_add(salt.wrapping_mul(0xD1B5_4A32_D192_ED03));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}
