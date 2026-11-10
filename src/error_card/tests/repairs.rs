// Repairs. Included to preserve libtest shard names.
/// A repository with one commit, in a fresh folder named after `name`, with a space and
/// letters outside ASCII in its path.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft card ä-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        run(Some(&dir), args)?;
    }
    Ok(dir)
}

/// Waits (a while at most) for the card's background work.
fn settle(card: &mut ErrorCard) {
    let start = Instant::now();
    while card.rx.is_some() && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(20));
        card.poll();
    }
    assert!(card.rx.is_none(), "still working");
}

/// The way the tab takes it: git fails on a lock, the card says how old it is and whether git
/// runs, and, confirmed, removes it.
#[test]
fn the_card_removes_a_stale_lock() -> Result<(), String> {
    let dir = repo("lock")?;
    std::fs::write(dir.join("a b ü.txt"), "a\n").map_err(|e| e.to_string())?;
    let lock = dir.join(".git/index.lock");
    std::fs::write(&lock, "").map_err(|e| e.to_string())?;
    let err = failure(run(Some(&dir), &["add", "-A"]), "locked")?;
    let ctx = egui::Context::default();
    let mut card = ErrorCard::for_error(&err, &dir, "Stage all", &ctx).ok_or("known")?;
    assert!(matches!(card.problem(), GitProblem::StaleLock { .. }));
    // Nothing happens before it is found and confirmed.
    card.confirm(&ctx);
    assert!(lock.exists());
    settle(&mut card);
    let found = card.found.clone().ok_or("found")??;
    assert!(
        found.notes[0].text.contains("index.lock was last written"),
        "{:?}",
        found.notes
    );
    assert!(found.notes[1].text.contains("git"), "{:?}", found.notes);
    assert_eq!(card.offered().map(Fix::button), Some("Remove stale lock"));
    card.ask();
    assert_eq!(card.stage(), &Stage::Confirming);
    card.back(&ctx);
    assert_eq!(card.stage(), &Stage::Idle);
    assert!(lock.exists());
    card.ask();
    card.confirm(&ctx);
    assert_eq!(card.stage(), &Stage::Working);
    settle(&mut card);
    assert!(
        matches!(card.stage(), Stage::Done { undo: None, what } if what.starts_with("Removed ")),
        "{:?}",
        card.stage()
    );
    // Nothing to undo: the card isn't kept for it.
    assert!(!card.fixed());
    assert!(!lock.exists());
    run(Some(&dir), &["add", "-A"])?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A lock gone by the time the card looks: nothing to remove.
#[test]
fn a_lock_already_gone_offers_nothing() -> Result<(), String> {
    let dir = repo("lock-gone")?;
    let lock = dir.join(".git/index.lock");
    std::fs::write(&lock, "").map_err(|e| e.to_string())?;
    let err = failure(run(Some(&dir), &["add", "-A"]), "locked")?;
    std::fs::remove_file(&lock).map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    let mut card = ErrorCard::for_error(&err, &dir, "Stage all", &ctx).ok_or("known")?;
    settle(&mut card);
    assert!(card.offered().is_none());
    card.ask();
    assert_eq!(card.stage(), &Stage::Idle);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Git for Windows' long paths, on a real repository: set in its own config, and undone.
#[test]
fn long_paths_are_enabled_and_undone() -> Result<(), String> {
    let dir = repo("longpaths")?;
    let found = fix::gather(&GitProblem::NameTooLong, &dir, WINDOWS)?;
    let fix = found.fix.ok_or("a fix")?;
    assert_eq!(fix.button(), "Enable long paths");
    assert!(
        fix.confirm().contains(".git/config (it isn't set now)"),
        "{}",
        fix.confirm()
    );
    let (what, undo) = fix.apply(&dir)?;
    assert!(what.starts_with("Set core.longpaths = true in "), "{what}");
    assert_eq!(
        run(Some(&dir), &["config", "--local", "core.longpaths"])?.trim(),
        "true"
    );
    // Set already: no fix offered again.
    let again = fix::gather(&GitProblem::NameTooLong, &dir, WINDOWS)?;
    assert!(again.fix.is_none());
    assert!(again.notes[0].warn);
    let undone = undo.ok_or("undoable")?.undo(&dir)?;
    assert!(undone.starts_with("Unset core.longpaths"), "{undone}");
    assert!(run(Some(&dir), &["config", "--local", "core.longpaths"]).is_err());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Trusting a folder adds it to the file named, and undoing takes it out (a file of the test's
/// own: the user's global config is never touched by tests).
#[test]
fn trusting_a_folder_and_taking_it_back() -> Result<(), String> {
    let dir = repo("trust")?;
    let file = dir.join("global.gitconfig");
    let trusted = format!("{}/x ü", dir.display());
    let fix = Fix::Config {
        edit: ConfigEdit::add(Scope::File(file.clone()), "safe.directory", &trusted),
        file: file.display().to_string(),
    };
    let (what, undo) = fix.apply(&dir)?;
    assert_eq!(
        what,
        format!("Added safe.directory = {trusted} to {}.", file.display())
    );
    let listed = |dir: &Path| {
        crate::git::errors::fix::get_all(dir, &Scope::File(file.clone()), "safe.directory")
    };
    assert_eq!(listed(&dir)?, std::slice::from_ref(&trusted));
    let undone = undo.ok_or("undoable")?.undo(&dir)?;
    assert!(undone.starts_with("Took safe.directory"), "{undone}");
    assert!(listed(&dir)?.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// The line-ending facts of a real repository: its core.autocrlf and the file's attributes.
#[test]
fn line_ending_facts() -> Result<(), String> {
    let dir = repo("crlf")?;
    run(Some(&dir), &["config", "core.autocrlf", "true"])?;
    std::fs::write(dir.join(".gitattributes"), "*.md text eol=lf\n").map_err(|e| e.to_string())?;
    let problem = GitProblem::LineEndings {
        files: vec!["read me.md".to_owned()],
        to_crlf: true,
        fatal: false,
    };
    let found = fix::gather(&problem, &dir, HERE)?;
    assert!(found.fix.is_none());
    let notes: Vec<&str> = found.notes.iter().map(|n| n.text.as_str()).collect();
    assert!(
        notes[0].starts_with("core.autocrlf is true here (set in "),
        "{notes:?}"
    );
    assert!(notes[0].contains(".git/config"), "{notes:?}");
    assert_eq!(notes[1], ".gitattributes gives read me.md: text, eol=lf.");
    run(Some(&dir), &["config", "--unset", "core.autocrlf"])?;
    let other = GitProblem::LineEndings {
        files: vec!["a.txt".to_owned()],
        to_crlf: true,
        fatal: false,
    };
    let found = fix::gather(&other, &dir, HERE)?;
    assert_eq!(found.notes[1].text, "No .gitattributes rule covers a.txt.");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// The whole way for a folder owned by another user, where the test can make one (as root):
/// opening it fails, the error is kept (not "not a repository"), explained, and once the
/// folder is trusted git opens it.
#[cfg(unix)]
#[test]
fn a_folder_owned_by_someone_else() -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let dir = repo("dubious")?;
    let mine = std::fs::metadata(&dir).map_err(|e| e.to_string())?.uid();
    if mine != 0 {
        // Only root can give a folder away.
        let _ = std::fs::remove_dir_all(&dir);
        return Ok(());
    }
    let chown = |path: &Path| std::os::unix::fs::chown(path, Some(4242), Some(4242));
    chown(&dir).map_err(|e| e.to_string())?;
    chown(&dir.join(".git")).map_err(|e| e.to_string())?;
    let err = failure(crate::git::toplevel(&dir), "refused")?;
    let Some(GitProblem::DubiousOwnership { path }) = crate::git::errors::explain(&err) else {
        return Err(format!("not explained: {err}"));
    };
    assert_eq!(Path::new(&path), dir.as_path());
    let file =
        std::env::temp_dir().join(format!("gitcraft-trust-{}.gitconfig", std::process::id()));
    let _ = std::fs::remove_file(&file);
    let fix = Fix::Config {
        edit: ConfigEdit::add(Scope::File(file.clone()), "safe.directory", &path),
        file: file.display().to_string(),
    };
    fix.apply(&dir)?;
    // git, reading that file as its global config, takes the folder now.
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["rev-parse", "--show-toplevel"])
        .env("GIT_CONFIG_GLOBAL", &file)
        .output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// A lock written again after the card looked: removing it fails, and going back looks again,
/// so that the fix is offered for the lock as it is now.
#[test]
fn a_lock_written_again_is_looked_at_again() -> Result<(), String> {
    let dir = repo("lock-rewritten")?;
    let lock = dir.join(".git/index.lock");
    std::fs::write(&lock, "").map_err(|e| e.to_string())?;
    let err = failure(run(Some(&dir), &["add", "-A"]), "locked")?;
    let ctx = egui::Context::default();
    let mut card = ErrorCard::for_error(&err, &dir, "Stage all", &ctx).ok_or("known")?;
    settle(&mut card);
    // As if a git wrote it since.
    if let Some(Ok(found)) = &mut card.found
        && let Some(Fix::RemoveLock(seen)) = &mut found.fix
    {
        seen.modified -= 100;
    }
    card.ask();
    card.confirm(&ctx);
    settle(&mut card);
    assert!(matches!(card.stage(), Stage::Failed(e) if e.contains("written again")));
    assert!(lock.exists());
    card.back(&ctx);
    assert!(card.found.is_none(), "looks again");
    settle(&mut card);
    card.ask();
    card.confirm(&ctx);
    settle(&mut card);
    assert!(
        matches!(card.stage(), Stage::Done { .. }),
        "{:?}",
        card.stage()
    );
    assert!(!lock.exists());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// The same failure again keeps the card; Try again is offered once per failure.
#[test]
fn the_same_failure_keeps_its_card() -> Result<(), String> {
    let dir = repo("same-again")?;
    let err = "fatal: refusing to merge unrelated histories";
    let ctx = egui::Context::default();
    let mut card = ErrorCard::for_error(err, &dir, "Merge", &ctx).ok_or("known")?;
    assert!(card.is_about(err));
    assert!(!card.is_about("fatal: early EOF"));
    assert!(!card.is_about("something else"));
    assert!(card.can_retry());
    card.retried();
    assert!(!card.can_retry());
    card.failed_again();
    assert!(card.can_retry());
    settle(&mut card);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A card that replaces one whose fix was made keeps that fix's undo; a failed undo can be
/// tried again.
#[test]
fn an_undo_is_never_lost() -> Result<(), String> {
    let dir = repo("undo-kept")?;
    let ctx = egui::Context::default();
    let fix = Fix::Config {
        edit: ConfigEdit::set(&dir, Scope::Repo, "core.longpaths", "true")?,
        file: "the repository's config".to_owned(),
    };
    let mut first =
        ErrorCard::for_error("error: x: Filename too long", &dir, "Pull", &ctx).ok_or("known")?;
    settle(&mut first);
    let (what, undo) = fix.apply(&dir)?;
    first.stage = Stage::Done { what, undo };
    assert!(first.fixed());
    let mut second = ErrorCard::for_error("fatal: early EOF", &dir, "Pull", &ctx)
        .ok_or("known")?
        .replacing(Some(first));
    assert!(second.fixed());
    assert!(
        second
            .earlier()
            .is_some_and(|w| w.contains("core.longpaths"))
    );
    second.undo_earlier(&ctx);
    let start = Instant::now();
    while second.earlier_undone().is_none() && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(20));
        second.poll();
    }
    assert!(matches!(second.earlier_undone(), Some(Ok(_))));
    assert!(!second.fixed());
    assert!(run(Some(&dir), &["config", "--local", "core.longpaths"]).is_err());
    settle(&mut second);

    // A lock's removal has no undo: such a card isn't kept for one.
    let removed = Stage::Done {
        what: "Removed x.".to_owned(),
        undo: None,
    };
    let mut third = ErrorCard::for_error("fatal: early EOF", &dir, "Pull", &ctx).ok_or("known")?;
    third.stage = removed;
    assert!(!third.fixed());
    settle(&mut third);

    // An undo that fails keeps its undo, to try again.
    let gone = dir.join("a folder, not a file");
    std::fs::create_dir_all(&gone).map_err(|e| e.to_string())?;
    let broken = Fix::Config {
        edit: ConfigEdit::add(Scope::File(gone.clone()), "safe.directory", "/x"),
        file: gone.display().to_string(),
    };
    let mut card = ErrorCard::for_error("fatal: early EOF", &dir, "Pull", &ctx).ok_or("known")?;
    settle(&mut card);
    card.stage = Stage::Done {
        what: String::new(),
        undo: Some(broken),
    };
    card.undo(&ctx);
    settle(&mut card);
    assert!(
        matches!(card.stage(), Stage::Done { what, undo: Some(_) } if what.starts_with("Undo failed")),
        "{:?}",
        card.stage()
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
