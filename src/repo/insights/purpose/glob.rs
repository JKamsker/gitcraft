//! The patterns of a rules file against paths, in time linear in both: each (pattern
//! position, path position) is worked out once, so no pattern of stars backtracks.

/// A part of a pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Byte(u8),
    /// `?`: one byte, not `/`.
    One,
    /// `*`: any bytes within a folder.
    Star,
    /// `**`: any bytes.
    Deep,
    /// `**/`: no folder, or any number of whole ones.
    Folders,
}

fn parts(pattern: &[u8]) -> Vec<Part> {
    let mut out = Vec::with_capacity(pattern.len());
    let mut i = 0;
    while i < pattern.len() {
        let (part, len) = match &pattern[i..] {
            [b'*', b'*', b'/', ..] => (Part::Folders, 3),
            [b'*', b'*', ..] => (Part::Deep, 2),
            [b'*', ..] => (Part::Star, 1),
            [b'?', ..] => (Part::One, 1),
            [c, ..] => (Part::Byte(*c), 1),
            [] => break,
        };
        out.push(part);
        i += len;
    }
    out
}

/// A pattern without `/` matches a file's name; with one, its path (a leading `/` ignored);
/// ending in `/`, everything in that folder. `*` stays within a folder, `**` doesn't.
pub(in crate::repo::insights) fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, text) = subject(pattern, path);
    glob(&parts(pattern.as_bytes()), text.as_bytes())
}

/// What a pattern is matched as, and against what of `path`.
fn subject<'a>(pattern: &str, path: &'a str) -> (String, &'a str) {
    let pattern = match pattern.strip_suffix('/') {
        Some(folder) => format!("{folder}/**"),
        None => pattern.to_owned(),
    };
    match pattern.strip_prefix('/') {
        Some(rooted) => (rooted.to_owned(), path),
        None if pattern.contains('/') => (pattern, path),
        None => (pattern, path.rsplit('/').next().unwrap_or(path)),
    }
}

/// How much work matching `pattern` against `path` is, in cells of [`glob`]'s table.
pub(in crate::repo::insights) fn cost(pattern: &str, path: &str) -> usize {
    let (pattern, text) = subject(pattern, path);
    (pattern.len() + 1) * (text.len() + 1)
}

/// Whether `parts` match all of `text`: one row of the table per part, from the last.
fn glob(parts: &[Part], text: &[u8]) -> bool {
    let n = text.len();
    // `next[t]`: the parts after this one match `text[t..]`.
    let mut next = vec![false; n + 1];
    next[n] = true;
    let mut row = vec![false; n + 1];
    for &part in parts.iter().rev() {
        let mut folders = false;
        for t in (0..=n).rev() {
            let c = text.get(t).copied();
            row[t] = match part {
                Part::Byte(b) => c == Some(b) && next[t + 1],
                Part::One => c.is_some_and(|c| c != b'/') && next[t + 1],
                Part::Star => next[t] || (c.is_some_and(|c| c != b'/') && row[t + 1]),
                Part::Deep => next[t] || (c.is_some() && row[t + 1]),
                Part::Folders => {
                    folders |= c == Some(b'/') && next[t + 1];
                    next[t] || folders
                }
            };
        }
        if !row.contains(&true) {
            return false;
        }
        std::mem::swap(&mut next, &mut row);
    }
    next[0]
}
