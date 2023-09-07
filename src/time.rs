//! Time-related helper functions.

use std::time::{SystemTime, UNIX_EPOCH};

use cable::Error;
use time_format::TimeStamp;

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

/// Format seconds since the Unix epoch as a timestamp with hour and minutes.
pub fn timestamp(time: u64) -> String {
    time_format::strftime_utc("%H:%M", time as TimeStamp).unwrap()
}
