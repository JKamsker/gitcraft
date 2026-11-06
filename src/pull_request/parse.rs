//! Reading what the forges say: strings, lists and times in their JSON, and GitHub's checks.

use serde_json::Value;

use super::Check;

/// The string at `pointer`, or nothing.
pub(crate) fn at<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The array at `pointer`, or nothing.
pub(crate) fn items<'a>(v: &'a Value, pointer: &str) -> &'a [Value] {
    v.pointer(pointer)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// One entry of a `statusCheckRollup`: a check run, or a commit status of the older kind.
pub(crate) fn check(v: &Value) -> Check {
    if at(v, "/__typename") == "StatusContext" {
        return Check {
            name: at(v, "/context").to_owned(),
            state: crate::ci::github::rollup_state(at(v, "/state")),
            detail: at(v, "/description").to_owned(),
            url: at(v, "/targetUrl").to_owned(),
        };
    }
    Check {
        name: at(v, "/name").to_owned(),
        state: crate::ci::github::state(at(v, "/status"), at(v, "/conclusion")),
        detail: at(v, "/workflowName").to_owned(),
        url: at(v, "/detailsUrl").to_owned(),
    }
}

/// Seconds since the epoch of a time like `2026-11-02T20:37:34Z` or `2026-11-02T21:37:34+01:00`.
pub(crate) fn parse_time(iso: &str) -> Option<i64> {
    let (iso, offset) = match iso.strip_suffix('Z') {
        Some(utc) => (utc, 0),
        None => {
            let at = iso.len().checked_sub(6)?;
            let (time, zone) = (iso.get(..at)?, iso.get(at..)?);
            let sign = match zone.as_bytes().first() {
                Some(b'+') => 1,
                Some(b'-') => -1,
                _ => return None,
            };
            let (hours, minutes) = zone.get(1..)?.split_once(':')?;
            let offset = hours.parse::<i64>().ok()? * 3_600 + minutes.parse::<i64>().ok()? * 60;
            (time, sign * offset)
        }
    };
    let (date, time) = iso.split_once('T')?;
    let numbers = |text: &str, separator: char| -> Option<[i64; 3]> {
        let mut parts = text.split(separator).map(|n| n.parse::<i64>().ok());
        Some([parts.next()??, parts.next()??, parts.next()??])
    };
    let [year, month, day] = numbers(date, '-')?;
    let [hour, minute, second] = numbers(time.split('.').next()?, ':')?;
    // Out of range is no time (and huge numbers would overflow below).
    let fits = (0..=9999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && (0..=23).contains(&hour)
        && (0..=59).contains(&minute)
        && (0..=60).contains(&second);
    if !fits {
        return None;
    }
    // Days from the civil calendar, counted from the March before: leap days come last then.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year.rem_euclid(400);
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_cycle - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second - offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utc_times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(parse_time("2026-11-02T20:37:34Z"), Some(1_793_651_854));
        assert_eq!(parse_time("2026-11-02T21:37:34+01:00"), Some(1_793_651_854));
        assert_eq!(
            parse_time("2026-11-02T15:37:34.123-05:00"),
            Some(1_793_651_854)
        );
        assert_eq!(parse_time("2026-11-02T20:37:34"), None);
        assert_eq!(parse_time("not a time"), None);
        assert_eq!(parse_time(""), None);
        // Out of range: no time, rather than an overflow.
        assert_eq!(parse_time("0-0-52T9223372036854775807:4294967295:0Z"), None);
        assert_eq!(parse_time("9223372036854775807-01-01T00:00:00Z"), None);
        assert_eq!(parse_time("2026-13-01T00:00:00Z"), None);
    }
}
