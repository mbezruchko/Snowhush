//! The New Year: a decorated fir stands in the snow with presents under it from December 20 until dawn on January 8,
//! and any other day it is turned on from the menu. Fireworks go up through its season, and at midnight the garland
//! counts down.

use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::Result;

use snowhush_core::halloween::{Choice, Today};
use snowhush_core::new_year::{self, Holiday, LocalTime};
use snowhush_core::simulation::Mode;

use crate::halloween::{self, Picture};
use crate::prefs::Settings;

/// The fir's three bough pictures, drawn on the Mac, its root at the left and its upper side at the top.
const NEEDLES: [&[u8]; 3] =
    [include_bytes!("../assets/tree/needles-0.png"), include_bytes!("../assets/tree/needles-1.png"), include_bytes!("../assets/tree/needles-2.png")];

/// The local time to the millisecond, for the countdown.
pub fn now() -> LocalTime {
    let now = unsafe { GetLocalTime() };
    LocalTime {
        year: now.wYear as u32,
        month: now.wMonth as u32,
        day: now.wDay as u32,
        hour: now.wHour as u32,
        minute: now.wMinute as u32,
        second: now.wSecond as f64 + now.wMilliseconds as f64 / 1000.0,
    }
}

/// The year the season began in, and whether it is the season.
pub fn today() -> Today {
    new_year::today(now())
}

/// Whether the New Year is wanted today.
pub fn wanted(choice: Choice) -> bool {
    choice.wanted(today())
}

/// The holiday the snow keeps, if any: one in its season comes first, then one turned on for every day; only while
/// snow alone falls.
pub fn holiday(settings: &Settings) -> Option<Holiday> {
    new_year::holiday(settings.picked.modes() == [Mode::Snow], settings.new_year, today(), settings.halloween, halloween::today())
}

/// Whether the fir stands in the snow.
pub fn shows(settings: &Settings) -> bool {
    holiday(settings) == Some(Holiday::NewYear)
}

/// The fir's bough pictures, the same size, in rows of premultiplied RGBA.
pub fn needles() -> Result<Vec<Picture>> {
    NEEDLES.iter().map(|png| halloween::decode(png)).collect()
}
