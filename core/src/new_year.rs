//! The New Year: a decorated fir stands in the snow with presents under it from December 20 until dawn on January 8,
//! and any other day it is turned on from the menu. Fireworks go up through the season, more and more toward midnight,
//! when the garland counts down the last ten seconds. The Mac app has the same in Swift.

use crate::halloween::Today;

/// Lets the New Year be tried out on any day: SNOWHUSH_NEW_YEAR=1; and its midnight, five seconds after the app starts
/// and every two minutes after: SNOWHUSH_MIDNIGHT=1.
pub fn previewed() -> bool {
    std::env::var("SNOWHUSH_NEW_YEAR").is_ok_and(|value| value == "1")
}

pub fn midnight_previewed() -> bool {
    std::env::var("SNOWHUSH_MIDNIGHT").is_ok_and(|value| value == "1")
}

/// The local time, months and days counted from 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LocalTime {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: f64,
}

/// Whether it is the season, wanted or not.
pub fn is_season(month: u32, day: u32, hour: u32) -> bool {
    month == 12 && day >= 20 || month == 1 && (day < 8 || day == 8 && hour < 6) || previewed()
}

/// The year the season began in, its days in January belonging to the December before, and whether it is the season.
pub fn today(time: LocalTime) -> Today {
    Today { year: time.year - u32::from(time.month == 1), season: is_season(time.month, time.day, time.hour) }
}

/// Seconds past the New Year's midnight by the clock, before it negative, in December and January only.
fn since_midnight(time: LocalTime) -> Option<f64> {
    let day = match time.month {
        12 => time.day as f64 - 32.0,
        1 => time.day as f64 - 1.0,
        _ => return None,
    };
    Some(day * 86_400.0 + time.hour as f64 * 3600.0 + time.minute as f64 * 60.0 + time.second)
}

/// Seconds past the New Year's midnight, from ten seconds before it, for the countdown, until the fireworks are over;
/// none at any other time. Previewed, it comes `uptime` seconds after the app started counts 5, and every two minutes.
pub fn celebration(time: LocalTime, uptime: f64) -> Option<f64> {
    let t = if midnight_previewed() { uptime % 120.0 - 15.0 } else { since_midnight(time)? };
    (-10.0..75.0).contains(&t).then_some(t)
}

/// How many rockets go up a minute through the season: three, and more and more the nearer the New Year is, twenty at
/// midnight, falling back to three over the hour after. By the calendar even while midnight is previewed.
pub fn fireworks_per_minute(time: LocalTime) -> f32 {
    if !is_season(time.month, time.day, time.hour) {
        return 0.0;
    }
    let Some(t) = since_midnight(time) else { return 3.0 };
    let rise = if t < 0.0 { (t / (3.0 * 24.0 * 3600.0)).exp() } else { (-t / 1800.0).exp() };
    (3.0 + 17.0 * rise) as f32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holiday {
    Halloween,
    NewYear,
}

/// The holiday the snow keeps, if any: one in its season comes first, then one turned on for every day; only while
/// snow alone falls.
pub fn holiday(
    snow_only: bool,
    new_year: crate::halloween::Choice,
    new_year_today: Today,
    halloween: crate::halloween::Choice,
    halloween_today: Today,
) -> Option<Holiday> {
    if !snow_only {
        return None;
    }
    if new_year_today.season && new_year.wanted(new_year_today) {
        return Some(Holiday::NewYear);
    }
    if halloween_today.season && halloween.wanted(halloween_today) {
        return Some(Holiday::Halloween);
    }
    if new_year.on {
        return Some(Holiday::NewYear);
    }
    if halloween.on {
        return Some(Holiday::Halloween);
    }
    None
}
