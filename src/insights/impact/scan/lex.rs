//! Mask literals and comments, then tokenize while preserving line positions.

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// `text` with what its comments, strings and character literals hold turned into spaces,
/// their delimiters too; line breaks are kept, so lines stay where they were.
pub fn mask(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let blank = |out: &mut String, from: &[char]| {
        for &c in from {
            out.push(if c == '\n' { '\n' } else { ' ' });
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let after_ident = i > 0 && is_ident(chars[i - 1]);
        let end = if c == '/' && next == Some('/') {
            Some(
                chars[i..]
                    .iter()
                    .position(|&c| c == '\n')
                    .map_or(chars.len(), |p| i + p),
            )
        } else if c == '/' && next == Some('*') {
            Some(block_comment_end(&chars, i))
        } else if c == '"' {
            Some(string_end(&chars, i + 1))
        } else if after_ident {
            None
        } else if let Some(end) = raw_string_end(&chars, i) {
            Some(end)
        } else if matches!(c, 'b' | 'c') && next == Some('"') {
            Some(string_end(&chars, i + 2))
        } else if c == 'b' && next == Some('\'') {
            char_end(&chars, i + 1)
        } else if c == '\'' {
            char_end(&chars, i)
        } else {
            None
        };
        match end {
            Some(end) => {
                blank(&mut out, &chars[i..end]);
                i = end;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Where the block comment starting at `i` ends (after its `*/`), comments in it nested.
fn block_comment_end(chars: &[char], i: usize) -> usize {
    let mut depth = 0;
    let mut j = i;
    while j < chars.len() {
        if chars[j] == '/' && chars.get(j + 1) == Some(&'*') {
            depth += 1;
            j += 2;
        } else if chars[j] == '*' && chars.get(j + 1) == Some(&'/') {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return j;
            }
        } else {
            j += 1;
        }
    }
    chars.len()
}

/// Where the string whose text starts at `from` ends (after its closing quote).
fn string_end(chars: &[char], from: usize) -> usize {
    let mut j = from;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            '"' => return j + 1,
            _ => j += 1,
        }
    }
    chars.len()
}

/// Where a raw string (`r"…"`, `br#"…"#`, `cr##"…"##`) starting at `i` ends; `None` when none
/// starts there (a raw identifier `r#name` is not one).
fn raw_string_end(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if matches!(chars.get(j), Some('b' | 'c')) {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if chars.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    while j < chars.len() {
        if chars[j] == '"' && (1..=hashes).all(|k| chars.get(j + k) == Some(&'#')) {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(chars.len())
}

/// Where the character literal starting with the quote at `i` ends; `None` when it is a
/// lifetime or a label (`'a`, `'static`).
fn char_end(chars: &[char], i: usize) -> Option<usize> {
    match chars.get(i + 1) {
        Some('\\') => {
            // `'\n'`, `'\''`, `'\u{1F600}'`: the escaped character, then up to the quote.
            let close = chars
                .get(i + 3..)?
                .iter()
                .take(10)
                .position(|&c| c == '\'')?;
            Some(i + 3 + close + 1)
        }
        Some(&c) if c != '\'' && chars.get(i + 2) == Some(&'\'') => Some(i + 3),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Tok {
    Ident(String),
    Punct(char),
    /// A lifetime or a label.
    Life,
}

/// The tokens of masked text, each with its line. Numbers are left out.
pub(super) fn tokens(masked: &str) -> Vec<(Tok, u32)> {
    let chars: Vec<char> = masked.chars().collect();
    let mut out = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '\'' && chars.get(i + 1).is_some_and(|&n| is_ident_start(n)) {
            i += 1;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            out.push((Tok::Life, line));
        } else if c.is_ascii_digit() {
            while i < chars.len() && (is_ident(chars[i]) || chars[i] == '.') {
                // A range (`0..n`) isn't part of the number.
                if chars[i] == '.' && chars.get(i + 1) == Some(&'.') {
                    break;
                }
                i += 1;
            }
        } else if is_ident_start(c) {
            let mut j = i;
            while j < chars.len() && is_ident(chars[j]) {
                j += 1;
            }
            let mut name: String = chars[i..j].iter().collect();
            // A raw identifier, `r#match`.
            if name == "r"
                && chars.get(j) == Some(&'#')
                && chars.get(j + 1).is_some_and(|&n| is_ident_start(n))
            {
                let k = j + 1;
                j = k;
                while j < chars.len() && is_ident(chars[j]) {
                    j += 1;
                }
                name = chars[k..j].iter().collect();
            }
            out.push((Tok::Ident(name), line));
            i = j;
        } else {
            out.push((Tok::Punct(c), line));
            i += 1;
        }
    }
    out
}

/// For each opening bracket, where it closes.
pub(super) fn pairs(toks: &[(Tok, u32)]) -> Vec<Option<usize>> {
    let mut pair = vec![None; toks.len()];
    let mut open: Vec<(char, usize)> = Vec::new();
    for (i, (tok, _)) in toks.iter().enumerate() {
        let Tok::Punct(c) = tok else {
            continue;
        };
        let opener = match c {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            '(' | '[' | '{' => {
                open.push((*c, i));
                continue;
            }
            _ => continue,
        };
        if let Some(at) = open.iter().rposition(|(o, _)| *o == opener) {
            pair[open[at].1] = Some(i);
            open.truncate(at);
        }
    }
    pair
}
