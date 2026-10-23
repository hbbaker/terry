//! Wall-clock time without a date crate: Unix seconds, and a UTC stamp for file names.

use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// Seconds since the Unix epoch, as the Python gate wrote them (`time.time()`).
pub fn unix_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0)
}

/// The current UTC time as `YYYYMMDD-HHMMSS`, for patch and backup file names.
pub fn utc_stamp() -> String {
    stamp_of(unix_seconds() as u64)
}

pub fn stamp_of(seconds: u64) -> String {
    let days = (seconds / SECONDS_PER_DAY) as i64;
    let in_day = seconds % SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        in_day / 3600,
        in_day % 3600 / 60,
        in_day % 60
    )
}

/// The (year, month, day) of a day count from 1970-01-01, by Howard Hinnant's
/// `civil_from_days` algorithm for the proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
