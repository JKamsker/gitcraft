use super::Value;
use num_format::{Locale, ToFormattedString};

/// Fluent's default numeric renderer is locale-independent. Keep its numeric Value intact
/// for plural rules, overriding presentation only (English/German decimal/group separators).
pub(super) fn format(value: &Value<'_>, german: bool) -> Option<String> {
    let Value::Number(n) = value else {
        return None;
    };
    if !n.value.is_finite() {
        return Some(n.value.to_string());
    }
    let max = n.options.maximum_fraction_digits.map(|d| d.min(15));
    let min = n.options.minimum_fraction_digits.unwrap_or(0).min(15);
    let raw = match max {
        Some(d) => format!("{:.*}", d.max(min), n.value),
        None => n.value.to_string(),
    };
    let (integer, fraction) = raw.split_once('.').unwrap_or((&raw, ""));
    let mut fraction = fraction.trim_end_matches('0').to_owned();
    while fraction.len() < min {
        fraction.push('0');
    }
    let mut integer = if n.options.use_grouping {
        integer
            .parse::<i128>()
            .map(|n| n.to_formatted_string(if german { &Locale::de } else { &Locale::en }))
            .unwrap_or_else(|_| integer.to_owned())
    } else {
        integer.to_owned()
    };
    if !fraction.is_empty() {
        integer.push(if german { ',' } else { '.' });
        integer.push_str(&fraction);
    }
    Some(integer)
}
