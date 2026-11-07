use std::fmt;

/// A civil day in the timezone recorded on the chosen Git timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Day {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Day {
    pub fn parse(s: &str) -> Option<Self> {
        if s.len() != 10 || s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-')
        {
            return None;
        }
        if s.bytes()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
        {
            return None;
        }
        let day = Self {
            year: s.get(..4)?.parse().ok()?,
            month: s.get(5..7)?.parse().ok()?,
            day: s.get(8..)?.parse().ok()?,
        };
        (day.year >= 1
            && day.year <= 9999
            && (1..=12).contains(&day.month)
            && (1..=day.month_days()).contains(&day.day))
        .then_some(day)
    }
    pub fn timestamp(s: &[u8]) -> Result<Self, String> {
        std::str::from_utf8(s)
            .ok()
            .and_then(|s| s.get(..10))
            .and_then(Self::parse)
            .ok_or_else(|| "Git returned an unsupported calendar date".into())
    }
    pub fn from_unix_days(days: i64) -> Option<Self> {
        let z = days.checked_add(719468)?;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        let year = year + i64::from(month <= 2);
        if !(1..=9999).contains(&year) {
            return None;
        }
        Some(Self {
            year: year as i32,
            month: month as u32,
            day: day as u32,
        })
    }
    pub fn month_days(self) -> u32 {
        match self.month {
            4 | 6 | 9 | 11 => 30,
            2 => {
                if self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => 31,
        }
    }
    pub fn month_start(self) -> Self {
        Self { day: 1, ..self }
    }
    pub fn shift_month(self, n: i32) -> Self {
        let m = (self.year * 12 + self.month as i32 - 1 + n).clamp(12, 9999 * 12 + 11);
        Self {
            year: m.div_euclid(12),
            month: m.rem_euclid(12) as u32 + 1,
            day: 1,
        }
    }
    /// Monday=0; Gregorian calendar, including dates before Unix epoch.
    pub fn weekday(self) -> u32 {
        let y = self.year - i32::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = self.month as i32 + if self.month > 2 { -3 } else { 9 };
        let doy = (153 * m + 2) / 5 + self.day as i32 - 1;
        let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
        (days + 3).rem_euclid(7) as u32
    }
}
impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}
