//! Tolerances loosened: a line that compares within some margin, removed, and the same line
//! added back with a wider one (`rtol=1e-6` → `rtol=1e-2`, `toBeCloseTo(x, 4)` → `(x, 1)`).

/// Words that name a margin, where a larger number lets more through.
const MARGINS: [&str; 16] = [
    "rtol",
    "atol",
    "abs_tol",
    "rel_tol",
    "rel=",
    "abs=",
    "tolerance",
    "tol=",
    "tol:",
    "epsilon",
    "max_relative",
    "max_ulps",
    "delta=",
    "threshold",
    "margin",
    "within(",
];

/// Words that name a precision in digits, where a smaller number lets more through.
/// With a call, how many arguments it takes before the digits are among them.
const DIGITS: [(&str, usize); 4] = [
    ("places=", 0),
    ("decimal=", 0),
    ("tobecloseto(", 2),
    ("assertalmostequal(", 3),
];

/// What a line's margin is: the word and the number it is given.
#[derive(Clone, Debug, PartialEq)]
pub struct Margin {
    pub word: &'static str,
    pub value: f64,
    /// Smaller is looser (a number of digits).
    pub digits: bool,
}

/// The margin `text` sets, if it sets one.
pub fn margin(text: &str) -> Option<Margin> {
    let lower = text.to_lowercase();
    for word in MARGINS {
        if let Some(at) = lower.find(word) {
            let value = first_number(&lower[at + word.len()..])?;
            return Some(Margin {
                word,
                value,
                digits: false,
            });
        }
    }
    for (word, args) in DIGITS {
        if let Some(at) = lower.find(word) {
            let rest = &lower[at + word.len()..];
            // The digits are the call's last argument (`toBeCloseTo(x, 2)`), or the value of
            // `places=`.
            let value = if args > 0 {
                last_number_in_call(rest, args)?
            } else {
                first_number(rest)?
            };
            return Some(Margin {
                word,
                value,
                digits: true,
            });
        }
    }
    None
}

/// Whether going from `before` to `after` lets more through.
pub fn loosened(before: &Margin, after: &Margin) -> bool {
    before.word == after.word
        && if before.digits {
            after.value < before.value
        } else {
            after.value > before.value
        }
}

/// The first number in `text` (soon after its start): `1e-6`, `0.001`, `1_000`, `.5`.
fn first_number(text: &str) -> Option<f64> {
    let start = text.find(|c: char| c.is_ascii_digit() || c == '.')?;
    // Too far from the word to be its value.
    if start > 24 {
        return None;
    }
    number_at(&text[start..])
}

fn number_at(text: &str) -> Option<f64> {
    let mut end = 0;
    let bytes = text.as_bytes();
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
    let literal: String = text[..end]
        .chars()
        .filter(|c| *c != '_')
        .collect::<String>()
        .trim_end_matches(['e', 'E', '.'])
        .to_owned();
    literal.parse::<f64>().ok()
}

/// The last argument of the call `text` is inside of (up to its `)`) as a number, when the
/// call is given at least `min` arguments (fewer: the digits are left to their default).
fn last_number_in_call(text: &str, min: usize) -> Option<f64> {
    let args = text.split(')').next().unwrap_or(text);
    if args.split(',').count() < min {
        return None;
    }
    let last = args.rsplit(',').next()?.trim();
    let last = last.rsplit('=').next().unwrap_or(last).trim();
    number_at(last)
}
