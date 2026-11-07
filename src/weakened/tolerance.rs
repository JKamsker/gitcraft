//! Tolerances loosened: a line that compares within some margin, removed, and the same line
//! added back with a wider one (`rtol=1e-6` → `rtol=1e-2`, `toBeCloseTo(x, 4)` → `(x, 1)`).

/// Words that name a margin, where a larger number lets more through.
const MARGINS: [&str; 12] = [
    "rtol",
    "atol",
    "abs_tol",
    "rel_tol",
    "rel",
    "abs",
    "tolerance",
    "tol",
    "epsilon",
    "max_relative",
    "max_ulps",
    "delta",
];

/// Words that name a precision in digits, where a smaller number lets more through; with a
/// call, how many arguments it takes before the digits are among them (0: a keyword).
const DIGITS: [(&str, usize); 4] = [
    ("places", 0),
    ("decimal", 0),
    ("tobecloseto(", 2),
    ("assertalmostequal(", 3),
];

/// A margin a line sets: the word, its number, and the number as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Margin {
    pub word: &'static str,
    pub value: f64,
    pub written: String,
    /// Smaller is looser (a number of digits).
    pub digits: bool,
}

/// Every margin `text` sets, in the order of their words.
pub fn margins(text: &str) -> Vec<Margin> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for word in MARGINS {
        for at in word_places(&lower, word) {
            if let Some((value, written)) = value_after(&lower[at + word.len()..]) {
                out.push(Margin {
                    word,
                    value,
                    written,
                    digits: false,
                });
            }
        }
    }
    for (word, args) in DIGITS {
        for at in word_places(&lower, word) {
            let rest = &lower[at + word.len()..];
            let found = if args > 0 {
                last_argument(rest, args)
            } else {
                value_after(rest)
            };
            if let Some((value, written)) = found {
                out.push(Margin {
                    word,
                    value,
                    written,
                    digits: true,
                });
            }
        }
    }
    out
}

/// Where `word` stands as a word of its own in `text` (`tol`, not the `tol` of `atol`).
fn word_places<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(word)
        .map(|(at, _)| at)
        .filter(move |&at| {
            let before = text[..at].chars().next_back();
            let after = text[at + word.len()..].chars().next();
            let free = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            free(before) && (word.ends_with('(') || free(after))
        })
}

/// The number given to a word: right after it, past `=`, `:`, `(` and spaces only.
fn value_after(rest: &str) -> Option<(f64, String)> {
    let start = rest.find(|c: char| !matches!(c, ' ' | '=' | ':' | '('))?;
    if start == 0 || start > 4 {
        return None;
    }
    number_at(&rest[start..])
}

/// The literal number at the start of `text`: `1e-6`, `0.001`, `1_000`, `.5`.
fn number_at(text: &str) -> Option<(f64, String)> {
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len() {
        let c = bytes[end];
        let exponent_sign =
            (c == b'-' || c == b'+') && end > 0 && matches!(bytes[end - 1], b'e' | b'E');
        if c.is_ascii_digit() || c == b'.' || c == b'_' || c == b'e' || c == b'E' || exponent_sign {
            end += 1;
        } else {
            break;
        }
    }
    let written = text[..end].trim_end_matches(['e', 'E', '.', '_', '-', '+']);
    // `10**-6`, `1/1000`: a number worked out, not one written; left alone.
    if written.is_empty() || text[written.len()..].starts_with(['*', '/']) {
        return None;
    }
    let digits: String = written.chars().filter(|c| *c != '_').collect();
    digits.parse::<f64>().ok().map(|v| (v, written.to_owned()))
}

/// The last argument of the call `text` is inside of, as a number, when the call is given at
/// least `min` arguments (fewer: the digits are left to their default). Parentheses nested
/// in the arguments (`toBeCloseTo(sum(a, b), 2)`) are skipped over.
fn last_argument(text: &str, min: usize) -> Option<(f64, String)> {
    let mut depth = 0usize;
    let mut args = vec![0usize];
    let mut end = text.len();
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                end = i;
                break;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => args.push(i + 1),
            _ => {}
        }
    }
    if args.len() < min {
        return None;
    }
    let last = text[*args.last()?..end].trim();
    let last = last.rsplit('=').next().unwrap_or(last).trim();
    number_at(last).filter(|(_, w)| w.len() == last.len())
}

/// The margins `after` loosens from `before`: the same word, a looser number.
pub fn loosened<'a>(before: &[Margin], after: &'a [Margin]) -> Vec<(&'a Margin, String)> {
    after
        .iter()
        .filter_map(|a| {
            let b = before.iter().find(|b| b.word == a.word)?;
            let looser = if a.digits {
                a.value < b.value
            } else {
                a.value > b.value
            };
            looser.then(|| (a, b.written.clone()))
        })
        .collect()
}
