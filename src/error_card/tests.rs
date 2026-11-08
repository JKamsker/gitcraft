//! The card's words, the fix each problem gets, and the card's whole way on real repositories:
//! looked at in the background, confirmed, made and undone.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::fix::{self, Fix, Plan, attribute_values, origin_and_value};
use super::text::{self, Place};
use super::{ErrorCard, Stage, cut};
use crate::git::errors::GitProblem;
use crate::git::errors::fix::{ConfigEdit, Scope};
use crate::git::run;

/// The error of `r`, which should have failed (`what`).
fn failure<T: std::fmt::Debug>(r: Result<T, String>, what: &str) -> Result<String, String> {
    match r {
        Ok(v) => Err(format!("{what} worked: {v:?}")),
        Err(e) => Ok(e),
    }
}

const HERE: Place = Place {
    windows: false,
    remote: false,
};
const WINDOWS: Place = Place {
    windows: true,
    remote: false,
};
const WSL: Place = Place {
    windows: true,
    remote: true,
};

fn every_problem() -> Vec<GitProblem> {
    vec![
        GitProblem::DubiousOwnership {
            path: "/srv/r".to_owned(),
        },
        GitProblem::StaleLock {
            lock: "/srv/r/.git/index.lock".to_owned(),
        },
        GitProblem::LineEndings {
            files: vec!["a".to_owned()],
            to_crlf: true,
            fatal: false,
        },
        GitProblem::NameTooLong,
        GitProblem::Certificate {
            detail: "x".to_owned(),
        },
        GitProblem::Transfer { too_large: true },
        GitProblem::Transfer { too_large: false },
        GitProblem::UnrelatedHistories,
        GitProblem::ShallowPush,
        GitProblem::SshKeyRefused { login: None },
    ]
}

#[test]
fn every_problem_is_explained() {
    for problem in every_problem() {
        for place in [HERE, WINDOWS, WSL] {
            assert!(!text::title(&problem).is_empty());
            let said = text::explanation(&problem, place);
            assert!(said.iter().all(|p| !p.trim().is_empty()), "{problem:?}");
            assert!(!said.is_empty(), "{problem:?}");
        }
    }
}

#[test]
fn the_explanation_names_what_it_is_about() {
    let trust = text::explanation(
        &GitProblem::DubiousOwnership {
            path: "/srv/dir with space/ü".to_owned(),
        },
        HERE,
    );
    assert!(trust[0].starts_with("/srv/dir with space/ü"));
    assert!(trust[1].contains("only this one"));
    let lock = text::explanation(
        &GitProblem::StaleLock {
            lock: "C:\\r\\.git\\index.lock".to_owned(),
        },
        WINDOWS,
    );
    assert!(lock[0].contains("git keeps index.lock"), "{lock:?}");
    let files: Vec<String> = (0..7).map(|i| format!("f{i}")).collect();
    let crlf = text::explanation(
        &GitProblem::LineEndings {
            files,
            to_crlf: false,
            fatal: true,
        },
        HERE,
    );
    assert_eq!(crlf[0], "Files: f0, f1, f2 and 4 more.");
    assert!(crlf.iter().any(|p| p.contains("core.safecrlf")));
    // Long paths and schannel are Git for Windows' alone.
    let long = |place| text::explanation(&GitProblem::NameTooLong, place).join(" ");
    assert!(long(WINDOWS).contains("core.longpaths"));
    assert!(!long(HERE).contains("core.longpaths"));
    assert!(!long(WSL).contains("core.longpaths"));
    let cert = |place| {
        text::explanation(
            &GitProblem::Certificate {
                detail: "d".to_owned(),
            },
            place,
        )
        .join(" ")
    };
    assert!(cert(WINDOWS).contains("schannel"));
    assert!(!cert(WSL).contains("schannel"));
    let ssh = |place| {
        text::explanation(
            &GitProblem::SshKeyRefused {
                login: Some("git@example.org".to_owned()),
            },
            place,
        )
    };
    assert!(ssh(HERE)[0].contains("ssh -T git@example.org"));
    assert_eq!(ssh(WSL).len(), 2);
}

#[test]
fn which_problems_get_a_fix() {
    let plan = |problem: &GitProblem, place| fix::plan(problem, place);
    let lock = GitProblem::StaleLock {
        lock: "/r/.git/index.lock".to_owned(),
    };
    assert_eq!(
        plan(&lock, WSL),
        Plan::RemoveLock("/r/.git/index.lock".to_owned())
    );
    assert_eq!(plan(&GitProblem::NameTooLong, WINDOWS), Plan::LongPaths);
    assert_eq!(plan(&GitProblem::NameTooLong, HERE), Plan::Explain);
    assert_eq!(plan(&GitProblem::NameTooLong, WSL), Plan::Explain);
    let cert = GitProblem::Certificate {
        detail: String::new(),
    };
    assert_eq!(plan(&cert, WINDOWS), Plan::WindowsCertificates);
    assert_eq!(plan(&cert, HERE), Plan::Explain);
    for problem in [
        GitProblem::Transfer { too_large: true },
        GitProblem::UnrelatedHistories,
        GitProblem::ShallowPush,
        GitProblem::SshKeyRefused { login: None },
    ] {
        assert_eq!(plan(&problem, WINDOWS), Plan::Explain, "{problem:?}");
    }
}

#[test]
fn the_confirmation_says_which_file_and_what() {
    let add = Fix::Config {
        edit: ConfigEdit::add(Scope::Global, "safe.directory", "/srv/r"),
        file: "/home/me/.gitconfig".to_owned(),
    };
    assert_eq!(add.button(), "Trust this folder");
    assert_eq!(
        add.confirm(),
        "Adds safe.directory = /srv/r to /home/me/.gitconfig. Undo, on this card, takes it out again."
    );
    let set = Fix::Config {
        edit: ConfigEdit {
            scope: Scope::Global,
            key: "http.sslBackend",
            value: "schannel".to_owned(),
            add: false,
            previous: Some("openssl".to_owned()),
        },
        file: "C:/Users/me/.gitconfig".to_owned(),
    };
    assert_eq!(set.button(), "Use Windows' certificates");
    assert!(
        set.confirm()
            .contains("in C:/Users/me/.gitconfig (it is openssl now)")
    );
    let lock = Fix::RemoveLock(crate::git::errors::lock::LockFile {
        shown: "/r/.git/index.lock".to_owned(),
        modified: 0,
        age: 7200,
    });
    assert_eq!(lock.button(), "Remove stale lock");
    assert!(
        lock.confirm()
            .contains("/r/.git/index.lock, last written 2 hours ago")
    );
    assert!(lock.confirm().contains("no undo"));
}

#[test]
fn ages_and_names() {
    assert_eq!(text::age(5), "less than a minute");
    assert_eq!(text::age(120), "2 minutes");
    assert_eq!(text::age(86_400 * 3), "3 days");
    assert_eq!(text::age(u64::MAX), text::age(i64::MAX as u64));
    assert_eq!(text::file_name("C:\\r\\.git\\index.lock"), "index.lock");
    assert_eq!(text::file_name("index.lock"), "index.lock");
}

#[test]
fn reads_what_git_config_and_check_attr_print() {
    assert_eq!(
        origin_and_value("file:/home/me/.gitconfig\0true\0"),
        Some(("/home/me/.gitconfig".to_owned(), "true".to_owned()))
    );
    assert_eq!(
        origin_and_value("file:C:/Program Files/Git/etc/gitconfig\0input\n\0"),
        Some((
            "C:/Program Files/Git/etc/gitconfig".to_owned(),
            "input".to_owned()
        ))
    );
    assert_eq!(origin_and_value(""), None);
    assert_eq!(
        attribute_values("a b.txt\0text\0set\0a b.txt\0eol\0lf\0"),
        ["text", "eol=lf"]
    );
    assert_eq!(
        attribute_values("x\0text\0unset\0x\0eol\0unspecified\0"),
        ["-text"]
    );
    assert!(attribute_values("").is_empty());
}

#[test]
fn git_output_is_cut_on_a_character() {
    let long = "é".repeat(5000);
    let kept = cut(&long, 4001);
    assert!(kept.ends_with('…'));
    assert!(kept.len() <= 4001 + '…'.len_utf8());
    assert_eq!(cut("short", 10), "short");
    assert_eq!(cut("", 10), "");
}

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

/// Constructs inert state directly: no for_error, worker, Git, process list or native API.
#[test]
fn localization_actual_retry_control_keeps_raw_dispatch_identity() {
    use egui_kittest::{Harness, kittest::Queryable};
    crate::i18n::set_language("de");
    let card = ErrorCard {
        problem: GitProblem::NameTooLong,
        dir: PathBuf::from("/nicht-geöffnet/日本語"),
        place: HERE,
        label: "Reload".to_owned(),
        said: "raw git error --literal=unchanged".to_owned(),
        found: Some(Ok(Default::default())),
        stage: Stage::Idle,
        rx: None,
        earlier: None,
        earlier_undone: None,
        earlier_rx: None,
        retried: false,
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, (card, event): &mut (ErrorCard, Option<super::CardEvent>)| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            if let Some(next) = card.ui(ui, true) {
                *event = Some(next);
            }
        },
        (card, None),
    );
    h.run();
    h.get_by_label("Ein Pfad ist zu lang");
    h.get_by_label("Erneut versuchen").click();
    h.run();
    assert_eq!(h.state().1, Some(super::CardEvent::Retry));
    assert_eq!(h.state().0.label(), "Reload");
    assert_eq!(h.state().0.said, "raw git error --literal=unchanged");
    assert!(!h.state().0.can_retry());
    assert!(h.state().0.rx.is_none());
}

#[test]
fn localization_explanations_confirmations_paths_plurals_and_durations() {
    crate::i18n::set_language("de");
    let fix = Fix::Config {
        edit: ConfigEdit::add(
            Scope::Global,
            "safe.directory",
            "/srv/日本語 mit Leerzeichen",
        ),
        file: "C:/Users/ü/.gitconfig".to_owned(),
    };
    assert_eq!(fix.button(), "Diesem Ordner vertrauen");
    assert_eq!(
        fix.confirm(),
        "Fügt safe.directory = /srv/日本語 mit Leerzeichen zu C:/Users/ü/.gitconfig hinzu. Rückgängig auf dieser Karte entfernt den Eintrag wieder."
    );
    let files = (0..1004).map(|i| format!("f{i}")).collect();
    let explanation = text::explanation(
        &GitProblem::LineEndings {
            files,
            to_crlf: true,
            fatal: true,
        },
        HERE,
    );
    assert_eq!(explanation[0], "Dateien: f0, f1, f2 und 1.001 weitere.");
    assert!(explanation.iter().any(|s| s.contains("core.safecrlf")));
    assert_eq!(text::age(60), "1 Minute");
    assert_eq!(text::age(120), "2 Minuten");
    assert_eq!(text::age(7_200), "2 Stunden");
    for problem in every_problem() {
        assert!(!text::title(&problem).starts_with("interaction-"));
        for place in [HERE, WINDOWS, WSL] {
            assert!(
                text::explanation(&problem, place)
                    .iter()
                    .all(|s| !s.starts_with("interaction-"))
            );
        }
    }
    let ssh = text::explanation(
        &GitProblem::SshKeyRefused {
            login: Some("git@literal-host".into()),
        },
        WSL,
    );
    assert!(ssh[0].contains("ssh -T git@literal-host"));
    assert!(ssh[1].contains("anderen Computer"));
    crate::i18n::set_language("en-US");
    assert_eq!(fix.button(), "Trust this folder");
    assert_eq!(text::age(120), "2 minutes");
}
