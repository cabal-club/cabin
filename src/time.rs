//! Time-related helper functions.

use std::time::{SystemTime, UNIX_EPOCH};

use cable::Error;
use chrono::{Local, LocalResult, TimeZone};

/// Return the current system time in seconds since the Unix epoch.
pub fn now() -> Result<u64, Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?;

    Ok(now)
}

/// Return the time defining two weeks before the current system time.
///
/// Used to calculate the start time for channel time range
/// requests.
pub fn two_weeks_ago() -> Result<u64, Error> {
    let two_weeks_ago = now()? - 1_209_600_000;

    Ok(two_weeks_ago)
}

/// Format the given timestamp (represented in milliseconds since the Unix
/// epoch) as hour and minutes relative to the local timezone.
pub fn format(timestamp: u64) -> String {
    if let LocalResult::Single(date_time) = Local.timestamp_millis_opt(timestamp as i64) {
        format!("{}", date_time.format("%H:%M"))
    } else {
        // Something is wrong with the timestamp; display a place-holder to
        // avoid panicking on an unwrap.
        String::from("XX:XX")
    }
}
