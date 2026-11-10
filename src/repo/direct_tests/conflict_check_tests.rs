use super::conflict_labels::LISTED;
use super::*;
use crate::git::simulate::Stop;

fn file(path: &str, kinds: &[&str]) -> ConflictFile {
    ConflictFile {
        path: path.to_owned(),
        kinds: kinds.iter().map(|k| (*k).to_owned()).collect(),
    }
}

fn done(outcome: Outcome, commits: usize, mergiraf: Vec<Mergiraf>) -> Check {
    Check::Done(Arc::new(Verdict {
        prediction: Prediction { outcome, commits },
        mergiraf,
    }))
}

#[test]
fn labels_say_how_it_would_go() {
    let clean = done(Outcome::Clean { changes: true }, 0, Vec::new());
    assert_eq!(
        label(Op::Merge, &clean),
        Some(("applies cleanly".to_owned(), Tone::Good))
    );
    let nothing = done(Outcome::Clean { changes: false }, 1, Vec::new());
    assert_eq!(
        label(Op::CherryPick, &nothing),
        Some(("already applied".to_owned(), Tone::Faint))
    );
    assert_eq!(label(Op::Rebase, &nothing).map(|l| l.1), Some(Tone::Good));
    assert_eq!(
        label(Op::Merge, &nothing),
        Some(("already merged".to_owned(), Tone::Faint))
    );
    let three = done(
        Outcome::Conflicts {
            files: vec![file("a", &[]), file("b", &[]), file("c", &[])],
            stop: None,
        },
        0,
        Vec::new(),
    );
    assert_eq!(
        label(Op::Merge, &three),
        Some(("conflicts in 3 files".to_owned(), Tone::Warn))
    );
    let unsure = done(Outcome::Unsure("x".into()), 200, Vec::new());
    assert_eq!(
        label(Op::Rebase, &unsure),
        Some(("may conflict".to_owned(), Tone::Warn))
    );
    assert_eq!(label(Op::Merge, &Check::Unavailable("old".into())), None);
    assert_eq!(
        label(Op::Merge, &Check::Checking).map(|l| l.1),
        Some(Tone::Faint)
    );
}

#[test]
fn mergiraf_is_offered_for_text_conflicts_it_knows() {
    let patterns = vec!["*.rs".to_owned(), "*.json".to_owned()];
    let known = Some(patterns.as_slice());
    assert_eq!(
        mergiraf_hint(&file("src/a.rs", &["contents"]), known),
        Mergiraf::Likely
    );
    assert_eq!(
        mergiraf_hint(&file("notes.txt", &["contents"]), known),
        Mergiraf::UnknownLanguage
    );
    assert_eq!(
        mergiraf_hint(&file("img.rs", &["binary", "contents"]), known),
        Mergiraf::NotText("binary file".to_owned())
    );
    assert_eq!(
        mergiraf_hint(&file("gone.rs", &["modify/delete"]), known),
        Mergiraf::NotText("changed on one side, deleted on the other".to_owned())
    );
    assert_eq!(
        mergiraf_hint(&file("a.rs", &["contents"]), None),
        Mergiraf::NotInstalled
    );
}

/// A rebase that stops says at which commit, lists the files with Mergiraf's chances,
/// and that what is after it wasn't checked.
#[test]
fn a_rebase_tooltip_names_the_commit_and_the_files() {
    let stop = Stop {
        index: 3,
        commit: simulate::Pick {
            hash: "0123456789abcdef".into(),
            parent: Some("p".into()),
            summary: "Third".into(),
        },
    };
    let check = done(
        Outcome::Conflicts {
            files: vec![file("src/a.rs", &["contents"]), file("b.png", &["binary"])],
            stop: Some(stop),
        },
        5,
        vec![Mergiraf::Likely, Mergiraf::NotText("binary".into())],
    );
    let text = tooltip(Op::Rebase, &check, true);
    assert!(
        text.starts_with("Commit 3 of 5, 01234567 “Third”, conflicts in 2 files:"),
        "{text}"
    );
    assert!(
        text.contains("• src/a.rs — Mergiraf can likely solve it"),
        "{text}"
    );
    assert!(text.contains("• b.png — binary: solve by hand"), "{text}");
    assert!(text.contains("aren't checked"), "{text}");
    assert!(text.contains("Uncommitted changes"), "{text}");
    assert!(!text.contains("Settings → Git"), "{text}");
}

#[test]
fn long_lists_are_cut_and_a_missing_mergiraf_is_mentioned() {
    let files: Vec<ConflictFile> = (0..20)
        .map(|n| file(&format!("f{n}.rs"), &["contents"]))
        .collect();
    let check = done(
        Outcome::Conflicts { files, stop: None },
        0,
        vec![Mergiraf::NotInstalled; 20],
    );
    let text = tooltip(Op::Merge, &check, false);
    assert!(text.starts_with("Conflicts in 20 files:"), "{text}");
    assert_eq!(text.matches("\n• ").count(), LISTED);
    assert!(text.contains("…and 5 more"), "{text}");
    assert!(text.contains("Settings → Git → Mergiraf"), "{text}");
    assert!(!text.contains("Uncommitted"), "{text}");
}

/// Mergiraf found but unable to list its languages says so, with why, instead of
/// suggesting to install it.
#[test]
fn a_failed_language_query_is_shown_with_its_error() {
    let check = done(
        Outcome::Conflicts {
            files: vec![file("a.rs", &["contents"])],
            stop: None,
        },
        0,
        vec![Mergiraf::Unchecked("Mergiraf: bad flag".into())],
    );
    let text = tooltip(Op::Merge, &check, false);
    assert!(text.contains("• a.rs — text conflict"), "{text}");
    assert!(
        text.contains("couldn't list the file types it knows: Mergiraf: bad flag"),
        "{text}"
    );
    assert!(!text.contains("installs it"), "{text}");
}

/// The cache answers again without asking git, keeps what is asked for often, and drops
/// the oldest beyond its size, but never a check still running.
#[test]
fn the_cache_keeps_recent_checks_and_drops_old_ones() {
    let mut inner = Inner::default();
    let key = |n: usize| Key::new(Op::Merge, "head", &format!("c{n}"));
    let clean = done(Outcome::Clean { changes: true }, 0, Vec::new());
    let entry = |asked: u64, check: Check| Entry {
        asked,
        check,
        failed: None,
    };
    inner.entries.insert(key(0), entry(0, Check::Checking));
    for n in 1..=CAPACITY + 10 {
        inner.entries.insert(key(n), entry(n as u64, clean.clone()));
    }
    // The first finished one was asked for again, late.
    if let Some(entry) = inner.entries.get_mut(&key(1)) {
        entry.asked = 10_000;
    }
    inner.evict();
    assert_eq!(inner.entries.len(), CAPACITY);
    assert!(inner.entries.contains_key(&key(0)), "running");
    assert!(inner.entries.contains_key(&key(1)), "asked for again");
    for n in 2..=12 {
        assert!(!inner.entries.contains_key(&key(n)), "{n}");
    }
    assert!(inner.entries.contains_key(&key(13)));
}

/// A failed check (a machine out of reach) is asked again once [`RETRY`] has passed, and
/// a check about a HEAD the branch has moved on from is dropped from the queue.
#[test]
fn failures_are_retried_and_stale_questions_dropped() {
    let checks = ConflictChecks::default();
    let ctx = egui::Context::default();
    let failed = Key::new(Op::Merge, "head", "x");
    let stale = Key::new(Op::Merge, "old-head", "y");
    let Some(long_ago) = Instant::now().checked_sub(RETRY * 2) else {
        return;
    };
    if let Ok(mut inner) = checks.inner.lock() {
        // No worker starts: the test looks at the queue.
        inner.workers = WORKERS;
        let entry = |check: Check, failed: Option<Instant>| Entry {
            asked: 0,
            check,
            failed,
        };
        let gone = Check::Failed("unreachable".into());
        inner
            .entries
            .insert(failed.clone(), entry(gone.clone(), Some(Instant::now())));
        inner
            .entries
            .insert(stale.clone(), entry(Check::Checking, None));
        inner.queue.push(stale.clone());
        // Just failed: the failure stands.
        drop(inner);
        assert!(matches!(
            checks.get(&ctx, Path::new("."), failed.clone()),
            Check::Failed(_)
        ));
        if let Ok(mut inner) = checks.inner.lock()
            && let Some(e) = inner.entries.get_mut(&failed)
        {
            e.failed = Some(long_ago);
        }
    }
    // Long enough ago: asked again; and the old HEAD's question is gone.
    assert!(matches!(
        checks.get(&ctx, Path::new("."), failed.clone()),
        Check::Checking
    ));
    let state = checks
        .inner
        .lock()
        .map(|inner| (inner.queue.clone(), inner.entries.contains_key(&stale)))
        .ok();
    assert_eq!(state, Some((vec![failed], false)));
    // A closed tab leaves its workers nothing to do.
    let inner = checks.inner.clone();
    drop(checks);
    assert_eq!(inner.lock().map(|i| i.queue.len()).ok(), Some(0));
}

/// The queue keeps the latest questions only, and a check that was running when the
/// checks were forgotten (F5) doesn't come back with its old answer.
#[test]
fn the_queue_is_bounded_and_forgetting_drops_running_work() {
    let mut inner = Inner::default();
    let key = |n: usize| Key::new(Op::Merge, "head", &format!("c{n}"));
    for n in 0..MAX_QUEUE + 5 {
        inner.queue.push(key(n));
        let entry = Entry {
            asked: n as u64,
            check: Check::Checking,
            failed: None,
        };
        inner.entries.insert(key(n), entry);
    }
    inner.bound_queue();
    assert_eq!(inner.queue.len(), MAX_QUEUE);
    assert_eq!(inner.queue.first(), Some(&key(5)));
    assert!(!inner.entries.contains_key(&key(4)));
    let checks = ConflictChecks::default();
    if let Ok(mut shared) = checks.inner.lock() {
        *shared = inner;
    }
    let before = checks.inner.lock().map(|i| i.generation).ok();
    checks.forget();
    let after = checks
        .inner
        .lock()
        .map(|i| (i.generation, i.entries.len(), i.queue.len()))
        .ok();
    assert_eq!(after, before.map(|g| (g + 1, 0, 0)));
}

/// Through the real thing: a check starts, finishes in the background, and is answered
/// from the cache after; a key it has is not checked again.
#[test]
fn checks_run_in_the_background_and_are_kept() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-checks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| crate::git::run(Some(&dir), args).map(|s| s.trim().to_owned());
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["commit", "-q", "--allow-empty", "-m", "two"],
    ] {
        git(args)?;
    }
    let head = git(&["rev-parse", "HEAD"])?;
    let base = git(&["rev-parse", "HEAD~1"])?;
    let ctx = egui::Context::default();
    let checks = ConflictChecks::default();
    // Asked about before a commit moved HEAD on, and never taken by a worker.
    let stale = Key::new(Op::Merge, "an-older-head", &base);
    if let Ok(mut inner) = checks.inner.lock() {
        inner.workers = WORKERS;
        inner.queue.push(stale.clone());
        let entry = Entry {
            asked: 0,
            check: Check::Checking,
            failed: None,
        };
        inner.entries.insert(stale.clone(), entry);
    }
    let key = Key::new(Op::Merge, &head, &base);
    let first = checks.get(&ctx, &dir, key.clone());
    let dropped = checks
        .inner
        .lock()
        .map(|mut inner| {
            inner.workers = 0;
            (inner.queue.clone(), inner.entries.contains_key(&stale))
        })
        .map_err(|e| e.to_string())?;
    assert_eq!(dropped, (vec![key.clone()], false));
    // The worker this test held back.
    if let Ok(mut inner) = checks.inner.lock() {
        inner.queue.clear();
        inner.entries.clear();
    }
    let first_again = checks.get(&ctx, &dir, key.clone());
    let started = Instant::now();
    let answer = loop {
        match checks.get(&ctx, &dir, key.clone()) {
            Check::Checking if started.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            other => break other,
        }
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(first, Check::Checking));
    assert!(matches!(first_again, Check::Checking));
    match answer {
        Check::Done(verdict) => {
            assert_eq!(
                verdict.prediction.outcome,
                Outcome::Clean { changes: false }
            );
        }
        // A git too old to merge-tree says so instead.
        Check::Unavailable(_) => {}
        other => return Err(format!("{other:?}")),
    }
    // Answered from the cache: the repository is gone, and still the same answer.
    assert!(!matches!(checks.get(&ctx, &dir, key), Check::Checking));
    Ok(())
}
