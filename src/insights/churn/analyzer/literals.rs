//! Literal and comment boundaries for the advisory churn lexer.
use super::Lang;

/// Where a string quoted by `quote` that starts at `i` (after its opening quote) ends: after
/// its closing quote, a backslash escaping what follows. `line`: it can't go past a line's end
/// (a stray quote doesn't swallow the file).
pub(super) fn skip_quoted(b: &[u8], mut i: usize, quote: u8, escapes: bool, line: bool) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' if escapes => i += 2,
            b'\n' if line => return i + 1,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// Where a Rust raw string whose `#`s and opening quote start at `i` ends, or `None` when it
/// isn't one.
pub(super) fn skip_raw(b: &[u8], i: usize) -> Option<usize> {
    let hashes = b[i..].iter().take_while(|&&c| c == b'#').count();
    if b.get(i + hashes) != Some(&b'"') {
        return None;
    }
    let mut j = i + hashes + 1;
    while j < b.len() {
        if b[j] == b'"'
            && b[j + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == b'#')
                .count()
                == hashes
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(b.len())
}

/// Rust nests block comments; other supported dialects stop at the first close.
pub(super) fn skip_block(b: &[u8], mut i: usize, lang: Lang) -> usize {
    // Rust's block comments nest; the others' end at the first `*/`.
    let mut depth = 1;
    i += 2;
    while i < b.len() && depth > 0 {
        if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
            depth -= 1;
            i += 2;
        } else if lang == Lang::Rust && b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            depth += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    i
}

/// A Rust character literal consumes one character or escape; lifetimes keep scanning.
pub(super) fn skip_character(b: &[u8], mut i: usize) -> usize {
    let next = b.get(i + 1).copied();
    // A character literal (`'x'`, `'\n'`, `'ü'`), else a lifetime or a label.
    if next == Some(b'\\') {
        i = skip_quoted(b, i + 1, b'\'', true, true);
    } else {
        let len = match next {
            Some(x) if x >= 0xF0 => 4,
            Some(x) if x >= 0xE0 => 3,
            Some(x) if x >= 0xC0 => 2,
            _ => 1,
        };
        if b.get(i + 1 + len) == Some(&b'\'') {
            i += len + 2;
        } else {
            i += 1;
        }
    }
    i
}
