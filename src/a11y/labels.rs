//! What screen readers call the painted widgets: plain text, the parts joined by commas, as a
//! row reads aloud ("commit a301da9, Merge…, Jonas, 26 minutes ago, branch dev").

/// The longest name read out, in characters: a commit message can be a whole essay.
pub const MAX_CHARS: usize = 300;

/// One line of text: runs of whitespace (newlines, tabs) become one space, the ends are
/// trimmed, and past [`MAX_CHARS`] it is cut with an ellipsis.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_CHARS * 4));
    let mut chars = 0;
    for word in text.split_whitespace() {
        if chars > 0 {
            out.push(' ');
            chars += 1;
        }
        for c in word.chars() {
            if chars >= MAX_CHARS {
                // Cut at the limit, not after a trailing space.
                let kept = out.trim_end().len();
                out.truncate(kept);
                out.push('…');
                return out;
            }
            out.push(c);
            chars += 1;
        }
    }
    out
}

/// The name a widget is given: one line, without a tooltip's shortcut.
pub fn name(text: &str) -> String {
    clean(without_keys(text))
}

/// The parts that say something, cleaned and joined with commas.
pub fn join<S: AsRef<str>>(parts: impl IntoIterator<Item = S>) -> String {
    parts
        .into_iter()
        .map(|p| clean(p.as_ref()))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `n` things: "1 file", "3 files".
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A commit of the graph: its short hash, summary, author, how long ago, and the branches and
/// tags on it (each already said as "branch dev", "tag v1").
pub fn commit(short: &str, summary: &str, author: &str, when: &str, refs: &[String]) -> String {
    let summary = if summary.trim().is_empty() {
        "no message"
    } else {
        summary
    };
    let head = format!("commit {}", short.trim());
    join(
        [head.as_str(), summary, author, when]
            .into_iter()
            .chain(refs.iter().map(String::as_str)),
    )
}

/// The changes row on top of the graph, with what it holds.
pub fn changes(conflicted: usize, staged: usize, unstaged: usize) -> String {
    let parts = [
        (conflicted, count(conflicted, "conflict", "conflicts")),
        (staged, format!("{staged} staged")),
        (unstaged, format!("{unstaged} unstaged")),
    ];
    join(
        std::iter::once("Uncommitted changes".to_owned())
            .chain(parts.into_iter().filter(|(n, _)| *n > 0).map(|(_, s)| s)),
    )
}

/// What git's status letter means.
pub fn file_status(code: char) -> &'static str {
    match code {
        'A' => "added",
        '?' => "untracked",
        'M' => "modified",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'T' => "type changed",
        'U' => "conflicted",
        _ => "",
    }
}

/// A file of a list of changes: its path, what happened to it, and where it came from.
pub fn file(path: &str, code: char, old_path: Option<&str>) -> String {
    let from = old_path.map(|old| format!("from {old}"));
    join([path, file_status(code), from.as_deref().unwrap_or("")])
}

/// A ref's name as the window shows it: `refs/heads/main` is `main`, `refs/remotes/origin/x`
/// is `origin/x`.
pub fn ref_name(full: &str) -> &str {
    ["refs/heads/", "refs/remotes/", "refs/tags/"]
        .iter()
        .find_map(|prefix| full.strip_prefix(prefix))
        .unwrap_or(full)
}

/// A branch of the sidebar: its name, whether it is checked out, how far it is from its
/// upstream.
pub fn branch(name: &str, current: bool, ahead: usize, behind: usize) -> String {
    let ahead = (ahead > 0).then(|| format!("{ahead} ahead"));
    let behind = (behind > 0).then(|| format!("{behind} behind"));
    join([
        format!("branch {name}").as_str(),
        if current { "checked out" } else { "" },
        ahead.as_deref().unwrap_or(""),
        behind.as_deref().unwrap_or(""),
    ])
}

/// `base`, then each note whose condition holds: "branch x, upstream gone, hidden from the
/// graph".
pub fn with_notes(base: &str, notes: &[(bool, &str)]) -> String {
    join(std::iter::once(base).chain(notes.iter().filter(|n| n.0).map(|n| n.1)))
}

/// A header that folds what is under it: its name, how much it holds, open or closed.
pub fn section(name: &str, count: &str, open: bool) -> String {
    join([name, count, if open { "open" } else { "closed" }])
}

/// A worktree: where it is, its branch (or a detached HEAD), its uncommitted changes, and the
/// notes that hold ("open here", "locked").
pub fn worktree(
    path: &str,
    branch: Option<&str>,
    changes: Option<usize>,
    notes: &[(bool, &str)],
) -> String {
    let changes = changes
        .filter(|n| *n > 0)
        .map(|n| count(n, "change", "changes"));
    let base = join([
        format!("worktree {path}").as_str(),
        branch.unwrap_or("detached HEAD"),
        changes.as_deref().unwrap_or(""),
    ]);
    with_notes(&base, notes)
}

/// A pull request of the sidebar.
pub fn pull_request(number: u64, title: &str, author: &str, branch: &str, draft: bool) -> String {
    let number = format!("pull request {number}");
    with_notes(
        &join([number.as_str(), title, author, branch]),
        &[(draft, "draft")],
    )
}

/// A submodule: its path, its commit (or `git describe` of it), whether it has changes.
pub fn submodule(path: &str, commit: &str, modified: bool) -> String {
    let base = join([format!("submodule {path}").as_str(), commit]);
    with_notes(&base, &[(modified, "modified")])
}

/// A stash: `stash@{2}` is said "stash 2", then its message.
pub fn stash(name: &str, message: &str) -> String {
    let number = name
        .strip_prefix("stash@{")
        .and_then(|rest| rest.strip_suffix('}'))
        .map(|n| format!("stash {n}"));
    join([number.as_deref().unwrap_or(name), message])
}

/// A name without the shortcut a tooltip ends with: "Reload (F5)" is "Reload". The keys are in
/// the manual, and a name says what a widget is, not how a mouse user reaches it.
pub fn without_keys(text: &str) -> &str {
    let trimmed = text.trim_end();
    let Some(open) = trimmed.rfind(" (") else {
        return text;
    };
    let Some(inside) = trimmed[open + 2..].strip_suffix(')') else {
        return text;
    };
    let is_key = |part: &str| {
        let part = part.trim();
        ["Ctrl", "Cmd", "Alt", "Shift", "Esc", "Enter"]
            .iter()
            .any(|k| part.starts_with(k))
            || (part.len() >= 2
                && part.starts_with('F')
                && part[1..].chars().all(|c| c.is_ascii_digit()))
    };
    if !inside.is_empty() && inside.split(" / ").all(is_key) {
        &trimmed[..open]
    } else {
        text
    }
}
