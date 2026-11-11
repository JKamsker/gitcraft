use std::path::{Path, PathBuf};

use super::super::commands::checkout_pull_undo;
use super::super::{OpKind, Undo, UndoEntry, ops};
use super::{run_steps, stash_push_args};
use crate::git;

#[path = "ignored_scans.rs"]
mod ignored_scans;
#[path = "routes.rs"]
mod routes;
#[path = "short_rebase.rs"]
mod short_rebase;
fn recorded_route() -> Result<git::RouteBinding, String> {
    serde_json::from_value(serde_json::json!({
        "args": ["push", "--", "original", "HEAD:refs/heads/main"],
        "routes": ["https://push.invalid/team/repo"],
        "configuration": "captured-routing-config"
    }))
    .map_err(|e| e.to_string())
}
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-ops-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

#[test]
fn pull_preserves_merge_options_and_refuses_ignored_file_overwrite() -> Result<(), String> {
    let upstream = repo("pull-ignored-upstream");
    let clone = tempfile::tempdir().map_err(|e| e.to_string())?;
    let clone_path = clone.path().to_str().ok_or("clone path")?;
    git::run(Some(&upstream), &["clone", "-q", ".", clone_path])?;
    let before = git::head(clone.path());
    std::fs::write(upstream.join("secret.env"), "upstream").map_err(|e| e.to_string())?;
    git::run(Some(&upstream), &["add", "secret.env"])?;
    git::run(Some(&upstream), &["commit", "-qm", "add secret path"])?;
    std::fs::write(clone.path().join(".git/info/exclude"), "secret.env\n")
        .map_err(|e| e.to_string())?;
    std::fs::write(clone.path().join("secret.env"), "private").map_err(|e| e.to_string())?;
    let key = "branch.main.mergeOptions";
    let old = "--log=3 --no-stat";
    // --short would report heads/main when a tag uses the same name.
    git::run(Some(clone.path()), &["tag", "main"])?;
    git::run(Some(clone.path()), &["config", key, old])?;
    let pull = ops(&[&["pull", "--ff-only"]]);
    let argv = super::ignored_safe_pull(clone.path(), &pull[0])?;
    assert_eq!(argv[1], format!("{key}={old} --no-overwrite-ignore"));
    let (result, _) = run_steps(clone.path(), &pull, OpKind::plain(), None);
    assert!(result.is_err(), "{result:?}");
    assert_eq!(git::head(clone.path()), before);
    assert_eq!(
        std::fs::read(clone.path().join("secret.env")).map_err(|e| e.to_string())?,
        b"private"
    );
    assert_eq!(
        git::run(Some(clone.path()), &["config", "--get", key])?.trim(),
        old
    );
    std::fs::remove_file(clone.path().join("secret.env")).map_err(|e| e.to_string())?;
    let (result, _) = run_steps(clone.path(), &pull, OpKind::plain(), None);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(git::head(clone.path()), git::head(&upstream));
    let _ = std::fs::remove_dir_all(upstream);
    Ok(())
}

#[test]
fn rebase_pull_refuses_ignored_files_before_fetch_and_honors_explicit_modes() -> Result<(), String>
{
    let dir = repo("rebase-pull-ignored");
    std::fs::write(dir.join(".git/info/exclude"), "private\n").map_err(|e| e.to_string())?;
    std::fs::write(dir.join("private"), "precious").map_err(|e| e.to_string())?;
    let before = git::head(&dir);
    let rebase = ops(&[&["pull", "--rebase", "--autostash"]]);
    let (result, _) = run_steps(&dir, &rebase, OpKind::plain(), None);
    assert!(result.is_err_and(|e| e.contains("ignored local files")));
    assert_eq!(git::head(&dir), before);
    assert!(!dir.join(".git/FETCH_HEAD").exists());
    assert_eq!(
        std::fs::read(dir.join("private")).map_err(|e| e.to_string())?,
        b"precious"
    );
    git::run(Some(&dir), &["config", "pull.rebase", "true"])?;
    assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_err());
    for args in [&["pull", "--ff-only"][..], &["pull", "--no-rebase"][..]] {
        assert!(super::ignored_safe_pull(&dir, &ops(&[args])[0]).is_ok());
    }
    git::run(Some(&dir), &["config", "branch.main.rebase", "false"])?;
    assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_ok());
    git::run(Some(&dir), &["config", "--unset", "branch.main.rebase"])?;
    let config = dir.join(".git/config");
    let mut text = std::fs::read_to_string(&config).map_err(|e| e.to_string())?;
    text.push_str("\n[branch \"main\"]\n rebase\n");
    std::fs::write(&config, &text).map_err(|e| e.to_string())?;
    assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_err());
    git::run(Some(&dir), &["config", "branch.main.rebase", ""])?;
    assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_ok());
    std::fs::remove_file(dir.join("private")).map_err(|e| e.to_string())?;
    assert!(super::ignored_safe_pull(&dir, &rebase[0]).is_ok());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

/// A quiet operation (the background fetch) runs its git steps, and only checks its
/// expectations: git is not given them.
#[test]
fn a_quiet_operation_runs_its_steps_but_not_its_expectations() {
    let dir = repo("quiet-steps");
    let before = git::head(&dir);
    let steps: Vec<Vec<String>> = [
        super::expect_symref("HEAD", "refs/heads/main"),
        super::expect_ref_at("HEAD", &before),
    ]
    .into_iter()
    .chain(ops(&[&["commit", "-q", "--allow-empty", "-m", "quiet"]]))
    .collect();
    let (result, _, stopped) =
        super::run_steps_counted(&dir, &steps, OpKind::plain(), None, true, None);
    let after = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(stopped, steps.len());
    assert_ne!(after, before);
}

fn entry(kind: OpKind) -> Option<UndoEntry> {
    match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            id: 0,
            agent: None,
            label: "op".to_owned(),
            undo,
            redo,
            heads: None,
        }),
        _ => None,
    }
}

fn stash(dir: &Path, file: &str) -> Option<UndoEntry> {
    let _ = std::fs::write(dir.join(file), "x\n");
    let push = ops(&[&stash_push_args()]);
    let undo = Undo::Custom {
        undo: [super::expect_ref("refs/stash")]
            .into_iter()
            .chain(ops(&[&["stash", "pop", "--index"]]))
            .collect(),
        redo: push.clone(),
    };
    let (result, kind) = run_steps(dir, &push, OpKind::Normal(undo, String::new()), None);
    assert!(result.is_ok());
    entry(kind)
}

/// Undoing a stash pops that stash, not one pushed since (in the terminal).
#[test]
fn undoing_a_stash_refuses_once_another_was_pushed() {
    let dir = repo("stash-newer");
    let first = stash(&dir, "a.txt");
    let _ = std::fs::write(dir.join("b.txt"), "y\n");
    let other = git::run(Some(&dir), &["stash", "push", "-u"]);
    let refused = first
        .as_ref()
        .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None).0);
    let count = git::run(Some(&dir), &["stash", "list"]).map(|s| s.lines().count());
    // Popped by hand: now the newest is this one again, and undoing goes through.
    let popped = git::run(Some(&dir), &["stash", "pop"]);
    let _ = std::fs::remove_file(dir.join("b.txt"));
    let undone = first
        .as_ref()
        .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None));
    let restored = dir.join("a.txt").exists();
    // Redone, the undo expects the new stash.
    let redone = undone.and_then(|(r, kind)| match kind {
        OpKind::Undo(e) if r.is_ok() => {
            Some(run_steps(&dir, &e.redo.clone(), OpKind::Redo(e), None))
        }
        _ => None,
    });
    let again = redone.and_then(|(r, kind)| match kind {
        OpKind::Redo(e) if r.is_ok() => {
            Some(run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None).0)
        }
        _ => None,
    });
    let _ = std::fs::remove_dir_all(&dir);
    assert!(other.is_ok());
    assert!(matches!(refused, Some(Err(_))));
    assert_eq!(count, Ok(2));
    assert!(popped.is_ok());
    assert!(restored);
    assert!(matches!(again, Some(Ok(_))));
}

/// Redoing a stash with nothing left to stash: undoing it then pops no older stash.
#[test]
fn a_redone_stash_that_saved_nothing_is_not_popped() {
    let dir = repo("stash-redo-nothing");
    let _ = std::fs::write(dir.join("old.txt"), "o\n");
    let older = git::run(Some(&dir), &["stash", "push", "-u"]);
    let first = stash(&dir, "a.txt");
    let undone = first
        .map(|e| run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None))
        .and_then(|(r, kind)| r.ok().and(Some(kind)));
    // The changes popped back go elsewhere: redoing stashes nothing.
    let _ = std::fs::remove_file(dir.join("a.txt"));
    let redone = undone.and_then(|kind| match kind {
        OpKind::Undo(e) => Some(run_steps(&dir, &e.redo.clone(), OpKind::Redo(e), None)),
        _ => None,
    });
    let again = redone.and_then(|(r, kind)| match kind {
        OpKind::Redo(e) if r.is_ok() => {
            Some(run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None).0)
        }
        _ => None,
    });
    let count = git::run(Some(&dir), &["stash", "list"]).map(|s| s.lines().count());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(older.is_ok());
    assert!(matches!(again, Some(Err(_))));
    assert_eq!(count, Ok(1));
}

/// A quick edit's rebase does not start once HEAD moved past the commits it read.
#[test]
fn an_expected_head_that_moved_stops_the_operation() {
    let dir = repo("expect-head");
    let read = git::head(&dir);
    let made = git::run(
        Some(&dir),
        &["commit", "-q", "--allow-empty", "-m", "since"],
    );
    let since = git::head(&dir);
    let step = ops(&[&["commit", "-q", "--allow-empty", "-m", "op"]]);
    let guarded = |at: &str| {
        [super::expect_ref_at("HEAD", at)]
            .into_iter()
            .chain(step.clone())
            .collect::<Vec<_>>()
    };
    let (refused, _) = run_steps(&dir, &guarded(&read), OpKind::plain(), None);
    let still = git::head(&dir);
    let (ran, _) = run_steps(&dir, &guarded(&since), OpKind::plain(), None);
    let after = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_ok());
    assert!(refused.is_err());
    assert_eq!(still, since);
    assert!(ran.is_ok());
    assert_ne!(after, since);
}

#[test]
fn checkout_pull_and_redo_refuse_an_externally_advanced_cached_target() -> Result<(), String> {
    let dir = repo("checkout-pull-stale-target");
    let was = git::head(&dir);
    let steps = crate::repo::commands::checkout_pull_steps("target", &was);
    let redo = match checkout_pull_undo("main", "target", &was, &steps) {
        Undo::Custom { redo, .. } => redo,
        _ => return Err("missing checkout pull undo".into()),
    };
    assert_eq!(steps, redo);
    git::run(Some(&dir), &["checkout", "-qb", "target"])?;
    git::run(
        Some(&dir),
        &["commit", "-q", "--allow-empty", "-m", "external advance"],
    )?;
    let advanced = git::head(&dir);
    git::run(Some(&dir), &["checkout", "-q", "main"])?;
    for run in [&steps, &redo] {
        let (result, _) = run_steps(&dir, run, OpKind::plain(), None);
        assert!(result.is_err());
        assert_eq!(git::head(&dir), was);
        assert_eq!(git::resolve(&dir, "refs/heads/target")?, advanced);
        assert_eq!(
            git::run(Some(&dir), &["symbolic-ref", "--short", "HEAD"])?.trim(),
            "main"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

/// Undoing "Checkout and pull" does not put the branch back over commits made on it since.
#[test]
fn undoing_checkout_and_pull_keeps_commits_made_since() {
    let dir = repo("pull-since");
    let was = git::head(&dir);
    let steps = [
        &["checkout", "-q", "-b", "side"][..],
        &["commit", "-q", "--allow-empty", "-m", "pulled"],
    ];
    for args in steps {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    // As if `main` had been checked out and pulled to `pulled` from `side`.
    let pulled = git::head(&dir);
    let reset = git::run(Some(&dir), &["branch", "-f", "main", &pulled]);
    let checkout = ops(&[&["checkout", "main", "--"]]);
    let undo = checkout_pull_undo("side", "main", &was, &checkout);
    let (result, kind) = run_steps(&dir, &checkout, OpKind::Normal(undo, String::new()), None);
    let entry = entry(kind);
    // Then a commit on main.
    let more = git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "mine"]);
    let mine = git::head(&dir);
    let refused = entry
        .as_ref()
        .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None).0);
    let main_now = git::resolve(&dir, "refs/heads/main");
    let head_now = git::head(&dir);
    // Without it, undoing goes through.
    let back = git::run(Some(&dir), &["reset", "-q", "--hard", &pulled]);
    let undone = entry
        .as_ref()
        .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None).0);
    let main_after = git::resolve(&dir, "refs/heads/main");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(reset.is_ok() && result.is_ok() && more.is_ok() && back.is_ok());
    assert!(matches!(refused, Some(Err(_))));
    assert_eq!(main_now, Ok(mine.clone()));
    assert_eq!(head_now, mine);
    assert!(matches!(undone, Some(Ok(_))));
    assert_eq!(main_after, Ok(was));
}
