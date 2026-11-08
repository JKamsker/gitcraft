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
            .ok_or_else(|| {
                crate::i18n::text("backend-git-returned-an-unsupported-calendar-date-791647").into()
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
