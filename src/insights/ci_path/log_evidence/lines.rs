//! Recognizers for the single lines a job log is read by: cargo's and the shard script's
//! binary headers, libtest's announcements and results, and the cache, toolchain and error
//! lines kept as notes.

/// GitHub's timestamp prefix, terminal colours and what a carriage return wrote over removed.
pub(super) fn plain(raw: &str) -> String {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let raw = match raw.split_once(' ') {
        Some((stamp, rest)) if timestamp(stamp) => rest,
        _ => raw,
    };
    let raw = raw.trim_end_matches('\r');
    let raw = raw.rsplit('\r').next().unwrap_or(raw);
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.next_if_eq(&'[').is_some() {
                while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn timestamp(stamp: &str) -> bool {
    let b = stamp.as_bytes();
    b.len() >= 20
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[10] == b'T'
        && stamp.ends_with('Z')
}

/// The suite a test binary's header names, by the binary's file name without its build hash
/// (`gitcraft-0f3a….exe` → `gitcraft`), so it is the same suite in every build:
/// - cargo's `Running unittests src/lib.rs (target/debug/deps/gitcraft-0f3a…)` →
///   `gitcraft (unittests src/lib.rs)`, which keeps a crate's library and binary apart;
/// - cargo's `Doc-tests gitcraft` → `doc-tests gitcraft`;
/// - ci/test-shards.py's `gitcraft-0f3a….exe: serial agents: 12 tests`, printed before it runs
///   a binary directly → `gitcraft`.
pub(super) fn binary(line: &str) -> Option<String> {
    if let Some(name) = line.strip_prefix("Doc-tests ") {
        let name = name.trim();
        return (!name.is_empty() && !name.contains(' ')).then(|| format!("doc-tests {name}"));
    }
    if let Some(running) = line.strip_prefix("Running ") {
        let (source, path) = running.trim().strip_suffix(')')?.split_once(" (")?;
        let (stem, _) = stem(path);
        return (!stem.is_empty()).then(|| format!("{stem} ({})", source.trim()));
    }
    let (file, rest) = line.split_once(": ")?;
    let (_, count) = rest.rsplit_once(": ")?;
    let count = count
        .strip_suffix(" tests")
        .or_else(|| count.strip_suffix(" test"))?;
    count.parse::<usize>().ok()?;
    // Only a hashed binary's name: other `a: b: 3 tests` lines are not headers.
    match stem(file) {
        (stem, true) if !stem.is_empty() && !file.contains(char::is_whitespace) => Some(stem),
        _ => None,
    }
}

/// A built test binary's file name without its directory, `.exe` and `-<hash>`, and whether
/// it had the build hash every test binary cargo builds carries.
fn stem(path: &str) -> (String, bool) {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let file = file.strip_suffix(".exe").unwrap_or(file);
    match file.rsplit_once('-') {
        Some((stem, hash)) if hash.len() >= 8 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {
            (stem.to_owned(), true)
        }
        _ => (file.to_owned(), false),
    }
}

/// libtest's `running 12 tests` / `running 1 test`.
pub(super) fn announced(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("running ")?;
    let (count, word) = rest.split_once(' ')?;
    matches!(word, "test" | "tests")
        .then(|| count.parse().ok())
        .flatten()
}

/// libtest's `test path::name ... ` and what follows on the line: the outcome, or with
/// `--nocapture` (and `--test-threads=1`) the test's own output, its outcome on a later line.
pub(super) fn test_start(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("test ")?;
    let (name, after) = rest.split_once(" ...")?;
    let after = after.strip_prefix(' ').unwrap_or(after);
    (!name.is_empty()).then_some((name, after))
}

/// libtest's outcome words: `ok`, `FAILED`, `ignored` and `ignored, <reason>`.
pub(super) fn outcome(text: &str) -> Option<super::Outcome> {
    use super::Outcome;
    match text.trim() {
        "ok" => Some(Outcome::Passed),
        "FAILED" => Some(Outcome::Failed),
        r if r == "ignored" || r.starts_with("ignored, ") => Some(Outcome::Ignored),
        _ => None,
    }
}

/// cargo's `error: test failed, to rerun pass `--lib``: without `--no-fail-fast` cargo stops
/// at the first failing binary, so the binaries after it never ran. (`--no-fail-fast` runs
/// every binary and ends with `error: N targets failed:` instead, which leaves none out.)
pub(super) fn stopped(line: &str) -> bool {
    line.starts_with("error: test failed")
}

/// A version line of the Rust toolchain: `rustc 1.95.0 (…)`, `cargo 1.95.0 (…)`.
pub(super) fn toolchain(line: &str) -> bool {
    ["rustc ", "cargo ", "rustfmt ", "clippy "]
        .iter()
        .any(|tool| {
            line.strip_prefix(tool).is_some_and(|rest| {
                let version = rest.split(' ').next().unwrap_or_default();
                version.split('.').count() >= 3
                    && version
                        .split(['.', '-'])
                        .next()
                        .is_some_and(|major| major.parse::<u32>().is_ok())
            })
        })
}

/// A cache line of actions/cache or Swatinem/rust-cache: hit or miss, and the key.
pub(super) fn cache(line: &str, short: impl Fn(&str) -> String) -> Option<(bool, String)> {
    if let Some(key) = line.strip_prefix("Cache restored from key:") {
        Some((true, short(key)))
    } else if let Some(key) = line.strip_prefix("Cache not found for input keys:") {
        Some((false, short(key)))
    } else if let Some(key) = line.strip_prefix("Restored from cache key ") {
        Some((true, short(key.trim_matches('"'))))
    } else if line == "No cache found." {
        Some((false, "(key not logged)".to_owned()))
    } else {
        None
    }
}
