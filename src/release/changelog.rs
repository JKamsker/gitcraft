//! Changelog insertion, release dates, and tag-name validation.

/// `CHANGELOG.md` with the release's notes added on top, under its first heading when it starts
/// with one (`# Changelog`).
pub(crate) fn with_entry(text: &str, tag: &str, notes: &str, date: &str) -> String {
    let entry = format!("## {tag} - {date}\n\n{}\n\n", notes.trim());
    let mut lines = text.split_inclusive('\n').peekable();
    let mut head = String::new();
    if lines.peek().is_some_and(|l| l.starts_with("# ")) {
        head.push_str(lines.next().unwrap_or_default());
        // The heading's own paragraph stays above.
        while let Some(l) = lines.peek() {
            if l.starts_with("## ") {
                break;
            }
            head.push_str(lines.next().unwrap_or_default());
        }
        if !head.ends_with("\n\n") {
            head.push('\n');
        }
    }
    let rest: String = lines.collect();
    format!("{head}{entry}{rest}")
}

/// Today's date, `YYYY-MM-DD` (UTC).
pub(crate) fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The calendar date of `days` since 1970-01-01 (Howard Hinnant's algorithm).
pub(super) fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Whether `tag` can be a new tag: something, not taken, nothing git would refuse in a name.
pub(crate) fn tag_problem(tag: &str, taken: &[String]) -> Option<String> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Some("Name the version".to_owned());
    }
    if taken.iter().any(|t| t == tag) {
        return Some(format!("{tag} is a tag already"));
    }
    let bad = tag.starts_with(['-', '.', '/'])
        || tag.ends_with(['.', '/'])
        || tag.ends_with(".lock")
        || tag.contains("..")
        || tag.contains("@{")
        || tag
            .chars()
            .any(|c| c.is_control() || " ~^:?*[\\".contains(c));
    bad.then(|| format!("{tag} can't be a tag's name"))
}
