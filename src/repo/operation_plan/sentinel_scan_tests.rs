//! Reads gitcraft's own source for NUL-prefixed string literals: the internal sentinels an
//! operation step, a graph row or an output can start with (#193). It sees an ordinary
//! single-line literal whose NUL is followed by a letter, a sentinel's name; a value built
//! with `concat!`/`format!` or a string continued over lines is out of its reach. Line-based:
//! rustfmt closes a `#[cfg(test)]` item with a `}` at the attribute's indentation, which is
//! all it takes to leave test-only code out. Files named as tests are left out whole.

use std::path::{Path, PathBuf};

/// One NUL-prefixed literal in production source.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Found {
    /// Relative to the crate, with `/` (`src/gerrit.rs`).
    pub file: String,
    pub line: usize,
    /// The constant it initializes; `None` for a literal used inline.
    pub constant: Option<String>,
    pub value: String,
}

/// Every NUL-prefixed literal under `root/src`, in path order.
pub(super) fn scan(root: &Path) -> Result<Vec<Found>, String> {
    let mut files = Vec::new();
    collect(&root.join("src"), &mut files)?;
    files.sort();
    let mut found = Vec::new();
    for path in files {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let file = relative.to_string_lossy().replace('\\', "/");
        found.extend(scan_text(&file, &text));
    }
    Ok(found)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// A file or folder named the way gitcraft names test modules (`tests.rs`, `x_tests.rs`,
/// `x_tests/`, `readability_tests_lanes.rs`, `rebase_test_support.rs`).
fn test_only(file: &str) -> bool {
    file.split('/').any(|part| {
        let stem = part.strip_suffix(".rs").unwrap_or(part);
        stem == "tests"
            || stem.ends_with("_tests")
            || stem.contains("_tests_")
            || stem.ends_with("test_support")
    })
}

pub(super) fn scan_text(file: &str, text: &str) -> Vec<Found> {
    if test_only(file) {
        return Vec::new();
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(line) = lines.get(at) {
        let code = line.trim_start();
        if code == "#[cfg(test)]" || code.starts_with("#[cfg(all(test") {
            at = past_item(&lines, at);
            continue;
        }
        if !code.starts_with("//") {
            found.extend(literals(line).into_iter().map(|value| Found {
                file: file.to_owned(),
                line: at + 1,
                constant: constant_name(code),
                value,
            }));
        }
        at += 1;
    }
    found
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The line after the item the `#[cfg(test)]` at `at` gates.
fn past_item(lines: &[&str], at: usize) -> usize {
    let indent = lines.get(at).map_or(0, |l| indentation(l));
    let mut next = at + 1;
    while lines.get(next).is_some_and(|l| {
        let code = l.trim_start();
        code.is_empty() || code.starts_with("#[") || code.starts_with("//")
    }) {
        next += 1;
    }
    let Some(item) = lines.get(next).map(|l| l.trim_end()) else {
        return next;
    };
    // `mod x;`, a field, `fn f() {}`, `thread_local! { … }`: the item is this line.
    if item.ends_with([';', ',', '}']) {
        return next + 1;
    }
    // Otherwise rustfmt closes it at the attribute's indentation: `}`, `];` or `);`. A wrapped
    // signature's `) -> T {` starts the same way but opens the body, so it does not close.
    let closes = |l: &&str| {
        let code = l.trim();
        indentation(l) == indent
            && code.starts_with(['}', ']', ')'])
            && code.ends_with(['}', ';', ','])
    };
    lines[next..]
        .iter()
        .position(closes)
        .map_or(lines.len(), |end| next + end + 1)
}

/// `NAME` of a `[pub(..)] const NAME: &str = …` line.
fn constant_name(code: &str) -> Option<String> {
    let (before, rest) = code.split_once("const ")?;
    let visibility = before.trim();
    if !(visibility.is_empty() || visibility.starts_with("pub")) {
        return None;
    }
    let (name, _) = rest.split_once(':')?;
    Some(name.trim().to_owned())
}

/// The ordinary (not byte, raw or char) string literals on `line` that start with NUL and
/// then a letter: a sentinel's name. A bare `"\0"` separator or a binary payload is no
/// sentinel.
fn literals(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in line.match_indices('"') {
        let before = line[..at].chars().next_back();
        if before.is_some_and(|c| matches!(c, 'b' | 'r' | '#' | '\'' | '\\')) {
            continue;
        }
        let rest = &line[at + 1..];
        if !["\\0", "\\u{0}", "\\u{00}", "\\x00"]
            .iter()
            .any(|p| rest.starts_with(p))
        {
            continue;
        }
        if let Some(value) = unescape(rest)
            && value
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_alphabetic())
        {
            out.push(value);
        }
    }
    out
}

/// The literal's value from just after its opening quote; `None` when it doesn't end on
/// this line or holds an escape this reader doesn't know.
fn unescape(rest: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(escaped(&mut chars)?),
            c => out.push(c),
        }
    }
    None
}

fn escaped(chars: &mut std::str::Chars<'_>) -> Option<char> {
    Some(match chars.next()? {
        '0' => '\0',
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        c @ ('\\' | '"' | '\'') => c,
        'x' => {
            let hex: String = chars.by_ref().take(2).collect();
            char::from(u8::from_str_radix(&hex, 16).ok()?)
        }
        'u' => {
            if chars.next()? != '{' {
                return None;
            }
            let hex: String = chars.by_ref().take_while(|c| *c != '}').collect();
            char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
        }
        _ => return None,
    })
}

#[test]
fn constants_and_inline_literals_are_found_outside_test_only_code() {
    let source = r#"
pub(crate) const MARK: &str = "\0mark";
const TRIED: &str = "\u{0}tried\u{0}";
pub(in crate::repo) const HEX: &str = "\x00hex";
const SEPARATOR: &str = "\0";
fn inline(s: &str) -> bool { s.starts_with("\u{0}inline:") || s == "\0" }
fn bytes() -> &'static [u8] { b"\0binary" }
// "\0comment"
#[cfg(test)]
mod tests {
    const HIDDEN: &str = "\0hidden";
}
#[cfg(test)]
#[test]
fn hidden() -> Result<(), String> {
    let _ = "\0hidden-too";
    Ok(())
}
#[cfg(test)]
mod declared;
const AFTER: &str = "\0after";
#[cfg(test)]
thread_local! { static CELL: u8 = 0; }
const PAST_MACRO: &str = "\0past-macro";
#[cfg(test)]
const TABLE: &[&str] = &[
    "\0in-table",
];
const PAST_TABLE: &str = "\0past-table";
"#;
    let found: Vec<_> = scan_text("src/x.rs", source)
        .into_iter()
        .map(|f| (f.line, f.constant, f.value))
        .collect();
    let named = |line, name: &str, value: &str| (line, Some(name.to_owned()), value.to_owned());
    assert_eq!(
        found,
        [
            named(2, "MARK", "\0mark"),
            named(3, "TRIED", "\0tried\0"),
            named(4, "HEX", "\0hex"),
            (6, None, "\0inline:".to_owned()),
            named(21, "AFTER", "\0after"),
            named(24, "PAST_MACRO", "\0past-macro"),
            named(29, "PAST_TABLE", "\0past-table"),
        ]
    );
    assert!(scan_text("src/x/tests.rs", source).is_empty());
    assert!(scan_text("src/x_tests/a.rs", source).is_empty());
    assert!(!scan_text("src/insights/ci_path/test_history.rs", source).is_empty());
}

#[test]
fn a_wrapped_test_signature_keeps_its_body_test_only() {
    let source = r#"
#[cfg(test)]
fn wrapped(
    value: u8,
) -> u8 {
    let _ = "\0in-body";
    value
}
const AFTER: &str = "\0after";
"#;
    let found: Vec<_> = scan_text("src/x.rs", source)
        .into_iter()
        .map(|f| (f.line, f.value))
        .collect();
    assert_eq!(found, [(9, "\0after".to_owned())]);
}
