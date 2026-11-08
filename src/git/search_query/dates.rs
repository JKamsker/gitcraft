//! The dates of `after:` and `before:`: a day (`2024-05-01`, `2024-05`, `2024`), `today`,
//! `yesterday`, or a time ago (`3d`, `2w`, `6m`, `1y`, `12h`), as git's `--since` / `--until`
//! read them. Relative dates are worked out by git, on the machine it runs on.

use std::time::{SystemTime, UNIX_EPOCH};

/// A date as `git log` takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitDate {
    /// The value of `--since=` / `--until=`.
    pub arg: String,
    /// The day it starts, when it is written as one (year, month, day).
    pub day: Option<(i32, u32, u32)>,
}

/// The longest span back a relative date may name: further would reach before 1970.
const MAX_YEARS: u32 = 50;
/// git reads a day from 2100 on as one of this year.
const LAST_YEAR: u32 = 2099;

/// How dates are written, for the hints and the help.
pub const DATE_FORMATS: &str = "backend-search-date-formats";
pub fn date_formats() -> &'static str {
    crate::i18n::text(DATE_FORMATS)
}

/// What `value` means as a date, or why it isn't one.
pub fn git_date(value: &str) -> Result<GitDate, String> {
    let lower = value.to_lowercase();
    let arg = match lower.as_str() {
        "today" => Some("midnight".to_owned()),
        "yesterday" => Some("yesterday.midnight".to_owned()),
        _ => relative(&lower),
    };
    if let Some(arg) = arg {
        return Ok(GitDate { arg, day: None });
    }
    let (y, m, d) = day(&lower).ok_or_else(|| {
        crate::i18n::message(
            "backend-value-is-not-a-date-write-date-formats-at-most-max-years-years-ba-cd7a9d",
            &[
                ("value", (value).to_string().into()),
                ("date_formats", date_formats().into()),
                ("max_years", MAX_YEARS.into()),
                ("last_year", (LAST_YEAR).to_string().into()),
            ],
        )
    })?;
    Ok(GitDate {
        arg: format!("{y:04}-{m:02}-{d:02} 00:00:00"),
        day: Some((y, m, d)),
    })
}

/// `2w`, `3d`, `12h`, `6m`, `1y`: so long ago.
fn relative(value: &str) -> Option<String> {
    let unit = value.chars().last()?;
    let number = &value[..value.len() - unit.len_utf8()];
    if number.is_empty() || number.len() > 6 || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = number.parse().ok()?;
    // git reads spans reaching before 1970 (or overflowing) as some other date: at most
    // MAX_YEARS back.
    let (unit, most) = match unit {
        'h' => ("hours", MAX_YEARS * 8_760),
        'd' => ("days", MAX_YEARS * 365),
        'w' => ("weeks", MAX_YEARS * 52),
        'm' => ("months", MAX_YEARS * 12),
        'y' => ("years", MAX_YEARS),
        _ => return None,
    };
    (n <= most).then(|| format!("{n}.{unit}.ago"))
}

/// `2024-05-01`, `2024-05` (its first day) or `2024` (its first day).
fn day(value: &str) -> Option<(i32, u32, u32)> {
    let mut parts = value.split('-');
    let year = number(parts.next()?, 4, 4)?;
    let month = parts.next().map_or(Some(1), |m| number(m, 1, 2))?;
    let day = parts.next().map_or(Some(1), |d| number(d, 1, 2))?;
    if parts.next().is_some() || !(1970..=LAST_YEAR).contains(&year) {
        return None;
    }
    let (year, month, day) = (year as i32, month, day);
    ((1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day))
        .then_some((year, month, day))
}

fn number(text: &str, min_len: usize, max_len: usize) -> Option<u32> {
    let ok = (min_len..=max_len).contains(&text.len()) && text.chars().all(|c| c.is_ascii_digit());
    if ok { text.parse().ok() } else { None }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A date offered while typing `after:` or `before:`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateSuggestion {
    pub value: String,
    pub detail: String,
}

/// The dates offered for `after:` / `before:`: recent spans, then the start of this month
/// and of this year (by the UTC calendar).
pub fn date_suggestions() -> Vec<DateSuggestion> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (year, month, _) = civil_from_days((now / 86_400) as i64);
    let mut out: Vec<DateSuggestion> = [
        ("today", "midnight today"),
        ("yesterday", "midnight yesterday"),
        ("1w", "one week ago"),
        ("1m", "one month ago"),
        ("3m", "three months ago"),
        ("1y", "one year ago"),
    ]
    .into_iter()
    .map(|(value, detail)| DateSuggestion {
        value: value.to_owned(),
        detail: detail.to_owned(),
    })
    .collect();
    out.push(DateSuggestion {
        value: format!("{year:04}-{month:02}-01"),
        detail: "the start of this month".to_owned(),
    });
    out.push(DateSuggestion {
        value: format!("{year:04}-01-01"),
        detail: "the start of this year".to_owned(),
    });
    out
}

/// The (year, month, day) of `days` since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_dates_are_so_long_ago() {
        for (value, arg) in [
            ("2w", "2.weeks.ago"),
            ("3d", "3.days.ago"),
            ("12h", "12.hours.ago"),
            ("6m", "6.months.ago"),
            ("1Y", "1.years.ago"),
            ("0d", "0.days.ago"),
            ("50y", "50.years.ago"),
            ("600m", "600.months.ago"),
            ("18250d", "18250.days.ago"),
            ("438000h", "438000.hours.ago"),
            ("today", "midnight"),
            ("Yesterday", "yesterday.midnight"),
        ] {
            assert_eq!(
                git_date(value).map(|d| d.arg),
                Ok(arg.to_owned()),
                "{value}"
            );
        }
    }

    #[test]
    fn days_start_at_midnight() {
        for (value, arg) in [
            ("2024-05-01", "2024-05-01 00:00:00"),
            ("2024-5-1", "2024-05-01 00:00:00"),
            ("2024-05", "2024-05-01 00:00:00"),
            ("2024", "2024-01-01 00:00:00"),
            ("2024-02-29", "2024-02-29 00:00:00"),
            ("2000-02-29", "2000-02-29 00:00:00"),
        ] {
            assert_eq!(
                git_date(value).map(|d| d.arg),
                Ok(arg.to_owned()),
                "{value}"
            );
        }
    }

    #[test]
    fn what_is_not_a_date_is_refused() {
        for value in [
            "",
            "w",
            "2",
            "2x",
            "-2w",
            "2.5w",
            "123456d",
            "2023-02-29",
            "1900-02-29",
            "2024-13",
            "2024-00-10",
            "2024-04-31",
            "24-05-01",
            "2024-05-01-01",
            "1969",
            "next week",
            "２w",
            "2024-05-01T10:00",
            "1.weeks.ago",
            "now",
            "51y",
            "601m",
            "18251d",
            "2601w",
            "438001h",
            "1000000h",
            "99999y",
            "2100",
            "2100-01-01",
            "9999-12-31",
        ] {
            assert!(git_date(value).is_err(), "{value}");
        }
    }

    #[test]
    fn the_calendar_is_right() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(20_763), (2026, 11, 6));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn suggested_dates_are_all_dates() {
        let suggestions = date_suggestions();
        assert_eq!(suggestions.len(), 8);
        for s in suggestions {
            assert!(git_date(&s.value).is_ok(), "{}", s.value);
        }
    }
}
