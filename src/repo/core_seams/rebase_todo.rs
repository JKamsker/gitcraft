use super::*;

/// A new folder under `parent` that only this user can enter. Never one that already exists:
/// in a shared temporary folder, another user could have made it to swap the todo list (whose
/// `exec` lines git runs) for their own.
pub(super) fn private_dir(parent: &Path) -> std::io::Result<PathBuf> {
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let base = format!(
        "gitcraft-rebase-{}-{}",
        std::process::id(),
        crate::git::now()
    );
    let mut n = 0u32;
    loop {
        let dir = parent.join(if n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        });
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 100 => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// git's todo list for `entries` (oldest first). `message_file(i)` is the quoted path of entry
/// `i`'s new message when it was reworded.
/// `refs`: (commit, `refs/heads/<branch>`) for the branches moved along (`--update-refs`), each
/// placed after its commit, as git places them.
pub(super) fn todo(
    entries: &[Entry],
    message_file: impl Fn(usize) -> Option<String>,
    refs: &[(String, String)],
) -> String {
    let mut out = String::new();
    // A branch's update-ref waits for the squashes and fixups melded into its commit: placed
    // before them, the branch would be left on the commit as it was before they were.
    let mut waiting: Vec<&str> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if !matches!(e.step, Step::Squash | Step::Fixup) {
            for name in waiting.drain(..) {
                out.push_str(&format!("update-ref {name}\n"));
            }
        }
        let verb = match e.step {
            Step::Pick | Step::Reword => "pick",
            Step::Squash => "squash",
            Step::Fixup => "fixup",
            Step::Drop => "drop",
            Step::Edit => "edit",
        };
        out.push_str(&format!("{verb} {} {}\n", e.commit.hash, e.commit.summary));
        if let Some(file) = message_file(i) {
            out.push_str(&format!(
                "exec git commit --amend --allow-empty --cleanup=whitespace -F {file}\n"
            ));
        }
        waiting.extend(
            refs.iter()
                .filter(|(at, _)| *at == e.commit.hash)
                .map(|(_, name)| name.as_str()),
        );
    }
    for name in waiting {
        out.push_str(&format!("update-ref {name}\n"));
    }
    out
}

/// The branches on `commits` that Update dependent branches moves along: (commit,
/// `refs/heads/<branch>`). Not those checked out in a worktree (this one's included), which
/// git refuses to move under them.
pub(super) fn dependent_refs(
    repo: &Path,
    commits: &[&str],
) -> Result<Vec<(String, String)>, String> {
    let out = crate::git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/heads",
        ],
    )?;
    let checked_out: Vec<String> = crate::git::worktree_list(repo, None)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|w| w.branch)
        .map(|b| format!("refs/heads/{b}"))
        .collect();
    Ok(out
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(at, name)| commits.contains(at) && !checked_out.iter().any(|c| c == name))
        .map(|(at, name)| (at.to_owned(), name.to_owned()))
        .collect())
}

/// `path` for git's shell (`sh`, also on Windows): forward slashes, single-quoted.
pub(super) fn sh_path(path: &Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    format!("'{}'", p.replace('\'', r"'\''"))
}

/// How the rows are moved and their actions set, for the footer's tooltip.
pub(super) fn keys_hint(ctx: &egui::Context) -> String {
    use crate::keymap::{self, Command};
    let steps = Step::ALL
        .map(|step| format!("{}: {}", step.label(), keymap::label(ctx, step.command())))
        .join("; ");
    crate::i18n::message(
        "repo-core-3235",
        &[
            (
                "RebaseMoveUp",
                keymap::label(ctx, Command::RebaseMoveUp).to_string().into(),
            ),
            (
                "RebaseMoveDown",
                keymap::label(ctx, Command::RebaseMoveDown)
                    .to_string()
                    .into(),
            ),
            (
                "RebasePrevious",
                keymap::label(ctx, Command::RebasePrevious)
                    .to_string()
                    .into(),
            ),
            (
                "RebaseNext",
                keymap::label(ctx, Command::RebaseNext).to_string().into(),
            ),
            ("steps", steps.to_string().into()),
        ],
    )
}

pub(super) fn short(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}
