//! A fence entry: a path pattern in the spirit of `.gitignore`.
//!
//! - `name` (no `/`): a file or folder of that name anywhere (`.env`, `migrations`).
//! - `dir/`: that folder and everything in it (anywhere when it has no other `/`).
//! - `a/b`, `/a`: from the repository's top (a leading `/` only anchors).
//! - `*` any characters but `/`, `?` one of them, `**` any number of folders.

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
        let normal = text.replace('\\', "/");
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

    /// Matched from the repository's top only.
    pub fn anchored(&self) -> bool {
        self.anchored
    }

    /// Whether `path` (relative to the repository's top, `/` between names) is inside the
    /// fence: the path itself, or a folder it is in, matches.
    pub fn matches(&self, path: &str) -> bool {
        let names: Vec<&str> = path.split('/').filter(|n| !n.is_empty()).collect();
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

/// `*` and `?` within one name.
fn glob(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi, ni));
                pi += 1;
            }
            Some(&c) if c == '?' || c == n[ni] => {
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
    p[pi..].iter().all(|c| *c == '*')
}

/// Every pattern of `list` (one per entry), empty ones and comments left out.
pub fn parse_list(list: &[String]) -> Vec<Pattern> {
    list.iter().filter_map(|l| Pattern::parse(l)).collect()
}

/// The first pattern `path` is inside of.
pub fn fenced<'a>(patterns: &'a [Pattern], path: &str) -> Option<&'a Pattern> {
    patterns.iter().find(|p| p.matches(path))
}
