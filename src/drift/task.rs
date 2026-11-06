//! The task a worktree was made for: what was asked (kept beside the worktree's own git data,
//! so it goes with the worktree and is never committed), or its linked issue, and the words of
//! it that folders are matched with.

use std::path::{Path, PathBuf};

/// The file the task is kept in, in the worktree's own git folder.
const FILE: &str = "gitcraft-task.md";

/// Words too common to tell anything about a folder.
const COMMON: [&str; 40] = [
    "the", "and", "for", "with", "that", "this", "from", "into", "when", "then", "than", "also",
    "should", "would", "could", "make", "made", "add", "fix", "fixes", "use", "uses", "using",
    "not", "but", "all", "any", "are", "was", "has", "have", "its", "our", "you", "your", "please",
    "issue", "bug", "feat", "src",
];

/// The task's words that say what it is about: lower case, three letters or more, the common
/// ones left out, a plural's or a tense's ending cut (`logins` → `login`).
pub fn keywords(task: &str) -> Vec<String> {
    let mut out: Vec<String> = task
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !w.chars().all(|c| c.is_ascii_digit()))
        .filter(|w| !COMMON.contains(w))
        .map(stem)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `w` without a plural's or tense's ending.
pub fn stem(w: &str) -> String {
    for end in ["ing", "ies", "ed", "es", "s"] {
        if let Some(rest) = w.strip_suffix(end)
            && rest.chars().count() >= 3
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

/// Whether the folder `dir` (`/` between names) is one the task talks about: one of its names,
/// or a part of one (`user-login` → `user`, `login`), is a keyword, or a keyword starts it.
pub fn related(dir: &str, keywords: &[String]) -> bool {
    dir.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| p.chars().count() >= 3)
        .map(stem)
        .any(|part| {
            keywords
                .iter()
                .any(|k| *k == part || (k.len() >= 4 && part.starts_with(k.as_str())))
        })
}

/// The number of the issue a branch (or the task) names: `fix/123-login`, `issue-123`,
/// `gh-123`, `issues/123`, `#123`.
pub fn issue_number(branch: &str, task: &str) -> Option<u64> {
    let last = branch.rsplit('/').next().unwrap_or(branch).to_lowercase();
    let leading: String = last.chars().take_while(char::is_ascii_digit).collect();
    let named =
        !leading.is_empty() && (leading == last || last[leading.len()..].starts_with(['-', '_']));
    if named && let Ok(n) = leading.parse() {
        return Some(n);
    }
    let lower = branch.to_lowercase();
    for mark in ["issue-", "issues/", "issue/", "gh-", "#"] {
        if let Some(at) = lower.find(mark) {
            let digits: String = lower[at + mark.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(n) = digits.parse() {
                return Some(n);
            }
        }
    }
    task.split('#').skip(1).find_map(|rest| {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
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

/// The linked issue's title and text, through the forge's tool (`gh`, `glab`).
pub fn issue_text(forge: crate::forge::Forge, repo: &Path, number: u64) -> Result<String, String> {
    let n = number.to_string();
    let out = match forge {
        crate::forge::Forge::GitHub => {
            crate::forge::tool(forge, repo, &["issue", "view", &n, "--json", "title,body"])?
        }
        crate::forge::Forge::GitLab => {
            crate::forge::tool(forge, repo, &["issue", "view", &n, "--output", "json"])?
        }
        other => return Err(format!("Issues of {} aren't read yet", other.name())),
    };
    let v: serde_json::Value =
        serde_json::from_str(&out).map_err(|e| format!("Could not read issue #{n}: {e}"))?;
    let field = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let body = if v.get("body").is_some() {
        field("body")
    } else {
        field("description")
    };
    Ok(format!("{}\n\n{}", field("title"), body).trim().to_owned())
}
