//! The task a worktree was made for: what was asked (kept beside the worktree's own git data,
//! so it goes with the worktree and is never committed), or its linked issue, and the words of
//! it that folders are matched with.

use std::path::{Path, PathBuf};

/// The file the task is kept in, in the worktree's own git folder.
const FILE: &str = "gitcraft-task.md";

/// Words too common to tell anything about a folder, and those of an issue's template.
const COMMON: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "that",
    "this",
    "from",
    "into",
    "when",
    "then",
    "than",
    "also",
    "should",
    "would",
    "could",
    "make",
    "made",
    "add",
    "fix",
    "fixe",
    "use",
    "using",
    "not",
    "but",
    "all",
    "any",
    "are",
    "was",
    "has",
    "have",
    "its",
    "our",
    "you",
    "your",
    "please",
    "issue",
    "issu",
    "bug",
    "feat",
    "src",
    "see",
    "step",
    "reproduce",
    "reproduc",
    "expect",
    "expected",
    "actual",
    "behavior",
    "behaviour",
    "describe",
    "description",
    "version",
    "screenshot",
    "context",
    "additional",
    "github",
    "com",
    "http",
    "https",
    "www",
    "todo",
];

/// The task's words that say what it is about: lower case, three letters or more, links and
/// the common words left out, a plural's or a tense's ending cut (`logins` → `login`).
pub fn keywords(task: &str) -> Vec<String> {
    let lower = task.to_lowercase();
    let text: Vec<&str> = lower
        .split_whitespace()
        .filter(|w| !w.contains("://") && !w.starts_with("www."))
        .collect();
    let mut out: Vec<String> = text
        .iter()
        .flat_map(|w| w.split(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().count() >= 3 && !w.chars().all(|c| c.is_ascii_digit()))
        .map(stem)
        .filter(|w| !COMMON.contains(&w.as_str()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `w` without a plural's or tense's ending: `logins` → `login`, `parsing` → `pars`, `deps` →
/// `dep`, `libraries` → `library`; never shorter than three letters (four for a tense).
pub fn stem(w: &str) -> String {
    for (end, keep) in [("ing", 4), ("ies", 3), ("ed", 4), ("es", 4), ("s", 3)] {
        if let Some(rest) = w.strip_suffix(end)
            && rest.chars().count() >= keep
            // `class`, `access`: no plural.
            && !(end == "s" && rest.ends_with('s'))
        {
            return if end == "ies" {
                format!("{rest}y")
            } else {
                rest.to_owned()
            };
        }
    }
    w.to_owned()
}

/// Whether the folder `dir` (`/` between names) is one the task talks about: a part of one of
/// its names (`user-login` → `user`, `login`) and a keyword start the same way, one of them
/// (three letters or more) the start of the other (`auth` / `authentication`, `parse` /
/// `parser`, `deps` / `dependencies`).
pub fn related(dir: &str, keywords: &[String]) -> bool {
    dir.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| p.chars().count() >= 3)
        .map(stem)
        .any(|part| {
            keywords.iter().any(|k| {
                // `auth` for `authentication`; `parser` for `pars(ing)`, two letters more
                // at most (`workflows` is no `work`).
                k.starts_with(part.as_str())
                    || (k.len() >= 4 && part.starts_with(k.as_str()) && part.len() <= k.len() + 2)
            })
        })
}

/// The number of the issue a branch (or the task) names: `fix/123-login`, `issue-123`,
/// `gh-123`, `issues/123`, or `fixes #123` in the task. A date or a stamp (`2024-11-06-x`,
/// `20261106-1530`) is no issue, nor a colour (`#333`) in the task's text.
pub fn issue_number(branch: &str, task: &str) -> Option<u64> {
    let last = branch.rsplit('/').next().unwrap_or(branch).to_lowercase();
    let leading: String = last.chars().take_while(char::is_ascii_digit).collect();
    let after = &last[leading.len()..];
    // `123-login`: a word after it; `123` alone.
    let named = !leading.is_empty()
        && leading.len() <= 6
        && (after.is_empty()
            || (after.starts_with(['-', '_'])
                && after[1..].chars().next().is_some_and(char::is_alphabetic)));
    if named && let Ok(n) = leading.parse() {
        return Some(n);
    }
    let lower = branch.to_lowercase();
    for mark in ["issue-", "issues/", "issue/", "gh-", "#"] {
        for (at, _) in lower.match_indices(mark) {
            let free = lower[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
            let digits: String = lower[at + mark.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if free && let Ok(n) = digits.parse() {
                return Some(n);
            }
        }
    }
    let words = task.to_lowercase();
    [
        "fixes #",
        "fix #",
        "closes #",
        "close #",
        "resolves #",
        "resolve #",
        "issue #",
        "issues #",
    ]
    .iter()
    .find_map(|mark| {
        let at = words.find(mark)?;
        let digits: String = words[at + mark.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    })
}

/// Where the task of the worktree at `worktree` is kept (its git folder, on its machine).
fn file(worktree: &Path) -> Result<PathBuf, String> {
    let dir = crate::git::run(Some(worktree), &["rev-parse", "--absolute-git-dir"])
        .map_err(|e| format!("Could not find the worktree's git folder: {e}"))?;
    let dir = dir.trim_end_matches(['\n', '\r']);
    Ok(crate::remote::join(
        &crate::remote::resolve(worktree, dir),
        FILE,
    ))
}

/// The task kept for the worktree (empty when none is).
pub fn read(worktree: &Path) -> Result<String, String> {
    match crate::remote::read_to_string(&file(worktree)?) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Could not read the worktree's task: {e}")),
    }
}

/// Keeps `task` for the worktree.
pub fn write(worktree: &Path, task: &str) -> Result<(), String> {
    crate::remote::write(&file(worktree)?, task.as_bytes())
        .map_err(|e| format!("Could not save the worktree's task: {e}"))
}

/// The linked issue's title and text, explicitly in the selected forge repository.
pub fn issue_text(
    hosting: &crate::forge::Hosting,
    repo: &Path,
    number: u64,
) -> Result<String, String> {
    hosting
        .forge
        .provider()
        .issue(repo, hosting, number)
        .map(|issue| {
            format!("{}\n\n{}", issue.title, issue.body)
                .trim()
                .to_owned()
        })
}
