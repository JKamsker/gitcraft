//! Decisions on real repositories: stored in git and read back, sent and fetched, applied when
//! the same conflict comes back, offered as stale when the code around it changed, and their
//! tests run, through the tab as the conflict editor uses them.

use std::path::{Path, PathBuf};

use super::memo::Draft;
use super::record::{Decision, key};
use super::*;
use crate::git::InProgress;
use crate::repo::conflict::{Choice, ConflictEditor};

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

/// A fresh repository named after `name`, with one commit.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-decide-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        git(&dir, args)?;
    }
    Ok(dir)
}

const FILE: &str =
    "1\n2\n3\n<<<<<<< HEAD\nnew validation\n=======\nold wire format\n>>>>>>> b\n4\n5\n6\n";

/// An editor on `text` written to `dir/path`.
fn editor(dir: &Path, path: &str, text: &str) -> Result<ConflictEditor, String> {
    std::fs::write(dir.join(path), text).map_err(|e| e.to_string())?;
    ConflictEditor::open(dir, path, Some(InProgress::Merge)).ok_or_else(|| "not opened".into())
}

/// Polls `tab` until `done` holds (or a few seconds pass).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn decisions_are_stored_in_git_and_read_back() -> Result<(), String> {
    let dir = repo("store")?;
    let k = key("ours\n", "theirs\n");
    let decision = Decision {
        path: "dír ü/a b.rs".into(),
        note: "keep the new validation, keep the old wire format — ünïcode \"quoted\"".into(),
        test: "cargo test wire".into(),
        resolution: "both\n".repeat(5000),
        ours: "ours\n".into(),
        theirs: "theirs\n".into(),
        ..Decision::default()
    };
    store::save(&dir, &k, &decision)?;
    let other = key("x", "y");
    let found = store::load(&dir, &[k.clone(), other.clone(), "../bad".into()]);
    let refs = git(&dir, &["for-each-ref", "--format=%(refname)", store::REFS])?;
    let _ = std::fs::remove_dir_all(&dir);
    let found = found?;
    assert_eq!(found.len(), 1);
    assert_eq!(found.get(&k), Some(&decision));
    assert!(!found.contains_key(&other));
    assert_eq!(refs.trim(), format!("{}{k}", store::REFS));
    assert!(store::save(Path::new("/nonexistent"), "../x", &decision).is_err());
    Ok(())
}

/// They travel only when asked: sent to a remote and fetched from it into another clone.
#[test]
fn decisions_travel_with_a_push_and_a_fetch() -> Result<(), String> {
    let dir = repo("travel")?;
    let remote = dir.with_extension("remote.git");
    let clone = dir.with_extension("clone");
    let _ = std::fs::remove_dir_all(&remote);
    let _ = std::fs::remove_dir_all(&clone);
    let run = |at: &Path, steps: Vec<Vec<String>>| -> Result<(), String> {
        for step in steps {
            let args: Vec<&str> = step.iter().map(String::as_str).collect();
            git(at, &args)?;
        }
        Ok(())
    };
    let outcome = (|| -> Result<Option<Decision>, String> {
        crate::git::run(None, &["init", "-q", "--bare", &remote.to_string_lossy()])?;
        git(
            &dir,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )?;
        let k = key("a", "b");
        let mut d = Decision {
            note: "first".into(),
            ..Decision::default()
        };
        store::save(&dir, &k, &d)?;
        run(&dir, store::push_ops("origin"))?;
        // Changed and sent again: the remote's is replaced.
        d.note = "second".into();
        store::save(&dir, &k, &d)?;
        run(&dir, store::push_ops("origin"))?;
        crate::git::run(None, &["init", "-q", &clone.to_string_lossy()])?;
        git(
            &clone,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )?;
        run(&clone, store::fetch_ops("origin"))?;
        Ok(store::load(&clone, std::slice::from_ref(&k))?.remove(&k))
    })();
    for d in [&dir, &remote, &clone] {
        let _ = std::fs::remove_dir_all(d);
    }
    assert_eq!(outcome?.map(|d| d.note), Some("second".to_owned()));
    Ok(())
}

/// Resolved with a note and saved, the decision is stored; the same conflict opened again is
/// resolved the same way at once, its note and test filled in.
#[test]
fn the_same_conflict_comes_back_resolved_with_its_reason() -> Result<(), String> {
    let dir = repo("again")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let mut ed = editor(&dir, "f.rs", FILE)?;
    ed.choose(0, Choice::OursThenTheirs);
    ed.memo.drafts = vec![Draft {
        open: false,
        note: "  keep the new validation, keep the old wire format ".into(),
        test: "true\n".into(),
    }];
    tab.conflict = Some(ed);
    tab.record_decisions("f.rs", &ctx);
    wait(&mut tab, &ctx, |tab| tab.status.is_some());
    let kept = tab.status.clone();

    // The same conflict, elsewhere in another file.
    let elsewhere = format!("0\n{FILE}");
    tab.conflict = Some(editor(&dir, "g.rs", &elsewhere)?);
    tab.status = None;
    tab.decisions_act(Act::Load, &ctx);
    wait(&mut tab, &ctx, |tab| {
        tab.conflict
            .as_ref()
            .is_some_and(|ed| ed.memo.offers.is_some())
    });
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        kept,
        Some(("Kept the decision and its reason".to_owned(), false))
    );
    let ed = tab.conflict.as_ref().ok_or("no editor")?;
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert_eq!(
        ed.result,
        "0\n1\n2\n3\nnew validation\nold wire format\n4\n5\n6\n"
    );
    let offer = ed.offer(0).ok_or("nothing offered")?;
    assert!(!offer.stale);
    assert_eq!(
        offer.decision.note,
        "keep the new validation, keep the old wire format"
    );
    assert_eq!(offer.decision.test, "true");
    assert_eq!(offer.decision.by, "t");
    assert_eq!(ed.memo.drafts[0].note, offer.decision.note);
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(s, _)| s.contains("Applied a decision"))
    );
    Ok(())
}

/// The code around the conflict changed since: offered as stale, not applied, until asked.
#[test]
fn a_decision_whose_surroundings_changed_is_only_offered() -> Result<(), String> {
    let dir = repo("stale")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let mut ed = editor(&dir, "f.rs", FILE)?;
    ed.choose(0, Choice::Theirs);
    ed.memo.drafts = vec![Draft {
        note: "the wire format wins".into(),
        ..Draft::default()
    }];
    tab.conflict = Some(ed);
    tab.record_decisions("f.rs", &ctx);
    wait(&mut tab, &ctx, |tab| tab.status.is_some());

    let moved = FILE.replace("3\n", "three\n");
    tab.conflict = Some(editor(&dir, "f.rs", &moved)?);
    tab.decisions_act(Act::Load, &ctx);
    wait(&mut tab, &ctx, |tab| {
        tab.conflict
            .as_ref()
            .is_some_and(|ed| ed.memo.offers.is_some())
    });
    let _ = std::fs::remove_dir_all(&dir);
    let ed = tab.conflict.as_mut().ok_or("no editor")?;
    let offer = ed.offer(0).ok_or("nothing offered")?.clone();
    assert!(offer.stale);
    assert_eq!(offer.now.0, "1\n2\nthree\n");
    assert_eq!(offer.decision.before, "1\n2\n3\n");
    assert_eq!(ed.choices, vec![None]);
    ed.use_offer(0);
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert!(ed.result.contains("1\n2\nthree\nold wire format\n4\n"));
    Ok(())
}

/// A test run once the file is saved: how it went is said and kept with the decision; a
/// failing one says its last line.
#[test]
fn tests_run_after_saving_and_are_remembered() -> Result<(), String> {
    let dir = repo("tests")?;
    let k = key("a\n", "b\n");
    let decision = |test: &str| Decision {
        test: test.into(),
        ours: "a\n".into(),
        theirs: "b\n".into(),
        ..Decision::default()
    };
    let passing = record_and_test(
        &dir,
        vec![(
            k.clone(),
            decision("printf 'ü %s\\n' \"two words\" && test -f .git/HEAD"),
        )],
        true,
    );
    let stored_pass = store::load(&dir, std::slice::from_ref(&k))?.remove(&k);
    let failing = record_and_test(
        &dir,
        vec![(
            k.clone(),
            decision("echo first; echo 'last words' >&2; exit 3"),
        )],
        true,
    );
    let stored_fail = store::load(&dir, std::slice::from_ref(&k))?.remove(&k);
    // Not asked to run: kept, not run.
    let not_run = record_and_test(&dir, vec![(k.clone(), decision("exit 1"))], false);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        passing.iter().any(|(s, bad)| s.contains("passes") && !bad),
        "{passing:?}"
    );
    let run = stored_pass.and_then(|d| d.last_test).ok_or("no run kept")?;
    assert!(run.passed);
    assert_eq!(run.output, "ü two words");
    assert!(
        failing
            .iter()
            .any(|(s, bad)| *bad && s.contains("fails: last words")),
        "{failing:?}"
    );
    let run = stored_fail.and_then(|d| d.last_test).ok_or("no run kept")?;
    assert!(!run.passed);
    assert!(run.output.contains("first") && run.output.contains("last words"));
    assert_eq!(
        not_run,
        vec![("Kept the decision and its reason".to_owned(), false)]
    );
    Ok(())
}

/// Edited by hand: the text of each conflict is found again in the result; when it can't be,
/// nothing is kept and that is said.
#[test]
fn hand_edits_keep_their_decision_when_they_can_be_told_apart() -> Result<(), String> {
    let dir = repo("manual")?;
    let mut ed = editor(&dir, "f.rs", FILE)?;
    let _ = std::fs::remove_dir_all(&dir);
    ed.memo.drafts = vec![Draft {
        note: "merged by hand".into(),
        ..Draft::default()
    }];
    ed.result = "1\n2\n3\nmerged by hand\n4\n5\n6\n".into();
    ed.manual = true;
    let kept = ed.to_record("me", 7)?;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].1.resolution, "merged by hand\n");
    assert_eq!(kept[0].1.at, 7);
    ed.result = "edited around\n".into();
    assert!(
        ed.to_record("me", 7)
            .is_err_and(|e| e.contains("edited by hand"))
    );
    // Without notes, nothing to keep.
    ed.memo.drafts = vec![Draft::default()];
    assert_eq!(ed.to_record("me", 7), Ok(Vec::new()));
    Ok(())
}

/// Drawn: the decision made before shows its reason and test, a stale one asks to be used,
/// and "Note why…" opens the fields.
#[test]
fn the_editor_shows_the_decision_and_its_note() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    type State = (RepoTab, Vec<crate::repo::Action>, bool);
    let dir = repo("drawn")?;
    let mut ed = editor(&dir, "f.rs", FILE)?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    ed.memo.spots = ed.decision_spots();
    ed.take_offers(vec![Some(Decision {
        note: "keep the old wire format".into(),
        test: "cargo test wire".into(),
        resolution: "old wire format\n".into(),
        ours: "new validation\n".into(),
        theirs: "old wire format\n".into(),
        before: "elsewhere\n".into(),
        by: "Zoë".into(),
        ..Decision::default()
    })]);
    ed.memo.drafts[0] = Draft::default();
    tab.conflict = Some(ed);
    let draw = |ui: &mut egui::Ui, (tab, acts, fonts): &mut State| {
        if !*fonts {
            crate::theme::install_fonts(ui.ctx());
            *fonts = true;
            return;
        }
        tab.conflict_view(ui, acts);
    };
    let mut harness = egui_kittest::Harness::new_ui_state(draw, (tab, Vec::new(), false));
    harness.run_steps(2);
    harness.get_by_label_contains("the code around it changed since");
    harness.get_by_label("“keep the old wire format”");
    harness.get_by_label_contains("Test “cargo test wire”: not run yet");
    harness.get_by_label("Use it anyway").click();
    harness.run();
    harness.get_by_label("Note why…").click();
    harness.run();
    harness.get_by_label("Done");
    let ed = harness.state().0.conflict.as_ref().ok_or("no editor")?;
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert!(ed.memo.drafts[0].open);
    Ok(())
}
