//! Halloween: from October 30, when the update bringing it comes, until dawn on November 1 the snow takes on a
//! spooky mood.
//! The Mac app has the same in Swift, with more to it.

use crate::geometry::smoothstep;

/// A jack-o'-lantern's height as a share of its width, and where its candle burns: a share of its width across and
/// of its height up from its bottom. Its shell and its carved face are drawn once, on the Mac, into
/// `windows/assets/lanterns`, at two pixels per unit of a drawing 100 units wide, apart so the candle can flicker
/// in the face alone.
pub const PUMPKIN_ASPECT: f32 = 0.82;
pub const PUMPKIN_CANDLE: [f32; 2] = [0.5, 0.45];

/// What the ghosts' signs say. The signs themselves are drawn once, on the Mac, into `windows/assets/signs`,
/// one picture per caption in this order, at two pixels per point with the stick's foot at the middle of the bottom edge.
pub const SIGN_CAPTIONS: [&str; 12] = [
    "BOO!",
    "SCARE ME!",
    "I DARE YOU",
    "DON'T TOUCH",
    "SNOW WAY!",
    "CHILL OUT",
    "FREEZE!",
    "BRRR-OO!",
    "NO PEEKING",
    "NOT A SNOWMAN",
    "BACK TO WORK",
    "TRICK OR TREAT",
];

/// Lets Halloween be tried out on any day: SNOWHUSH_HALLOWEEN=1, and the daylight under it at any hour: SNOWHUSH_HOUR=22.
pub fn previewed() -> bool {
    std::env::var("SNOWHUSH_HALLOWEEN").is_ok_and(|value| value == "1")
}

pub fn previewed_hour() -> Option<f32> {
    std::env::var("SNOWHUSH_HOUR").ok()?.parse().ok()
}

/// Whether it is Halloween at this local date and hour, months counted from 1, or it is being previewed.
pub fn is_halloween(month: u32, day: u32, hour: u32) -> bool {
    month == 10 && day >= 30 || month == 11 && day == 1 && hour < 6 || previewed()
}

/// How much of full daylight there is at this local hour: none at night, all of it from mid-morning to late afternoon.
pub fn daylight_share(hour: f32) -> f32 {
    smoothstep(6.0, 8.5, hour) * (1.0 - smoothstep(17.5, 20.5, hour))
}

/// How far into Halloween night it is: all the way, by day too, from the moment Halloween comes, and none on any
/// other day.
pub fn night(halloween: bool) -> f32 {
    if halloween { 1.0 } else { 0.0 }
}

/// The light and shadow tint `night` of the way into Halloween night: moonlit blue snow with violet shadows.
pub fn moonlit(light: [f32; 3], shadow_tint: [f32; 4], night: f32) -> ([f32; 3], [f32; 4]) {
    let moon = [0.76, 0.84, 1.0];
    let violet = [0.42, 0.3, 0.68, 0.6];
    (std::array::from_fn(|i| light[i] + (moon[i] - light[i]) * night), std::array::from_fn(|i| shadow_tint[i] + (violet[i] - shadow_tint[i]) * night))
}
