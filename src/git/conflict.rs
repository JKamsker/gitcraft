//! What the index keeps of a conflicted file: its stages (base, ours, theirs), the conflict
//! made again with the base shown, the two sides' blobs, and names for the two sides.

use std::path::Path;

use super::{END_OPTS, InProgress, LITERAL_PATHS, run, run_bytes, run_ok_codes};

/// One stage of a conflicted file in the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage {
    /// `100644`, `100755`, `120000` (a link), `160000` (a submodule).
    pub mode: String,
    pub oid: String,
}

/// The stages a conflicted file has: none of them is there in every conflict (add/add has no
/// base, modify/delete lacks one side).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConflictStages {
    pub base: Option<Stage>,
    pub ours: Option<Stage>,
    pub theirs: Option<Stage>,
}

/// The stages of `path` in the index of `repo` (all empty when it isn't in conflict).
pub fn conflict_stages(repo: &Path, path: &str) -> Result<ConflictStages, String> {
    let out = run(
        Some(repo),
        &[LITERAL_PATHS, "ls-files", "-u", "-z", "--", path],
    )?;
    Ok(parse_stages(&out, path))
}

/// `ls-files -u -z` output (`<mode> <oid> <stage>\t<path>\0`…), the entries of `path` kept.
fn parse_stages(out: &str, path: &str) -> ConflictStages {
    let mut stages = ConflictStages::default();
    for entry in out.split('\0') {
        let Some((info, name)) = entry.split_once('\t') else {
            continue;
        };
        if name != path {
            continue;
        }
        let mut words = info.split(' ');
        let (Some(mode), Some(oid), Some(stage)) = (words.next(), words.next(), words.next())
        else {
            continue;
        };
        let found = Some(Stage {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
        });
        match stage {
            "1" => stages.base = found,
            "2" => stages.ours = found,
            "3" => stages.theirs = found,
            _ => {}
        }
    }
    stages
}

/// The blob `oid` as it is checked out at `path`: through the filters its attributes name
/// (Git LFS's smudge, line endings), so an LFS pointer comes out as the file it points to.
pub fn checked_out_blob(repo: &Path, oid: &str, path: &str) -> Result<Vec<u8>, String> {
    run_bytes(
        Some(repo),
        &["cat-file", "--filters", &format!("--path={path}"), oid],
    )
}

/// Whether `path` would be ignored if it were a new file (a `.gitignore` names it).
pub fn is_ignored(repo: &Path, path: &str) -> Result<bool, String> {
    // Exit 0: ignored (and printed); 1: not.
    let out = run_ok_codes(
        Some(repo),
        &["check-ignore", "--no-index", "--", path],
        &[0, 1],
    )?;
    Ok(!out.trim().is_empty())
}

/// `path`'s conflict made again from its stages in the index, in the zdiff3 style: each
/// conflict with the base between `|||||||` and `=======`, as wide as git's own markers make
/// it. Needs git 2.43 (`merge-file --object-id`).
pub fn conflict_with_base(repo: &Path, path: &str) -> Result<String, String> {
    let stage = |n: u8| format!(":{n}:{path}");
    // The exit code is how many conflicts there are (up to 127); errors are negative (255).
    let ok: Vec<i32> = (0..=127).collect();
    run_ok_codes(
        Some(repo),
        &[
            "merge-file",
            "-p",
            "--zdiff3",
            "--object-id",
            "-L",
            "ours",
            "-L",
            "base",
            "-L",
            "theirs",
            &stage(2),
            &stage(1),
            &stage(3),
        ],
        &ok,
    )
    .map_err(|e| {
        if e.contains("object-id") || e.contains("usage:") {
            "showing the original needs git 2.43 or newer (or merge.conflictStyle set to diff3)"
                .to_owned()
        } else {
            e
        }
    })
}

/// Names for the two sides of the conflicts left by `state` (ours, theirs), from the branches
/// they come from: `main` and `feature/x` for a merge of `feature/x` into `main`. `None` for a
/// side no branch names (a detached HEAD, a stash).
pub fn side_branches(repo: &Path, state: Option<InProgress>) -> (Option<String>, Option<String>) {
    let theirs = match state {
        Some(InProgress::Merge) => Some("MERGE_HEAD"),
        Some(InProgress::CherryPick) => Some("CHERRY_PICK_HEAD"),
        Some(InProgress::Revert) => Some("REVERT_HEAD"),
        Some(InProgress::Rebase) => Some("REBASE_HEAD"),
        Some(InProgress::Am) | None => None,
    };
    let ours = run(Some(repo), &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| branch_of(repo, "HEAD"));
    (ours, theirs.and_then(|head| branch_of(repo, head)))
}

/// The local branch `rev` is on (`feature/x` for its tip or `feature/x~2`), if any.
fn branch_of(repo: &Path, rev: &str) -> Option<String> {
    let out = run(
        Some(repo),
        &[
            "name-rev",
            "--name-only",
            "--no-undefined",
            "--refs=refs/heads/*",
            END_OPTS,
            rev,
        ],
    )
    .ok()?;
    let name = out.trim();
    let name = name.split(['~', '^']).next().unwrap_or(name);
    let name = name.strip_prefix("heads/").unwrap_or(name);
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_of_the_path_only() {
        let out = [
            "100644 aaa 1\tlogo.png",
            "100644 bbb 2\tlogo.png",
            "100755 ccc 3\tlogo.png",
            "100644 ddd 2\tlogo.png.bak",
            "",
        ]
        .join("\0");
        let stages = parse_stages(&out, "logo.png");
        assert_eq!(
            stages.base,
            Some(Stage {
                mode: "100644".into(),
                oid: "aaa".into()
            })
        );
        assert_eq!(stages.ours.map(|s| s.oid), Some("bbb".into()));
        assert_eq!(stages.theirs.map(|s| s.mode), Some("100755".into()));
        // add/add: no base; modify/delete: one side.
        let add_add = parse_stages("100644 b 2\tn é.txt\x00100644 c 3\tn é.txt\x00", "n é.txt");
        assert!(add_add.base.is_none() && add_add.ours.is_some() && add_add.theirs.is_some());
        let deleted = parse_stages("100644 a 1\tf\x00100644 b 2\tf\x00", "f");
        assert!(deleted.theirs.is_none());
        assert_eq!(parse_stages("", "f"), ConflictStages::default());
    }
}
