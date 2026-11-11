use super::*;

#[test]
fn dropped_patches_are_applied() {
    use crate::repo::RepoTab;
    let dir = repo("dropped patch");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let file = dir.join("notes ü.txt");
    let _ = std::fs::write(&file, "one\n");
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-q", "-m", "notes"]).is_ok());
    // A diff, made and then undone, to apply again.
    let _ = std::fs::write(&file, "one\ntwo\n");
    let diff = git::run(Some(&dir), &["diff"]).unwrap_or_default();
    assert!(git::run(Some(&dir), &["checkout", "--", "."]).is_ok());
    let patches = std::env::temp_dir().join(format!("gitcraft-drop é {}", std::process::id()));
    let _ = std::fs::create_dir_all(&patches);
    let diff_file = patches.join("change.DIFF");
    let _ = std::fs::write(&diff_file, &diff);
    // Something else on the status line already says it failed: no matter.
    tab.status = Some(crate::repo::Notice::plain((
        "Fetch failed: offline".to_owned(),
        true,
    )));
    let given = tab.apply_dropped_patch(diff_file.clone(), &ctx);
    // A second one while the first is applied is refused, not lost silently.
    let second = tab.apply_dropped_patch(diff_file.clone(), &ctx);
    let diff_outcome = dropped_outcome(&mut tab, &ctx);
    // Git on Windows may check files out with CRLF (core.autocrlf): compare the lines.
    let applied = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    // Applied twice: it doesn't apply any more.
    let again = tab.apply_dropped_patch(diff_file, &ctx);
    let again_outcome = dropped_outcome(&mut tab, &ctx);
    // A mail: a commit, made and taken back, to apply again.
    assert!(git::run(Some(&dir), &["commit", "-q", "-am", "two lines"]).is_ok());
    let mail = git::run_bytes(Some(&dir), &["format-patch", "-1", "--stdout"]).unwrap_or_default();
    assert!(git::run(Some(&dir), &["reset", "-q", "--hard", "HEAD~1"]).is_ok());
    let mail_file = patches.join("0001-two lines.patch");
    let _ = std::fs::write(&mail_file, mail);
    let _ = tab.apply_dropped_patch(mail_file, &ctx);
    let mail_outcome = dropped_outcome(&mut tab, &ctx);
    let subject = git::run(Some(&dir), &["log", "-1", "--format=%s"]).unwrap_or_default();
    // Gone before it was applied.
    let _ = tab.apply_dropped_patch(patches.join("gone.patch"), &ctx);
    let gone_outcome = tab.take_dropped_patch_outcome();
    let waits_after = tab.dropped_patch_waits();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&patches);
    assert_eq!(given, Ok(()));
    assert!(second.is_err());
    assert_eq!(diff_outcome, Some(Ok(())));
    assert_eq!(applied, "one\ntwo\n");
    assert_eq!(again, Ok(()));
    assert!(matches!(again_outcome, Some(Err(_))), "{again_outcome:?}");
    assert_eq!(mail_outcome, Some(Ok(())));
    assert_eq!(subject.trim(), "two lines");
    assert!(matches!(gone_outcome, Some(Err(e)) if e.contains("gone.patch")));
    assert!(!waits_after);
}

/// Closing a tab cancels its in-flight local command, including its descendants.
#[test]
fn closing_a_tab_stops_its_background_child() -> Result<(), String> {
    let dir = repo("close-process");
    let ctx = egui::Context::default();
    let tab = crate::repo::RepoTab::new_deferred(dir.clone(), &ctx);
    let marker = dir.join("started");
    let effect = dir.join("survived");
    let descendant = format!(
        "printf started > {}; sleep 1; printf survived > {}",
        crate::remote::sh_quote(&marker.to_string_lossy()),
        crate::remote::sh_quote(&effect.to_string_lossy())
    );
    let script = format!("sh -c {} & wait", crate::remote::sh_quote(&descendant));
    let (done, result) = std::sync::mpsc::channel();
    let path = dir.clone();
    crate::repo::spawn(&tab.tx, &ctx, move || {
        let ran = git::run_shell(&path, &script).map(|_| ());
        let _ = done.send(ran);
        crate::repo::Msg::Then(Box::new(|_, _| {}))
    });
    let start = std::time::Instant::now();
    while !marker.exists() {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return Err("child did not start".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    drop(tab);
    let ran = result
        .recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    assert_eq!(ran, Err("cancelled".into()));
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let survived = effect.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        !survived,
        "the cancelled descendant performed its delayed write"
    );
    Ok(())
}

/// Shared refs and stashes are serialized even before either worktree starts loading.
#[test]
fn linked_worktrees_share_a_mutation_key_before_the_first_load() -> Result<(), String> {
    let dir = repo("shared-mutation-key");
    let linked = dir.join("linked");
    git::run(
        Some(&dir),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            &linked.to_string_lossy(),
        ],
    )?;
    assert_eq!(
        crate::tasks::mutation_schedule_key(crate::repo::messages::mutation_key(&dir)),
        crate::tasks::mutation_schedule_key(crate::repo::messages::mutation_key(&linked))
    );
    // Ownership is refined by the same background discovery used by reloads,
    // independently of the conservative scheduling bucket.
    crate::repo::messages::discover_fetch_key(&dir);
    crate::repo::messages::discover_fetch_key(&linked);
    assert_eq!(
        crate::repo::messages::fetch_key(&dir),
        crate::repo::messages::fetch_key(&linked)
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn feature_failure_continuation_runs_when_a_queued_job_is_cancelled() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("gitcraft-cancelled-completion".into(), &ctx);
    tab.tx.cancel.cancel();
    tab.then_or(
        &ctx,
        false,
        Err,
        || Ok::<_, String>(()),
        |tab, found, _| {
            tab.status = Some(crate::repo::Notice::plain((
                found.err().unwrap_or_else(|| "cancelled work ran".into()),
                true,
            )));
        },
    );
    let start = std::time::Instant::now();
    while tab.status.is_none() && start.elapsed() < std::time::Duration::from_secs(5) {
        tab.poll(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        tab.status.as_deref().cloned(),
        Some(("cancelled".into(), true))
    );
    Ok(())
}

/// Repeated reload requests coalesce while the first read runs; no thread per request.
#[test]
fn rapid_reloads_finish_and_coalesce() {
    let dir = repo("rapid-reloads");
    let ctx = egui::Context::default();
    // Drive reloads without an OS watcher: its arrival starts independent status
    // and submodule reads, which share `reads` but do not measure coalescing.
    let mut tab = crate::repo::RepoTab::new_deferred(dir.clone(), &ctx);
    tab.life.shown(std::time::Instant::now());
    tab.reload(&ctx);
    let settle = |tab: &mut crate::repo::RepoTab| {
        let start = std::time::Instant::now();
        while tab.loading || tab.reload_again {
            tab.poll(&ctx);
            assert!(start.elapsed() < std::time::Duration::from_secs(20));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    settle(&mut tab);
    let before = tab.reads;
    for _ in 0..100 {
        tab.reload(&ctx);
    }
    assert_eq!(tab.reads, before + 1);
    assert!(tab.reload_again);
    settle(&mut tab);
    assert!(!tab.loading);
    assert!(!tab.reload_again);
    assert!(tab.load_error.is_none());
    assert!(tab.history.iter().any(|commit| commit.summary == "base"));
    assert_eq!(tab.reads, before + 2);
    drop(tab);
    let _ = std::fs::remove_dir_all(dir);
}
