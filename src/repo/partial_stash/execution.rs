//! Stash execution, rollback and snapshot persistence.

use super::*;

/// Popping the stash back, but only while it is still the newest one.
pub(super) fn pop_undo() -> Vec<Vec<String>> {
    [expect_ref("refs/stash")]
        .into_iter()
        .chain(ops(&[&["stash", "pop", "--index"]]))
        .collect()
}

/// Makes the undo of a stash of only what is staged put the staged changes back with `git
/// apply` (the working tree first, then the index), then drop the stash: `git stash pop --index`
/// refuses while the files it changes have other changes, which a file staged in part always
/// has. The patch is made as `git stash` made the one it took out with (one line of context), so
/// that it goes back in where that came out. Left as it is (popping) when the patch can't be
/// kept (it isn't text git can read back).
pub(super) fn reapply_staged_undo(path: &Path, undo: &mut Vec<Vec<String>>) {
    let patch = git::run_bytes(
        Some(path),
        &[
            "diff-tree",
            "-p",
            "--binary",
            "--full-index",
            "--no-color",
            "-U1",
            "stash@{0}^1",
            "stash@{0}^2",
            "--",
        ],
    );
    let Some(patch) = patch.ok().and_then(|p| String::from_utf8(p).ok()) else {
        return;
    };
    let tree = |rev: &str| {
        git::run(
            Some(path),
            &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
        )
        .map(|s| s.trim().to_owned())
    };
    let (Ok(base), Ok(staged)) = (tree("stash@{0}^1"), tree("stash@{0}^2")) else {
        return;
    };
    let Ok(file) = git::save_patch_for(path, &patch) else {
        return;
    };
    // The check that the stash is still the newest stays first. Then that it holds what the
    // patch was made from: redone, it is made again from what is staged then, which may be
    // more, and dropping it would lose that.
    undo.retain(|step| is_expectation(step));
    undo.extend([
        expect_tree("stash@{0}^1", &base),
        expect_tree("stash@{0}^2", &staged),
    ]);
    // Both checked before either is applied, so that it isn't left half done.
    undo.extend(ops(&[
        &["apply", "--check", "--whitespace=nowarn", &file],
        &["apply", "--cached", "--check", "--whitespace=nowarn", &file],
        &["apply", "--whitespace=nowarn", &file],
        &["apply", "--cached", "--whitespace=nowarn", &file],
        &["stash", "drop", "-q"],
    ]));
}

/// `git stash store` of a snapshot, as its redo runs it again.
pub(super) fn store_args(snapshot: &git::Snapshot) -> Vec<String> {
    ops(&[&["stash", "store", "-m", &snapshot.message, &snapshot.commit]]).concat()
}

/// Makes the stash `request` asks for, in the background, and keeps its undo.
pub(crate) fn run_push(
    path: &Path,
    args: Vec<String>,
    head: String,
    roll_back_staged: bool,
) -> (Result<String, String>, OpKind) {
    let undo = Undo::Custom {
        undo: pop_undo(),
        redo: vec![args.clone()],
    };
    let before = git::resolve(path, "refs/stash").ok();
    let (result, mut kind, _) =
        run_steps_counted(path, &[args], OpKind::Normal(undo, head), None, false, None);
    if roll_back_staged
        && result.is_ok()
        && let OpKind::Normal(Undo::Custom { undo, .. }, _) = &mut kind
    {
        reapply_staged_undo(path, undo);
    }
    let result = match result {
        Ok(out) if out.contains("No local changes to save") => {
            Err(crate::i18n::text("repo-workflows-partial_stash-run-push-a7f01d16").to_owned())
        }
        Err(e) if e.trim() == "No staged changes" => {
            Err(crate::i18n::text("repo-workflows-partial_stash-run-push-e90b2782").to_owned())
        }
        Err(e) => Err(after_failed_push(
            path,
            before.as_deref(),
            &e,
            roll_back_staged,
        )),
        ok => ok,
    };
    (result, kind)
}

/// What to say when `git stash push` failed: git can save the stash and then fail to take the
/// changes out (a staged binary file, unstaged changes next to staged ones). Stashing only what
/// is staged takes them out with one `git apply`, which changes nothing when it fails: that
/// stash is dropped again, so nothing happened. Otherwise it stays, and the message says so.
pub(super) fn after_failed_push(
    path: &Path,
    before: Option<&str>,
    e: &str,
    roll_back_staged: bool,
) -> String {
    let Ok(now) = git::resolve(path, "refs/stash") else {
        return e.to_owned();
    };
    if before == Some(now.as_str()) {
        return e.to_owned();
    }
    // The stash of what is staged has the index as its tree: the index is untouched still.
    let untouched = git::run(Some(path), &["diff", "--cached", "--quiet", &now, "--"]).is_ok();
    if roll_back_staged
        && untouched
        && git::run(Some(path), &["stash", "drop", "-q", "stash@{0}"]).is_ok()
    {
        let why = if e.contains("binary patch") {
            crate::i18n::text("repo-workflows-partial_stash-after-failed-push-289207a8")
        } else {
            crate::i18n::text("repo-workflows-partial_stash-after-failed-push-26daff34")
        };
        return crate::i18n::message(
            "repo-workflows-partial_stash-after-failed-push-8f99c6b0",
            &[
                ("why", (why).into()),
                ("one-line-e-1", (one_line(e)).clone().into()),
            ],
        );
    }
    crate::i18n::message(
        "repo-workflows-partial_stash-after-failed-push-493b498c",
        &[
            ("super-short-now-1", (crate::repo::short(&now)).into()),
            ("one-line-e-2", (one_line(e)).clone().into()),
        ],
    )
}

/// What git said, on one line: the status line shows the first line of an error, and the one
/// starting with `error:` before any other, which would hide what is said before it.
pub(super) fn one_line(said: &str) -> String {
    said.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Saves a snapshot in the background (see [`git::snapshot`]) and stores it as the newest stash.
pub(super) fn run_snapshot(
    path: &Path,
    message: &str,
    untracked: bool,
    head: String,
) -> (Result<String, String>, OpKind) {
    let snapshot = match git::snapshot(path, message, untracked) {
        Ok(Some(s)) => s,
        Ok(None) if untracked => {
            return (
                Err(
                    crate::i18n::text("repo-workflows-partial_stash-run-snapshot-f67020b0")
                        .to_owned(),
                ),
                OpKind::plain(),
            );
        }
        Ok(None) => {
            return (
                Err(
                    crate::i18n::text("repo-workflows-partial_stash-run-snapshot-63cc5481")
                        .to_owned(),
                ),
                OpKind::plain(),
            );
        }
        Err(e) => return (Err(e), OpKind::plain()),
    };
    let store = vec![store_args(&snapshot)];
    let undo = Undo::Custom {
        undo: [expect_ref("refs/stash")]
            .into_iter()
            .chain(ops(&[&["stash", "drop", "-q"]]))
            .collect(),
        redo: store.clone(),
    };
    let (result, kind, _) =
        run_steps_counted(path, &store, OpKind::Normal(undo, head), None, false, None);
    (result, kind)
}
