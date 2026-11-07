//! Restacking real three-layer stacks: after an amend, through a conflict and its resume, an
//! abort, and with a layer checked out in another worktree.

use super::recorded;
use super::restack::{Outcome, Place, abort, parse_pending, pending, prepare, resume, start};
use super::test_repo::TestRepo;

/// The steps restacking `layers` (each on the one before, the first on `main`) takes.
fn steps(repo: &TestRepo, layers: &[&str]) -> Result<Vec<super::restack::Step>, String> {
    let rec = recorded(repo.path())?;
    let list: Vec<(String, String, Option<String>)> = layers
        .iter()
        .map(|b| {
            let r = rec.get(*b).cloned().unwrap_or_default();
            ((*b).to_owned(), r.parent, r.base)
        })
        .collect();
    Ok(prepare(repo.path(), &list)?
        .into_iter()
        .map(|p| p.step)
        .collect())
}

fn is_ancestor(repo: &TestRepo, a: &str, b: &str) -> bool {
    repo.git(&["merge-base", "--is-ancestor", a, b]).is_ok()
}

/// How many commits `b` has on top of `a`.
fn on_top(repo: &TestRepo, a: &str, b: &str) -> Result<String, String> {
    Ok(repo
        .git(&["rev-list", "--count", &format!("{a}..{b}")])?
        .trim()
        .to_owned())
}

#[test]
fn a_three_layer_stack_is_restacked_after_its_bottom_is_amended() -> Result<(), String> {
    let repo = TestRepo::new("restack three ü")?;
    repo.stack(&["schema", "api", "ui"])?;
    repo.checkout("schema")?;
    repo.git(&["commit", "-q", "--amend", "-m", "schema amended"])?;
    repo.checkout("api")?;
    let previews = prepare(
        repo.path(),
        &[(
            "api".into(),
            "schema".into(),
            recorded(repo.path())?
                .get("api")
                .and_then(|r| r.base.clone()),
        )],
    )?;
    assert_eq!(previews[0].commits, 1);
    assert_eq!(previews[0].new_base, repo.tip("schema")?);
    let before_ui = repo.tip("ui")?;
    let outcome = start(repo.path(), steps(&repo, &["api", "ui"])?)?;
    let Outcome::Done(moved) = outcome else {
        return Err(format!("stopped: {outcome:?}"));
    };
    assert_eq!(moved.len(), 2);
    assert!(is_ancestor(&repo, "schema", "api") && is_ancestor(&repo, "api", "ui"));
    // Each keeps only its own commit: the old schema commit is not replayed.
    assert_eq!(on_top(&repo, "schema", "api")?, "1");
    assert_eq!(on_top(&repo, "api", "ui")?, "1");
    // Back on the branch checked out before; the bases recorded are the parents' tips.
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(),
        "api"
    );
    let rec = recorded(repo.path())?;
    assert_eq!(
        rec.get("ui").and_then(|r| r.base.clone()),
        Some(repo.tip("api")?)
    );
    assert_eq!(
        rec.get("api").and_then(|r| r.base.clone()),
        Some(repo.tip("schema")?)
    );
    let ui = moved
        .iter()
        .find(|m| m.branch == "ui")
        .ok_or("ui didn't move")?;
    assert_eq!(
        (ui.before.as_str(), ui.place.clone()),
        (before_ui.as_str(), Place::Nowhere)
    );
    let api = moved
        .iter()
        .find(|m| m.branch == "api")
        .ok_or("api didn't move")?;
    assert_eq!(api.place, Place::Here);
    assert!(pending(repo.path())?.is_none());
    // Nothing left to do: a second restack moves nothing.
    let Outcome::Done(again) = start(repo.path(), steps(&repo, &["api", "ui"])?)? else {
        return Err("a restacked stack stopped".into());
    };
    assert!(again.is_empty());
    Ok(())
}

/// A stack whose middle layer changes the file its parent's amend changed too.
fn conflicting(name: &str) -> Result<TestRepo, String> {
    conflicting_in(name, "api")
}

/// `main → schema → api → ui`, `layer` (api or ui) changing the file schema then changes again.
fn conflicting_in(name: &str, layer: &str) -> Result<TestRepo, String> {
    let repo = TestRepo::new(name)?;
    repo.branch("schema", "main")?;
    repo.commit("s.txt", "schema\n", "schema")?;
    repo.steps(&super::set_parent_ops("schema", "main", &repo.tip("main")?))?;
    let file = |l: &str| if l == layer { "s.txt" } else { "other.txt" };
    repo.branch("api", "schema")?;
    repo.commit(file("api"), "api\n", "api")?;
    repo.steps(&super::set_parent_ops(
        "api",
        "schema",
        &repo.tip("schema")?,
    ))?;
    repo.branch("ui", "api")?;
    repo.commit(file("ui"), "ui\n", "ui")?;
    repo.steps(&super::set_parent_ops("ui", "api", &repo.tip("api")?))?;
    repo.checkout("schema")?;
    repo.commit("s.txt", "schema 2\n", "schema again")?;
    repo.checkout("main")?;
    Ok(repo)
}

#[test]
fn a_conflict_stops_the_restack_and_it_resumes_once_resolved() -> Result<(), String> {
    let repo = conflicting("restack conflict")?;
    let outcome = start(repo.path(), steps(&repo, &["api", "ui"])?)?;
    let Outcome::Stopped {
        branch, worktree, ..
    } = outcome
    else {
        return Err(format!("no stop: {outcome:?}"));
    };
    assert_eq!((branch.as_str(), worktree), ("api", None));
    let kept = pending(repo.path())?.ok_or("nothing kept")?;
    assert_eq!(
        (kept.done, kept.steps.len(), kept.head.as_str()),
        (0, 2, "refs/heads/main")
    );
    // Still conflicted: resuming stops again.
    assert!(matches!(resume(repo.path())?, Outcome::Stopped { .. }));
    std::fs::write(repo.dir.join("s.txt"), "both\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "s.txt"])?;
    let Outcome::Done(moved) = resume(repo.path())? else {
        return Err("resolved, it still stopped".into());
    };
    assert_eq!(moved.len(), 2);
    assert!(is_ancestor(&repo, "schema", "api") && is_ancestor(&repo, "api", "ui"));
    assert_eq!(repo.git(&["show", "api:s.txt"])?, "both\n");
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(),
        "main"
    );
    assert!(pending(repo.path())?.is_none());
    Ok(())
}

#[test]
fn a_rebase_finished_outside_the_restack_is_taken_in() -> Result<(), String> {
    let repo = conflicting("restack continued elsewhere")?;
    assert!(matches!(
        start(repo.path(), steps(&repo, &["api", "ui"])?)?,
        Outcome::Stopped { .. }
    ));
    std::fs::write(repo.dir.join("s.txt"), "both\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "s.txt"])?;
    // The app's own Continue (or the terminal) finished the rebase.
    repo.git(&["rebase", "--continue"])?;
    assert!(matches!(resume(repo.path())?, Outcome::Done(_)));
    assert!(is_ancestor(&repo, "api", "ui") && is_ancestor(&repo, "schema", "api"));
    Ok(())
}

#[test]
fn aborting_puts_every_branch_back() -> Result<(), String> {
    // api is replayed, then ui stops on its conflict: both are put back.
    let repo = conflicting_in("restack abort", "ui")?;
    let (api, ui) = (repo.tip("api")?, repo.tip("ui")?);
    let rec = recorded(repo.path())?;
    let outcome = start(repo.path(), steps(&repo, &["api", "ui"])?)?;
    assert!(
        matches!(&outcome, Outcome::Stopped { branch, .. } if branch == "ui"),
        "{outcome:?}"
    );
    assert_ne!(repo.tip("api")?, api);
    assert_ne!(recorded(repo.path())?, rec);
    abort(repo.path())?;
    assert_eq!((repo.tip("api")?, repo.tip("ui")?), (api, ui));
    assert_eq!(recorded(repo.path())?, rec);
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(),
        "main"
    );
    assert!(repo.git(&["status", "--porcelain"])?.trim().is_empty());
    assert!(pending(repo.path())?.is_none());
    assert!(abort(repo.path()).is_err());
    Ok(())
}

#[test]
fn a_layer_checked_out_in_another_worktree_is_rebased_there() -> Result<(), String> {
    let repo = TestRepo::new("restack worktree")?;
    repo.stack(&["schema", "api", "ui"])?;
    let wt = repo.dir.with_file_name(format!(
        "{} wt ü",
        repo.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&wt);
    let wt_str = wt.to_string_lossy().into_owned();
    repo.git(&["worktree", "add", "-q", &wt_str, "api"])?;
    // An untracked file there is kept.
    std::fs::write(wt.join("note.txt"), "mine\n").map_err(|e| e.to_string())?;
    repo.checkout("schema")?;
    repo.commit("s2.txt", "2\n", "schema 2")?;
    repo.checkout("main")?;
    let previews = {
        let rec = recorded(repo.path())?;
        let get = |b: &str| rec.get(b).and_then(|r| r.base.clone());
        prepare(
            repo.path(),
            &[
                ("api".into(), "schema".into(), get("api")),
                ("ui".into(), "api".into(), get("ui")),
            ],
        )?
    };
    assert!(previews[0].worktree.is_some() && previews[1].worktree.is_none());
    let result = start(repo.path(), previews.into_iter().map(|p| p.step).collect());
    let worktree_head = git_in(&wt, &["rev-parse", "HEAD"]);
    let note = std::fs::read_to_string(wt.join("note.txt")).unwrap_or_default();
    let _ = repo.git(&["worktree", "remove", "--force", &wt_str]);
    let _ = std::fs::remove_dir_all(&wt);
    let Outcome::Done(moved) = result? else {
        return Err("stopped".into());
    };
    assert!(is_ancestor(&repo, "schema", "api") && is_ancestor(&repo, "api", "ui"));
    // The worktree's checkout moved with its branch.
    assert_eq!(worktree_head?.trim(), repo.tip("api")?);
    assert_eq!(note, "mine\n");
    let api = moved
        .iter()
        .find(|m| m.branch == "api")
        .ok_or("api didn't move")?;
    assert!(matches!(api.place, Place::Worktree(_)));
    Ok(())
}

fn git_in(dir: &std::path::Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

#[test]
fn changes_here_are_refused_before_anything_moves() -> Result<(), String> {
    let repo = TestRepo::new("restack dirty")?;
    repo.stack(&["schema", "api"])?;
    repo.checkout("schema")?;
    repo.commit("s2.txt", "2\n", "schema 2")?;
    std::fs::write(repo.dir.join("base.txt"), "changed\n").map_err(|e| e.to_string())?;
    let api = repo.tip("api")?;
    let err = start(repo.path(), steps(&repo, &["api"])?)
        .err()
        .unwrap_or_default();
    assert!(err.contains("Commit or stash"), "{err}");
    assert_eq!(repo.tip("api")?, api);
    assert!(pending(repo.path())?.is_none());
    // Nothing to restack is done at once, whatever the tree.
    assert!(matches!(start(repo.path(), Vec::new())?, Outcome::Done(m) if m.is_empty()));
    Ok(())
}

#[test]
fn the_kept_restack_reads_back() {
    let out = "gitcraft-restack.head\nrefs/heads/main\0gitcraft-restack.done\n1\0\
               gitcraft-restack.step\napi\tschema\tabc\0gitcraft-restack.step\nui\tapi\tdef\0\
               gitcraft-restack.before\napi\t111\tabc\0gitcraft-restack.before\nui\t222\t\0";
    let got = parse_pending(out).unwrap_or_default();
    assert_eq!(got.done, 1);
    assert_eq!(got.current().map(|s| s.branch.as_str()), Some("ui"));
    assert_eq!(got.before[1].base, None);
    assert_eq!(got.head, "refs/heads/main");
    assert!(parse_pending("").is_none());
    assert!(parse_pending("gitcraft-restack.head\nx\0").is_none());
}

/// A worktree next to the repository with `branch` checked out; removed by the caller.
fn worktree(repo: &TestRepo, branch: &str) -> Result<std::path::PathBuf, String> {
    let name = repo
        .dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let wt = repo.dir.with_file_name(format!("{name} {branch} wt"));
    let _ = std::fs::remove_dir_all(&wt);
    repo.git(&["worktree", "add", "-q", &wt.to_string_lossy(), branch])?;
    Ok(wt)
}

#[test]
fn a_conflict_in_another_worktree_stops_there_resumes_and_aborts() -> Result<(), String> {
    let run = |resolve: bool| -> Result<(), String> {
        let repo = conflicting_in(&format!("restack wt conflict {resolve}"), "api")?;
        let wt = worktree(&repo, "api")?;
        let (api, ui) = (repo.tip("api")?, repo.tip("ui")?);
        let result = (|| -> Result<(), String> {
            let outcome = start(repo.path(), steps(&repo, &["api", "ui"])?)?;
            let Outcome::Stopped { worktree, .. } = outcome else {
                return Err(format!("no stop: {outcome:?}"));
            };
            // The same folder, however git spells it (on Windows: `/` and the long names).
            let same = |a: &std::path::Path| std::fs::canonicalize(a).ok();
            let stopped_in = worktree.as_deref().and_then(same);
            assert!(
                stopped_in.is_some() && stopped_in == same(&wt),
                "{worktree:?}"
            );
            // Here nothing moved, nor is anything checked out differently.
            assert_eq!(
                repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(),
                "main"
            );
            if resolve {
                std::fs::write(wt.join("s.txt"), "both\n").map_err(|e| e.to_string())?;
                git_in(&wt, &["add", "s.txt"])?;
                assert!(matches!(resume(repo.path())?, Outcome::Done(_)));
                assert!(is_ancestor(&repo, "schema", "api") && is_ancestor(&repo, "api", "ui"));
                assert_eq!(
                    git_in(&wt, &["symbolic-ref", "--short", "HEAD"])?.trim(),
                    "api"
                );
            } else {
                abort(repo.path())?;
                assert_eq!(
                    (repo.tip("api")?, repo.tip("ui")?),
                    (api.clone(), ui.clone())
                );
                assert_eq!(
                    git_in(&wt, &["symbolic-ref", "--short", "HEAD"])?.trim(),
                    "api"
                );
                assert!(git_in(&wt, &["status", "--porcelain"])?.trim().is_empty());
            }
            assert!(pending(repo.path())?.is_none());
            Ok(())
        })();
        let _ = repo.git(&["worktree", "remove", "--force", &wt.to_string_lossy()]);
        let _ = std::fs::remove_dir_all(&wt);
        result
    };
    run(true)?;
    run(false)
}

/// A parent rewound to an older commit: the commit it lost is dropped from the layer too,
/// not kept as one of the layer's own.
#[test]
fn a_rewound_parent_is_restacked_onto() -> Result<(), String> {
    let repo = TestRepo::new("restack rewound")?;
    repo.branch("a", "main")?;
    let a1 = repo.commit("a1.txt", "1\n", "a1")?;
    repo.commit("a2.txt", "2\n", "a2")?;
    repo.steps(&super::set_parent_ops("a", "main", &repo.tip("main")?))?;
    repo.branch("b", "a")?;
    repo.commit("b.txt", "b\n", "b")?;
    repo.steps(&super::set_parent_ops("b", "a", &repo.tip("a")?))?;
    repo.checkout("main")?;
    repo.git(&["branch", "-f", "a", &a1])?;
    let Outcome::Done(moved) = start(repo.path(), steps(&repo, &["b"])?)? else {
        return Err("stopped".into());
    };
    assert_eq!(moved.len(), 1);
    assert_eq!(on_top(&repo, "a", "b")?, "1");
    assert!(repo.git(&["cat-file", "-e", "b:a2.txt"]).is_err());
    Ok(())
}

/// Aborting leaves alone a layer that got commits after it was restacked.
#[test]
fn aborting_keeps_commits_made_after_a_layer_was_restacked() -> Result<(), String> {
    let repo = conflicting_in("restack abort keeps", "ui")?;
    let api = repo.tip("api")?;
    assert!(matches!(
        start(repo.path(), steps(&repo, &["api", "ui"])?)?,
        Outcome::Stopped { .. }
    ));
    repo.git(&["rebase", "--abort"])?;
    repo.checkout("api")?;
    let precious = repo.commit("precious.txt", "p\n", "precious")?;
    repo.checkout("main")?;
    let kept = abort(repo.path())?;
    assert_eq!(kept, ["api"]);
    assert_eq!(repo.tip("api")?, precious);
    assert_ne!(repo.tip("api")?, api);
    assert!(pending(repo.path())?.is_none());
    Ok(())
}

/// A layer whose worktree has a merge of the user's going on isn't rebased there, nor is that
/// merge taken for a stop of the restack.
#[test]
fn a_worktree_with_an_operation_in_progress_is_refused() -> Result<(), String> {
    let repo = TestRepo::new("restack wt busy")?;
    repo.stack(&["schema", "api"])?;
    repo.checkout("schema")?;
    repo.commit("s2.txt", "2\n", "schema 2")?;
    repo.git(&["branch", "side", "main"])?;
    repo.checkout("side")?;
    repo.commit("api.txt", "side\n", "side changes api's file")?;
    repo.checkout("main")?;
    let wt = worktree(&repo, "api")?;
    let result = (|| -> Result<(), String> {
        // A merge stopped on conflicts there.
        assert!(git_in(&wt, &["merge", "side"]).is_err());
        let api = repo.tip("api")?;
        let err = start(repo.path(), steps(&repo, &["api"])?)
            .err()
            .unwrap_or_default();
        assert!(err.contains("merge in progress"), "{err}");
        assert_eq!(repo.tip("api")?, api);
        // Kept, so that it can be aborted once the merge is dealt with.
        git_in(&wt, &["merge", "--abort"])?;
        assert!(abort(repo.path())?.is_empty());
        Ok(())
    })();
    let _ = repo.git(&["worktree", "remove", "--force", &wt.to_string_lossy()]);
    let _ = std::fs::remove_dir_all(&wt);
    result
}
