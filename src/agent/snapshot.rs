//! A worktree's files as they are now, untracked ones git doesn't ignore included, written as a
//! tree through an index of gitcraft's own: the worktree's own index, its files and whatever
//! runs in it are left alone. Used by the bake-off and by agent checkpoints.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::git;

/// The tree of the worktree at `wt` as it is now.
pub(crate) fn take(wt: &Path) -> Result<String, String> {
    let index = temporary_index(wt, "snapshot")?;
    let own = git_path(wt, "index")?;
    // Its own index as a start: what git knows of the files (no file read again when it didn't
    // change) and of a sparse checkout (files left out aren't deleted ones).
    let copied = copy(wt, &own, &index);
    let result = write(wt, &index, copied);
    remove(wt, &index);
    result
}

fn git_path(wt: &Path, what: &str) -> Result<String, String> {
    git::run(Some(wt), &["rev-parse", "--git-path", what])
        .map(|p| p.trim_end_matches(['\n', '\r']).to_owned())
}

/// A path for an index of gitcraft's own in `wt`'s git folder (as git prints it), one per
/// call: two at once (two tabs, a hook and a bake-off) never share one. Removed with
/// [`remove`].
pub(crate) fn temporary_index(wt: &Path, what: &str) -> Result<String, String> {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let name = format!(
        "gitcraft-{what}-{}-{}.index",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    git_path(wt, &name)
}

fn write(wt: &Path, index: &str, copied: bool) -> Result<String, String> {
    let env = [("GIT_INDEX_FILE", index)];
    let step = |args: &[&str]| git::run_env(Some(wt), args, &env);
    if !copied && git::run(Some(wt), &["rev-parse", "--verify", "-q", "HEAD"]).is_ok() {
        step(&["read-tree", "HEAD"])?;
    }
    step(&["add", "-A", "--", "."])?;
    Ok(step(&["write-tree"])?.trim().to_owned())
}

/// Copies the index at `from` to `to` (paths as git printed them, relative to `wt`): whether it
/// was there to copy.
fn copy(wt: &Path, from: &str, to: &str) -> bool {
    match crate::remote::parse(wt) {
        Some(target) => there(
            &target,
            &format!(
                "cp -f -- {} {}",
                crate::remote::sh_quote(from),
                crate::remote::sh_quote(to)
            ),
        ),
        None => std::fs::copy(wt.join(from), wt.join(to)).is_ok(),
    }
}

/// Removes the index at `index` (as git printed it for `wt`), on whichever machine it is.
pub(crate) fn remove(wt: &Path, index: &str) {
    match crate::remote::parse(wt) {
        Some(target) => {
            there(
                &target,
                &format!("rm -f -- {}", crate::remote::sh_quote(index)),
            );
        }
        None => {
            let _ = std::fs::remove_file(wt.join(index));
        }
    }
}

/// Runs `script` in the folder of `target`, on its machine: whether it went through.
fn there(target: &crate::remote::Target, script: &str) -> bool {
    let req = crate::remote::Request {
        cwd: Some(target.path.clone()),
        script: script.to_owned(),
        ..crate::remote::Request::default()
    };
    crate::remote::exec(&target.host, &req).is_ok_and(|out| out.code == 0)
}

#[cfg(test)]
mod tests {
    use super::take;
    use crate::git;

    #[test]
    fn a_snapshot_keeps_sparse_files_and_leaves_nothing_behind() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-snapshot-{} ñ", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("out")).map_err(|e| e.to_string())?;
        let g = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            g(args)?;
        }
        // An empty repository: no HEAD yet.
        std::fs::write(dir.join("a.txt"), "a\n").map_err(|e| e.to_string())?;
        let first = take(&dir)?;
        assert_eq!(g(&["ls-tree", "--name-only", &first])?, "a.txt\n");
        std::fs::write(dir.join("out/b.txt"), "b\n").map_err(|e| e.to_string())?;
        g(&["add", "-A"])?;
        g(&["commit", "-qm", "one"])?;
        // A file left out of the checkout, as a sparse checkout does: still in the snapshot.
        g(&["update-index", "--skip-worktree", "out/b.txt"])?;
        std::fs::remove_file(dir.join("out/b.txt")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("new ü.txt"), "n\n").map_err(|e| e.to_string())?;
        let status = g(&["status", "--porcelain"])?;
        let tree = take(&dir)?;
        let names = g(&["ls-tree", "-r", "--name-only", &tree])?;
        assert!(
            names.contains("out/b.txt") && names.contains("new"),
            "{names}"
        );
        assert_eq!(
            g(&["status", "--porcelain"])?,
            status,
            "the worktree's own index is as it was"
        );
        let left: Vec<_> = std::fs::read_dir(dir.join(".git"))
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("gitcraft-snapshot")
            })
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(left.is_empty(), "no index left behind");
        Ok(())
    }
}
