//! Whether an attempt builds: a command run in its worktree, in a terminal where its output
//! shows, its exit status left in a file of the repository's git folder for the grid to read.

use std::path::{Path, PathBuf};

use crate::agent::sh;

/// The build command last used, kept for the repository.
pub(crate) const COMMAND_FILE: &str = "bakeoff-build.json";

/// How a build of an attempt went, as far as is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Built {
    Preparing,
    Running,
    Unavailable,
    Passed,
    Failed(i32),
}

/// The file the build of the worktree `worktree` leaves its exit status in (for `repo`'s
/// repository): one per worktree folder.
pub(crate) fn status_file(repo: &Path, worktree: &Path) -> Result<PathBuf, String> {
    let name: String = crate::remote::git_path(worktree)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let digest = format!("{:x}", md5::compute(crate::remote::git_path(worktree)));
    let short: String = name
        .chars()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    crate::agent::repo_file(repo, &format!("bakeoff/{short}-{}.status", &digest[..8]))
}

/// `status` as `sh` of its machine names it: a path there; here, with `/` (Git's `sh` on
/// Windows takes `C:/x/y`).
pub(crate) fn for_sh(status: &Path) -> String {
    let there = crate::remote::git_path(status);
    if crate::remote::is_remote(status) {
        there
    } else {
        there.replace('\\', "/")
    }
}

/// What runs `command` with `sh -e` in the worktree and writes its exit status to `status`
/// (a path of the repository's machine, as git there names it).
pub(crate) fn script(command: &str, status: &str) -> String {
    let dir = crate::remote::parent_there(status);
    let mut s = sh::assign_verbatim("gitcraft_build", command.trim_end());
    s.push_str(&format!(
        "mkdir -p -- {dir} && rm -f -- {status}\n\
         gitcraft_say 'building (bake-off):'\n\
         printf '%s\\n' \"$gitcraft_build\" | sed 's/^/  $ /'\n\
         sh -e -c \"$gitcraft_build\" > {log} 2>&1\n\
         gitcraft_code=$?\n\
         cat -- {log}\n\
         printf '%s\\n' \"$gitcraft_code\" > {status}\n\
         if [ \"$gitcraft_code\" = 0 ]; then gitcraft_say 'it builds'; \
         else gitcraft_say \"the build failed (exit status $gitcraft_code)\"; fi",
        dir = sh::quote(dir),
        status = sh::quote(status),
        log = sh::quote(&format!("{status}.log")),
    ));
    s
}

/// Parse a terminal receipt; absence or malformed bytes alone say nothing about liveness.
pub(crate) fn read_status(text: Option<&str>) -> Option<Built> {
    match text.map(str::trim).map(str::parse::<i32>) {
        Some(Ok(0)) => Some(Built::Passed),
        Some(Ok(code)) => Some(Built::Failed(code)),
        _ => None,
    }
}

/// Absence is a running result only while this specific launch still has an
/// observed terminal/output lifetime. Unknown never asserts process completion.
pub(crate) fn observed_status(text: Option<&str>, launch_open: bool) -> Built {
    if let Some(status) = read_status(text) {
        return status;
    }
    if launch_open && text.is_none_or(|text| text.trim().is_empty()) {
        Built::Running
    } else {
        Built::Unavailable
    }
}
/// Build an immutable compared tree, including uncommitted worktree attempts. No source
/// checkout/index/ref is moved. The workspace remains registered for inspection/removal
/// through Worktrees; automatic forced cleanup could delete a user's later edits.
pub(crate) fn workspace(repo: &Path, tree: &str, base: &str) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if ![tree, base]
        .iter()
        .all(|s| matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Build needs immutable compared object IDs".into());
    }
    let commit = crate::git::run(
        Some(repo),
        &[
            "-c",
            "commit.gpgSign=false",
            "-c",
            "user.name=gitcraft bake-off",
            "-c",
            "user.email=bakeoff@localhost",
            "commit-tree",
            tree,
            "-p",
            base,
            "-m",
            "Disposable bake-off snapshot",
        ],
    )?
    .trim()
    .to_owned();
    let path = crate::agent::repo_file(
        repo,
        &format!(
            "bakeoff/workspaces/{}-{}-{}",
            crate::git::now(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ),
    )?;
    // Git refuses an occupied directory. Do not remove or adopt it on failure.
    crate::git::run(
        Some(repo),
        &[
            "-c",
            "core.hooksPath=",
            "worktree",
            "add",
            "--detach",
            "--",
            &crate::remote::git_path(&path),
            &commit,
        ],
    )?;
    Ok(path)
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    #[test]
    fn snapshots_are_independent_and_source_index_and_branch_stay_unchanged() -> Result<(), String>
    {
        let r = crate::git::fixture::Repo::new("bakeoff-snapshot")?;
        let base = r.commit("base", &[("file", Some("base\n"))])?;
        r.git(&["branch", "attempt"])?;
        let tree = r.git(&["rev-parse", "HEAD^{tree}"])?;
        std::fs::write(r.path().join("later"), "keep").map_err(|e| e.to_string())?;
        r.git(&["add", "later"])?;
        let index = std::fs::read(r.path().join(".git/index")).map_err(|e| e.to_string())?;
        let a = workspace(r.path(), tree.trim(), &base)?;
        let b = workspace(r.path(), tree.trim(), &base)?;
        assert_ne!(a, b);
        assert_eq!(crate::git::run(Some(&a), &["show", "HEAD:file"])?, "base\n");
        assert!(!a.join("later").exists());
        assert_eq!(std::fs::read(r.path().join(".git/index")).unwrap(), index);
        assert_eq!(r.git(&["rev-parse", "attempt"])?.trim(), base);
        assert!(workspace(r.path(), "HEAD", &base).is_err());
        // These owned fixtures are clean: ordinary removal refuses dirty workspaces.
        r.git(&["worktree", "remove", &a.to_string_lossy()])?;
        r.git(&["worktree", "remove", &b.to_string_lossy()])?;
        Ok(())
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn pure_bakeoff_recorded_receipts_distinguish_running_terminal_and_unavailable_outcome() {
        for text in [None, Some(""), Some(" \n")] {
            assert_eq!(read_status(text), None);
            assert_eq!(observed_status(text, true), Built::Running);
            assert_eq!(observed_status(text, false), Built::Unavailable);
        }
        for open in [true, false] {
            assert_eq!(observed_status(Some("garbage"), open), Built::Unavailable);
            assert_eq!(observed_status(Some("0\n"), open), Built::Passed);
            assert_eq!(observed_status(Some("17\n"), open), Built::Failed(17));
        }
    }
}
