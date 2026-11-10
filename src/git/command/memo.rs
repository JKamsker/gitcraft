//! Answers that stay the same while gitcraft runs, kept for a minute: git's version and where it
//! is installed, and where a repository's folders are (`rev-parse --show-toplevel`, `--git-dir`,
//! `--git-path FETCH_HEAD`…). A reload asks each of them several times, a git process each.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::Ran;

const KEPT_FOR: Duration = Duration::from_secs(60);

type Key = (Option<PathBuf>, Vec<String>);
type Kept = (Instant, Option<i32>, String, Vec<u8>, Vec<u8>);

static KEPT: Mutex<Option<HashMap<Key, Kept>>> = Mutex::new(None);

/// Whether git's answer to `args` can be kept: one that only a new git, or a repository moved,
/// would change. Not the hooks' folder, which `core.hooksPath` moves.
fn keepable(args: &[&str]) -> bool {
    let args = args
        .strip_prefix(&["rev-parse", "--path-format=absolute"])
        .or_else(|| args.strip_prefix(&["rev-parse"]))
        .ok_or(args);
    match args {
        Err(["--version"] | ["--exec-path"]) => true,
        Ok(
            ["--show-toplevel"]
            | ["--git-common-dir"]
            | ["--git-dir"]
            | ["--absolute-git-dir"]
            | ["--show-cdup"]
            | ["--show-prefix"],
        ) => true,
        Ok(["--git-path", path]) => !path.starts_with("hooks") && !path.is_empty(),
        _ => false,
    }
}

fn key(dir: Option<&Path>, args: &[&str]) -> Key {
    (
        dir.map(Path::to_owned),
        args.iter().map(|a| (*a).to_owned()).collect(),
    )
}

/// The answer kept for `args` in `dir`, if one is.
pub(super) fn get(dir: Option<&Path>, args: &[&str]) -> Option<Ran> {
    if !keepable(args) {
        return None;
    }
    let kept = KEPT.lock().ok()?;
    let (at, code, status, stdout, stderr) = kept.as_ref()?.get(&key(dir, args))?;
    (at.elapsed() < KEPT_FOR).then(|| Ran {
        code: *code,
        status: status.clone(),
        stdout: stdout.clone(),
        stderr: stderr.clone(),
        failed_hook: None,
    })
}

/// Keeps git's answer to `args` in `dir`, when it succeeded and can be kept.
pub(super) fn keep(dir: Option<&Path>, args: &[&str], ran: &Ran) {
    if ran.code != Some(0) || !keepable(args) {
        return;
    }
    if let Ok(mut kept) = KEPT.lock() {
        let kept = kept.get_or_insert_with(HashMap::new);
        kept.retain(|_, (at, ..)| at.elapsed() < KEPT_FOR);
        kept.insert(
            key(dir, args),
            (
                Instant::now(),
                ran.code,
                ran.status.clone(),
                ran.stdout.clone(),
                ran.stderr.clone(),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::keepable;

    #[test]
    fn only_answers_nothing_but_a_new_git_or_a_moved_repository_changes_are_kept() {
        for args in [
            &["--version"][..],
            &["rev-parse", "--show-toplevel"],
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            &["rev-parse", "--git-path", "FETCH_HEAD"],
        ] {
            assert!(keepable(args), "{args:?}");
        }
        for args in [
            &["rev-parse", "HEAD"][..],
            &["rev-parse", "--git-path", "hooks"],
            &["rev-parse", "--git-path", "hooks/pre-commit"],
            &["status"],
            &["--version", "--build-options"],
        ] {
            assert!(!keepable(args), "{args:?}");
        }
    }
}
