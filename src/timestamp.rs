//! RFC 3339 / ISO 8601 UTC timestamps, without a date crate.
//!
//! Claude Code writes `lastUpdated` with JavaScript's `Date.prototype.toISOString()`
//! and validates it as a **string** on read. We only ever need to produce that one
//! shape, so we do the civil-date arithmetic here rather than pulling in `chrono`
//! (or shelling out to `date`, which is neither portable nor testable).

use std::time::{SystemTime, UNIX_EPOCH};

/// Current UTC time as `YYYY-MM-DDTHH:MM:SS.sssZ`.
///
/// Matches `new Date().toISOString()` for every year in `1000..=9999`, which is the
/// only range a system clock will realistically produce. Outside it we emit more or
/// fewer digits rather than JavaScript's expanded-year form (`+275760-…`); nothing
/// reads these timestamps back as dates, so the shape is not worth the code.
pub fn utc_now_iso8601() -> String {
    let now = SystemTime::now();
    // A clock set before 1970 makes `duration_since` fail. Recover the negative
    // offset rather than silently reporting the epoch — a confidently wrong
    // timestamp is worse than an odd-looking one.
    let ms = match now.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    };
    iso8601_from_epoch_millis(ms)
}

/// Format Unix epoch milliseconds as an ISO-8601 UTC timestamp.
///
/// Split out from [`utc_now_iso8601`] so the calendar arithmetic is testable
/// against known instants.
pub fn iso8601_from_epoch_millis(epoch_ms: i64) -> String {
    // Floor-divide so pre-1970 instants (negative input) still land on the right day.
    let secs = epoch_ms.div_euclid(1000);
    let millis = epoch_ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    );

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hour, minute, second, millis
    )
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch → (year, month, day)
/// in the proleptic Gregorian calendar. Exact for the whole i64 range we care about.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap days land at the end of the cycle.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_epoch() {
        assert_eq!(iso8601_from_epoch_millis(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn formats_known_instants() {
        // Cross-checked against `new Date(ms).toISOString()`.
        assert_eq!(
            iso8601_from_epoch_millis(1_000_000_000_000),
            "2001-09-09T01:46:40.000Z"
        );
        assert_eq!(
            iso8601_from_epoch_millis(1_755_648_000_123),
            "2025-08-20T00:00:00.123Z"
        );
    }

    #[test]
    fn handles_leap_day() {
        // 2024-02-29 is a leap day in a leap century-rule year.
        assert_eq!(
            iso8601_from_epoch_millis(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
        // 2000 was a leap year (divisible by 400); 1900 was not.
        assert_eq!(
            iso8601_from_epoch_millis(951_782_400_000),
            "2000-02-29T00:00:00.000Z"
        );
        // 1900 is divisible by 100 but not by 400, so February had 28 days.
        // The day after 1900-02-28 is 1900-03-01, not 1900-02-29.
        assert_eq!(
            iso8601_from_epoch_millis(-2_203_977_600_000),
            "1900-02-28T00:00:00.000Z"
        );
        assert_eq!(
            iso8601_from_epoch_millis(-2_203_891_200_000),
            "1900-03-01T00:00:00.000Z"
        );
    }

    #[test]
    fn handles_pre_epoch_instants() {
        assert_eq!(iso8601_from_epoch_millis(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn years_outside_the_four_digit_range_still_produce_output() {
        // Documented limitation rather than a panic: we widen instead of switching
        // to JavaScript's expanded-year form.
        let far = iso8601_from_epoch_millis(253_402_300_800_000); // year 10000
        assert!(far.starts_with("10000-"), "{}", far);
        // And the extremes do not overflow or panic.
        let _ = iso8601_from_epoch_millis(i64::MAX);
        let _ = iso8601_from_epoch_millis(i64::MIN);
    }

    #[test]
    fn now_is_a_plausible_iso8601_string() {
        let s = utc_now_iso8601();
        assert_eq!(
            s.len(),
            24,
            "toISOString() shape is exactly 24 chars: {}",
            s
        );
        assert!(s.ends_with('Z'));
        assert!(s.as_bytes()[10] == b'T');
        // Sanity: we are somewhere after 2020 and before 2100.
        let year: i32 = s[..4].parse().unwrap();
        assert!((2020..2100).contains(&year), "implausible year in {}", s);
    }
}
