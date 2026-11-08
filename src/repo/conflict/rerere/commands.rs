//! What remembered resolutions ask git: whether rerere is on, turning it on or off, forgetting
//! a resolution, and a conflicted file rebuilt from the index as git first left it. Blocking:
//! from the background.

use std::collections::BTreeSet;
use std::path::Path;

use crate::git::{self, InProgress};
use crate::repo::diff::literal_path;
use crate::repo::ops;

/// How rerere is set up in a repository.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Setup {
    /// It is on: `rerere.enabled`, or else an `rr-cache` folder, as git decides.
    pub on: bool,
    /// It stages what it reuses by itself (`rerere.autoUpdate`): such a file never shows as
    /// conflicted, so it can't be checked first.
    pub stages: bool,
}

/// `git config --type=bool --get <name>`: `None` when it isn't set.
fn bool_config(repo: &Path, name: &str) -> Result<Option<bool>, String> {
    let set = git::run_ok_codes(
        Some(repo),
        &["config", "--type=bool", "--get", name],
        &[0, 1],
    )
    .map_err(|e| crate::i18n::message("repo-core-0635", &[("name", format!("{}", name).into()), ("e", format!("{}", e).into())]))?;
    Ok(match set.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

/// How rerere is set up in the repository at `repo`.
pub fn setup(repo: &Path) -> Result<Setup, String> {
    let on = match bool_config(repo, "rerere.enabled")? {
        Some(on) => on,
        None => {
            let cache = git::run(Some(repo), &["rev-parse", "--git-path", "rr-cache"])?;
            let cache = crate::remote::resolve(repo, cache.trim_end_matches(['\n', '\r']));
            crate::remote::try_is_dir(&cache).unwrap_or(false)
        }
    };
    let stages = bool_config(repo, "rerere.autoUpdate")?.unwrap_or(false);
    Ok(Setup { on, stages })
}

/// Runs `args` in `repo`, `ok`: the exit codes that are fine.
fn step(repo: &Path, args: &[&str], ok: &[i32]) -> Result<(), String> {
    git::run_ok_codes(Some(repo), args, ok).map(|_| ())
}

/// Turns rerere on or off in the repository. Turned on, what it reuses is left unstaged
/// (`rerere.autoUpdate` off, here) to be checked first, and the conflicts there are now are
/// recorded at once, so that how they are resolved is remembered too. Turned off, that setting
/// goes again.
pub fn switch(repo: &Path, on: bool) -> Result<(), String> {
    if on {
        step(repo, &["config", "--local", "rerere.enabled", "true"], &[0])?;
        leave_unstaged(repo)?;
        // Records the conflicts there are now (nothing to do without any).
        step(repo, &["rerere"], &[0])
    } else {
        step(
            repo,
            &["config", "--local", "rerere.enabled", "false"],
            &[0],
        )?;
        // 5: it wasn't set.
        step(
            repo,
            &["config", "--local", "--unset", "rerere.autoUpdate"],
            &[0, 5],
        )
    }
}

/// Has rerere leave what it reuses unstaged in the repository.
pub fn leave_unstaged(repo: &Path) -> Result<(), String> {
    step(
        repo,
        &["config", "--local", "rerere.autoUpdate", "false"],
        &[0],
    )
}

/// The git steps that forget the resolution reused for `path` (`forget`; git says so, and goes
/// on, when it has none) and put the conflict markers back in it.
pub fn forget_ops(path: &str, forget: bool) -> Vec<Vec<String>> {
    let path = literal_path(path);
    // Not `--literal-pathspecs` for checkout: it runs the post-checkout hook.
    let redo = ["checkout", "-m", "--", path.as_str()];
    if forget {
        ops(&[&["rerere", "forget", "--", path.as_str()], &redo])
    } else {
        ops(&[&redo])
    }
}

/// The files git's output says were resolved from a recorded resolution (`Resolved 'x' using
/// previous resolution.`, or `Staged` when rerere stages them itself).
pub fn reused_in(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("Resolved '")
                .or_else(|| line.strip_prefix("Staged '"))?;
            rest.strip_suffix("' using previous resolution.")
                .map(str::to_owned)
        })
        .collect()
}

/// The commit merged in by the operation `state` stopped at, as git names it.
fn other_head(state: Option<InProgress>) -> Option<&'static str> {
    match state? {
        InProgress::Merge => Some("MERGE_HEAD"),
        InProgress::CherryPick => Some("CHERRY_PICK_HEAD"),
        InProgress::Revert => Some("REVERT_HEAD"),
        InProgress::Rebase => Some("REBASE_HEAD"),
        InProgress::Am => None,
    }
}

/// The empty file's blob, in the repository's object format: the base of a file added on both
/// sides.
fn empty_blob(repo: &Path) -> Result<&'static str, String> {
    let format = git::run(Some(repo), &["rev-parse", "--show-object-format"])?;
    Ok(if format.trim() == "sha256" {
        "473a0f4c3be8a93681a267e3b1e9a7dcda1185436fe141f7749120a303721813"
    } else {
        "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
    })
}

/// The blobs of the three sides of conflicted `path` (base, ours, theirs), from the index; the
/// empty file as the base of one added on both sides. `None` when one side has none (deleted
/// there): nothing to rebuild.
fn stages(repo: &Path, path: &str) -> Result<Option<[String; 3]>, String> {
    let out = git::run(
        Some(repo),
        &["--literal-pathspecs", "ls-files", "-u", "-z", "--", path],
    )?;
    let mut sides: [Option<String>; 3] = [None, None, None];
    for entry in out.split('\0') {
        // `<mode> <object> <stage>\t<path>`
        let Some((meta, name)) = entry.split_once('\t') else {
            continue;
        };
        let mut words = meta.split(' ');
        let (Some(_), Some(object), Some(stage)) = (words.next(), words.next(), words.next())
        else {
            continue;
        };
        if name != path {
            continue;
        }
        if let Some(slot) = stage
            .parse::<usize>()
            .ok()
            .and_then(|s| s.checked_sub(1))
            .and_then(|i| sides.get_mut(i))
        {
            *slot = Some(object.to_owned());
        }
    }
    Ok(match sides {
        [Some(base), Some(ours), Some(theirs)] => Some([base, ours, theirs]),
        [None, Some(ours), Some(theirs)] => Some([empty_blob(repo)?.to_owned(), ours, theirs]),
        _ => None,
    })
}

/// Whether git merges `path` line by line with markers of its usual size, unfiltered: what the
/// rebuilt conflict is like. A file merged otherwise (`-merge`, `merge=binary`, a merge
/// driver), with markers of another size (`conflict-marker-size`) or kept filtered (LFS…)
/// isn't rebuilt.
fn rebuildable(repo: &Path, path: &str) -> Result<bool, String> {
    let out = git::run(
        Some(repo),
        &[
            "check-attr",
            "-z",
            "conflict-marker-size",
            "merge",
            "filter",
            "--",
            path,
        ],
    )?;
    // `<path>\0<attribute>\0<value>\0` for each.
    let fields: Vec<&str> = out.split('\0').collect();
    Ok(fields.chunks(3).all(|f| match f {
        [_, "conflict-marker-size", value] => matches!(*value, "unspecified" | "7"),
        [_, "merge", value] => matches!(*value, "unspecified" | "set" | "text"),
        [_, "filter", value] => matches!(*value, "unspecified" | "unset"),
        _ => true,
    }))
}

/// `text` with its line ends as `like`'s: CRLF when it has them (the index keeps LF, a
/// checkout may turn them into CRLF).
fn line_ends_like(text: String, like: &str) -> String {
    if like.contains("\r\n") && !text.contains("\r\n") {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

/// Conflicted `path` as git first left it, conflict markers and all, rebuilt from the index
/// without touching the file, its line ends as `current`'s (the file now). `None`: there is
/// nothing to rebuild (see [`stages`], [`rebuildable`], a binary file).
pub fn preimage(
    repo: &Path,
    path: &str,
    state: Option<InProgress>,
    current: &str,
) -> Result<Option<String>, String> {
    if !rebuildable(repo, path)? {
        return Ok(None);
    }
    let Some([base, ours, theirs]) = stages(repo, path)? else {
        return Ok(None);
    };
    let other = other_head(state)
        .and_then(|head| {
            git::run(
                Some(repo),
                &["rev-parse", "--verify", "-q", "--short", head],
            )
            .ok()
        })
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "theirs".to_owned());
    // Exits with the number of conflicts; above 127 for an error.
    let ok: Vec<i32> = (0..=127).collect();
    let merged = git::run_ok_codes(
        Some(repo),
        &[
            "merge-file",
            "-p",
            "--diff3",
            "--object-id",
            "-L",
            "HEAD",
            "-L",
            "base",
            "-L",
            &other,
            &ours,
            &base,
            &theirs,
        ],
        &ok,
    );
    match merged {
        Ok(out) => Ok(Some(line_ends_like(out, current))),
        Err(e) if e.to_lowercase().contains("binary") => Ok(None),
        Err(e) if e.contains("object-id") || e.contains("usage:") => {
            Err(crate::i18n::text("repo-core-0669").to_owned())
        }
        Err(e) => Err(crate::i18n::message("repo-core-0670", &[("path", format!("{}", path).into()), ("e", format!("{}", e).into())])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_ends_follow_the_file() {
        assert_eq!(line_ends_like("a\nb\n".into(), "x\r\ny\r\n"), "a\r\nb\r\n");
        assert_eq!(line_ends_like("a\nb\n".into(), "x\ny\n"), "a\nb\n");
        assert_eq!(line_ends_like("a\r\n".into(), "x\r\n"), "a\r\n");
    }

    #[test]
    fn reused_files_are_read_from_gits_output() {
        let out = "Auto-merging a b/ü.txt\nCONFLICT (content): Merge conflict in a b/ü.txt\n\
                   Resolved 'a b/ü.txt' using previous resolution.\nStaged 'c' using previous \
                   resolution.\nRecorded preimage for 'd'\nAutomatic merge failed";
        let found: Vec<String> = reused_in(out).into_iter().collect();
        assert_eq!(found, ["a b/ü.txt", "c"]);
        assert!(reused_in("CONFLICT (content): Merge conflict in x").is_empty());
        assert!(reused_in("").is_empty());
    }
}
