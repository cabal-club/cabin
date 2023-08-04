use std::time::{SystemTime, UNIX_EPOCH};

use time_format::TimeStamp;

/// Return the current system time in seconds since the Unix epoch.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Format seconds since the Unix epoch as a timestamp with hour and minutes.
pub fn timestamp(time: u64) -> String {
    time_format::strftime_utc("%H:%M", time as TimeStamp).unwrap()
}
