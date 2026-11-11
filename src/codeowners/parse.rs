//! Reading a CODEOWNERS file into rules, the way its forge does, and noting the lines it would
//! skip or read otherwise than they look.

use super::Flavor;
use super::glob::Pattern;

/// A line with a pattern.
#[derive(Clone, Debug)]
pub(super) struct Rule {
    /// Its line in the file, from 1.
    pub line: usize,
    /// The pattern as written (with its `!`).
    pub pattern: String,
    /// `!pattern`: GitLab's exclusion, Gitea's "every file but these".
    pub negated: bool,
    pub owners: Vec<String>,
    /// The GitLab section it is in (0: before any).
    pub section: usize,
    matcher: Matcher,
}

#[derive(Clone, Debug)]
enum Matcher {
    Glob(Pattern),
    Regex(regex::Regex),
}

impl Rule {
    /// Whether its pattern matches the file at `path` (whatever the `!`).
    pub(super) fn matches(&self, path: &str) -> bool {
        match &self.matcher {
            Matcher::Glob(p) => p.matches(path),
            Matcher::Regex(r) => r.is_match(path),
        }
    }
}

/// A GitLab section: `[Name]`, `^[Optional]`, `[Name][2]`, with owners for its lines that name
/// none.
#[derive(Clone, Debug, Default)]
pub(super) struct Section {
    pub name: String,
    pub optional: bool,
    pub approvals: u32,
    pub defaults: Vec<String>,
}

/// A line the forge skips or reads otherwise than it looks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Problem {
    pub line: usize,
    pub text: String,
}

pub(super) struct Parsed {
    pub rules: Vec<Rule>,
    pub sections: Vec<Section>,
    pub problems: Vec<Problem>,
}

/// Whether `text` has GitLab's section headers.
pub(super) fn has_sections(text: &str) -> bool {
    text.lines()
        .any(|line| section_header(line.trim()).is_some())
}

pub(super) fn parse(flavor: Flavor, text: &str) -> Parsed {
    let mut parsed = Parsed {
        rules: Vec::new(),
        sections: vec![Section::default()],
        problems: Vec::new(),
    };
    // The section lines go in (GitLab merges headers of the same name), and the owners its
    // header gives lines that name none.
    let mut section = 0;
    let mut defaults: Vec<String> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if flavor == Flavor::GitLab
            && let Some(header) = section_header(trimmed)
        {
            match header {
                Ok(header) => {
                    check_owners(&header.defaults, flavor, line, &mut parsed.problems);
                    defaults = header.defaults.clone();
                    section = match parsed
                        .sections
                        .iter()
                        .position(|s| s.name.eq_ignore_ascii_case(&header.name))
                    {
                        Some(i) => i,
                        None => {
                            parsed.sections.push(header);
                            parsed.sections.len() - 1
                        }
                    };
                }
                Err(why) => parsed.problems.push(Problem { line, text: why }),
            }
            continue;
        }
        let tokens = match flavor {
            Flavor::Gitea => tokenize_gitea(trimmed),
            Flavor::GitLab => tokenize_with(trimmed, false),
            Flavor::GitHub => tokenize(trimmed),
        };
        let Some((pattern, rest)) = tokens.split_first() else {
            continue;
        };
        let owners = owners_of_line(flavor, pattern, rest, line, &mut parsed.problems);
        if flavor == Flavor::GitHub && pattern.starts_with("\\#") {
            parsed.problems.push(Problem {
                line,
                text: format!("GitHub doesn't take \\# escapes: {pattern} may match nothing"),
            });
        }
        if flavor == Flavor::Gitea && owners.is_empty() {
            parsed.problems.push(Problem {
                line,
                text: format!("{pattern} has no owners: Gitea / Forgejo skip the line"),
            });
            continue;
        }
        if let Some(mut rule) = rule(flavor, line, pattern, owners, section, &mut parsed.problems) {
            if flavor == Flavor::GitLab && rule.owners.is_empty() && !rule.negated {
                if defaults.is_empty() {
                    parsed.problems.push(Problem {
                        line,
                        text: format!("{pattern} has no owners, and its section has none to give"),
                    });
                }
                rule.owners = defaults.clone();
            }
            check_owners(&rule.owners, flavor, line, &mut parsed.problems);
            parsed.rules.push(rule);
        }
    }
    parsed
}

/// The owners on a line after its pattern, as the forge takes them. GitLab reads the whole rest
/// of the line for `@names` and emails (a `#` there starts no comment); Gitea and Forgejo take
/// a name with or without its `@`.
fn owners_of_line(
    flavor: Flavor,
    pattern: &str,
    rest: &[String],
    line: usize,
    problems: &mut Vec<Problem>,
) -> Vec<String> {
    match flavor {
        Flavor::GitLab => {
            if rest.iter().any(|w| w.starts_with('#')) {
                problems.push(Problem {
                    line,
                    text: format!(
                        "{pattern}: GitLab has no comments after a pattern, and reads any \
                         @name in them as an owner"
                    ),
                });
            }
            rest.iter()
                .filter(|w| w.starts_with('@') || is_email(w))
                .cloned()
                .collect()
        }
        Flavor::Gitea => rest
            .iter()
            .map(|w| {
                if w.starts_with('@') {
                    w.clone()
                } else {
                    format!("@{w}")
                }
            })
            .collect(),
        Flavor::GitHub => rest.to_vec(),
    }
}

/// The rule of one line, or why it has none (noted in `problems`).
fn rule(
    flavor: Flavor,
    line: usize,
    pattern: &str,
    owners: Vec<String>,
    section: usize,
    problems: &mut Vec<Problem>,
) -> Option<Rule> {
    let negated = pattern.starts_with('!');
    let bare = pattern.strip_prefix('!').unwrap_or(pattern);
    let matcher = match flavor {
        // As Gitea and Forgejo compile it: `^pattern$`, no group, so that `a|b` is `^a` or `b$`.
        Flavor::Gitea => match regex::Regex::new(&format!("^{bare}$")) {
            Ok(re) => Matcher::Regex(re),
            Err(e) => {
                problems.push(Problem {
                    line,
                    text: format!(
                        "{pattern} isn't a regular expression Gitea / Forgejo can read: {}",
                        e.to_string().lines().last().unwrap_or_default().trim()
                    ),
                });
                return None;
            }
        },
        Flavor::GitHub if negated => {
            problems.push(Problem {
                line,
                text: format!("GitHub doesn't take ! patterns: it skips {pattern}"),
            });
            return None;
        }
        Flavor::GitHub | Flavor::GitLab => {
            if flavor == Flavor::GitHub && bare.contains('[') && !bare.contains("\\[") {
                problems.push(Problem {
                    line,
                    text: format!("GitHub doesn't take [ ] ranges: {pattern} may match nothing"),
                });
            }
            Matcher::Glob(Pattern::new(bare, flavor == Flavor::GitLab))
        }
    };
    if negated && flavor == Flavor::GitLab && !owners.is_empty() {
        problems.push(Problem {
            line,
            text: format!("{pattern} takes files out of its section: its owners are not used"),
        });
    }
    Some(Rule {
        line,
        pattern: pattern.to_owned(),
        negated,
        owners,
        section,
        matcher,
    })
}

/// Notes owners the forge won't take: `@` forms it doesn't know, emails on Gitea.
fn check_owners(owners: &[String], flavor: Flavor, line: usize, problems: &mut Vec<Problem>) {
    for owner in owners {
        let ok = match flavor {
            // An account or `org/team`, by name: an email is no owner there.
            Flavor::Gitea => owner.len() > 1 && !owner[1..].contains('@'),
            Flavor::GitHub => {
                (owner.starts_with('@') && owner.len() > 1 && !owner.starts_with("@@"))
                    || is_email(owner)
            }
            Flavor::GitLab => (owner.starts_with('@') && owner.len() > 1) || is_email(owner),
        };
        if !ok {
            problems.push(Problem {
                line,
                text: format!(
                    "{owner} isn't an owner {} knows (@user, @org/team{})",
                    flavor.name(),
                    if flavor == Flavor::Gitea {
                        ""
                    } else {
                        " or an email"
                    }
                ),
            });
        }
    }
}

fn is_email(text: &str) -> bool {
    text.split_once('@')
        .is_some_and(|(user, host)| !user.is_empty() && host.contains('.'))
}

/// The words of a line: split on unescaped whitespace, up to a `#` that starts a word (a
/// comment). `\ ` and `\#` stay in the word, as written (the pattern reads the `\` itself).
pub(super) fn tokenize(line: &str) -> Vec<String> {
    tokenize_with(line, true)
}

/// [`tokenize`], where a `#` after the first word starts a comment only when `comments`
/// (GitLab reads the rest of the line for owners, `#` and all).
fn tokenize_with(line: &str, comments: bool) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                word.push('\\');
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            '#' if word.is_empty() && (comments || words.is_empty()) => break,
            c if c.is_whitespace() => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The words of a line as Gitea and Forgejo split it: a `\\` takes the next character as it is
/// (and is dropped, so `\\.` is a `.`), a `#` anywhere starts a comment.
pub(super) fn tokenize_gitea(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            '#' => break,
            c if c.is_whitespace() => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// GitLab's section header `[Name]`, `^[Name]`, `[Name][2]`, followed by default owners; `Err`
/// for one GitLab wouldn't read. `None` when the line isn't one.
fn section_header(line: &str) -> Option<Result<Section, String>> {
    let (optional, rest) = match line.strip_prefix('^') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let rest = rest.strip_prefix('[')?;
    let Some(end) = rest.find(']') else {
        return Some(Err(format!("{line}: the section's name has no closing ]")));
    };
    let name = rest[..end].trim().to_owned();
    if name.is_empty() {
        return Some(Err(format!("{line}: a section needs a name")));
    }
    let mut after = &rest[end + 1..];
    let mut approvals = 1;
    if let Some(count) = after.strip_prefix('[') {
        let Some(close) = count.find(']') else {
            return Some(Err(format!("{line}: the approvals count has no closing ]")));
        };
        match count[..close].trim().parse() {
            Ok(_) if optional => {
                return Some(Err(format!(
                    "{line}: an optional section (^) can't ask for approvals"
                )));
            }
            Ok(n) => approvals = n,
            Err(_) => return Some(Err(format!("{line}: the approvals count isn't a number"))),
        }
        after = &count[close + 1..];
    }
    Some(Ok(Section {
        name,
        optional,
        approvals,
        defaults: tokenize(after),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_on_unescaped_spaces_and_stop_at_a_comment() {
        assert_eq!(
            tokenize("docs/my\\ file.md @ann  @acme/docs # who"),
            ["docs/my\\ file.md", "@ann", "@acme/docs"]
        );
        assert_eq!(tokenize("\\#notes @ann"), ["\\#notes", "@ann"]);
        assert_eq!(tokenize("a#b @ann"), ["a#b", "@ann"]);
        assert!(tokenize("# only a comment").is_empty());
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn gitea_words_drop_their_escapes_and_stop_at_any_hash() {
        assert_eq!(tokenize_gitea(".*\\.go @a#b @c"), [".*.go", "@a"]);
        assert_eq!(tokenize_gitea("a\\ b @x"), ["a b", "@x"]);
    }

    #[test]
    fn section_headers() {
        let s = section_header("[Docs] @ann @acme/writers");
        assert!(matches!(&s, Some(Ok(s)) if s.name == "Docs" && s.defaults.len() == 2));
        let s = section_header("^[Optional]");
        assert!(matches!(&s, Some(Ok(s)) if s.optional));
        let s = section_header("[Two][2]");
        assert!(matches!(&s, Some(Ok(s)) if s.approvals == 2));
        assert!(matches!(section_header("^[Optional][2]"), Some(Err(_))));
        assert!(matches!(section_header("[Broken"), Some(Err(_))));
        assert!(matches!(section_header("[]"), Some(Err(_))));
        assert!(matches!(section_header("[A][x]"), Some(Err(_))));
        assert!(section_header("/path @a").is_none());
    }

    #[test]
    fn owners_the_forge_wont_take_are_noted() {
        let parsed = parse(Flavor::GitHub, "*.rs ann @ok me@example.org\n");
        assert_eq!(parsed.problems.len(), 1);
        assert!(parsed.problems[0].text.contains("ann"));
        // Gitea takes a name without its `@` as an owner.
        let parsed = parse(Flavor::Gitea, "a ann\n");
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        assert_eq!(parsed.rules[0].owners, ["@ann"]);
    }
}
