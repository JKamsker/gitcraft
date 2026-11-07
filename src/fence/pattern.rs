//! A fence entry: a path pattern in the spirit of `.gitignore`.
//!
//! - `name` (no `/`): a file or folder of that name anywhere (`.env`, `migrations`).
//! - `dir/`: that folder and everything in it (anywhere when it has no other `/`).
//! - `a/b`, `/a`: from the repository's top (a leading `/` only anchors).
//! - `*` any characters but `/`, `?` one of them, `[abc]` / `[a-z]` / `[!x]` one of a set,
//!   `**` any number of folders.
//!
//! Matched without regard to case (`.ENV` is fenced by `.env`): a fence errs on the side of
//! catching. `!negations` aren't taken (see [`Pattern::check`]).

/// A pattern made ready to match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// As it was written, to show and to write into the agents' configs.
    pub text: String,
    /// Its parts between `/`.
    parts: Vec<String>,
    /// Matched from the top only (else at any depth).
    anchored: bool,
}

impl Pattern {
    /// `None` for an empty pattern or a comment.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.starts_with('#') {
            return None;
        }
        let normal = text.to_lowercase();
        let body = normal.trim_end_matches('/');
        let anchored = body.starts_with('/') || body.trim_start_matches('/').contains('/');
        let parts: Vec<String> = body
            .trim_start_matches('/')
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .map(str::to_owned)
            .collect();
        (!parts.is_empty()).then(|| Pattern {
            text: text.to_owned(),
            parts,
            anchored,
        })
    }

    /// Why `text` can't be a fence entry, if it can't: a negation (`!x`) would mean
    /// different things to the agents' tools, and gitcraft takes none.
    pub fn check(text: &str) -> Option<String> {
        let t = text.trim();
        t.starts_with('!')
            .then(|| format!("{t}: a fence takes no exceptions (`!`); list what is fenced instead"))
    }

    /// The pathspecs git lists what this entry fences with: the path and everything in it,
    /// at any depth unless anchored.
    pub fn pathspecs(&self) -> [String; 2] {
        let body = self.parts.join("/");
        let body = if self.anchored {
            body
        } else {
            format!("**/{body}")
        };
        [
            format!(":(glob,icase){body}"),
            format!(":(glob,icase){body}/**"),
        ]
    }

    /// Matched from the repository's top only.
    pub fn anchored(&self) -> bool {
        self.anchored
    }

    /// Whether `path` (relative to the repository's top, `/` between names) is inside the
    /// fence: the path itself, or a folder it is in, matches.
    pub fn matches(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        let names: Vec<&str> = lower.split('/').filter(|n| !n.is_empty()).collect();
        if self.anchored {
            return (1..=names.len()).any(|n| parts_match(&self.parts, &names[..n]));
        }
        // Anywhere: any run of names, starting at any depth, ending at any depth.
        (0..names.len()).any(|start| {
            (start + 1..=names.len()).any(|end| parts_match(&self.parts, &names[start..end]))
        })
    }
}

/// Whether the pattern's `parts` match exactly the `names` (with `**` for any run of them).
fn parts_match(parts: &[String], names: &[&str]) -> bool {
    match parts.split_first() {
        None => names.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=names.len()).any(|skip| parts_match(rest, &names[skip..]))
        }
        Some((first, rest)) => names
            .split_first()
            .is_some_and(|(name, more)| glob(first, name) && parts_match(rest, more)),
    }
}

/// One token of a name's pattern.
enum Token {
    Star,
    Any,
    Char(char),
    /// `[…]`: the ranges in it, and whether it is negated (`[!…]`).
    Set(Vec<(char, char)>, bool),
}

fn tokens(pattern: &str) -> Vec<Token> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => out.push(Token::Star),
            '?' => out.push(Token::Any),
            '[' if chars[i + 1..].contains(&']') => {
                let mut j = i + 1;
                let negated = matches!(chars.get(j), Some('!' | '^'));
                if negated {
                    j += 1;
                }
                let mut ranges = Vec::new();
                // A `]` first is part of the set.
                let mut first = true;
                while j < chars.len() && (chars[j] != ']' || first) {
                    first = false;
                    let from = chars[j];
                    if chars.get(j + 1) == Some(&'-') && chars.get(j + 2).is_some_and(|c| *c != ']')
                    {
                        ranges.push((from, chars[j + 2]));
                        j += 3;
                    } else {
                        ranges.push((from, from));
                        j += 1;
                    }
                }
                out.push(Token::Set(ranges, negated));
                i = j;
            }
            c => out.push(Token::Char(c)),
        }
        i += 1;
    }
    out
}

impl Token {
    fn takes(&self, c: char) -> bool {
        match self {
            Token::Star => false,
            Token::Any => true,
            Token::Char(x) => *x == c,
            Token::Set(ranges, negated) => {
                ranges.iter().any(|(a, b)| (*a..=*b).contains(&c)) != *negated
            }
        }
    }
}

/// `*`, `?` and `[…]` within one name.
fn glob(pattern: &str, name: &str) -> bool {
    let p = tokens(pattern);
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        match p.get(pi) {
            Some(Token::Star) => {
                star = Some((pi, ni));
                pi += 1;
            }
            Some(t) if t.takes(n[ni]) => {
                pi += 1;
                ni += 1;
            }
            _ => match star {
                Some((sp, sn)) => {
                    pi = sp + 1;
                    ni = sn + 1;
                    star = Some((sp, sn + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|t| matches!(t, Token::Star))
}

/// Every pattern of `list` (one per entry), empty ones, comments and negations left out.
pub fn parse_list(list: &[String]) -> Vec<Pattern> {
    list.iter()
        .filter(|l| Pattern::check(l).is_none())
        .filter_map(|l| Pattern::parse(l))
        .collect()
}

/// The first pattern `path` is inside of.
pub fn fenced<'a>(patterns: &'a [Pattern], path: &str) -> Option<&'a Pattern> {
    patterns.iter().find(|p| p.matches(path))
}
