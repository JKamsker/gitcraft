use super::*;

struct Sandbox(PathBuf);
impl Sandbox {
    fn new(scenario: Scenario) -> Result<Self, String> {
        create(scenario).map(Self)
    }
    fn session(&self) -> Result<Session, String> {
        load(&self.0).ok_or_else(|| "No practice marker".to_owned())
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Some(root) = self.0.parent() {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

#[test]
fn a_conflict_requires_finishing_the_merge_without_markers() -> Result<(), String> {
    let sandbox = Sandbox::new(Scenario::Conflict)?;
    let repo = &sandbox.0;
    let session = sandbox.session()?;
    assert!(!session.check(repo)?);
    assert!(run(repo, &["ls-files", "-u"])?.contains("greeting.txt"));
    run(repo, &["merge", "--abort"])?;
    assert!(!session.check(repo)?, "Aborting is not resolving");
    assert!(run(repo, &["merge", "--no-edit", "feature"]).is_err());
    // A clean commit containing the markers must not pass.
    commit(repo, "Accidentally committed the conflict markers")?;
    assert!(!session.check(repo)?);
    write(repo, "greeting.txt", "Hello from main and feature!\n")?;
    commit(repo, "Resolved the greeting")?;
    assert!(session.check(repo)?);
    assert!(
        load(repo).is_some(),
        "Scenario survives re-opening the repository"
    );
    write(repo, "greeting.txt", "Uncommitted experiment\n")?;
    assert!(!session.check(repo)?);
    Ok(())
}

#[test]
fn diverged_branches_must_be_joined_on_main() -> Result<(), String> {
    let sandbox = Sandbox::new(Scenario::Diverged)?;
    let repo = &sandbox.0;
    let session = sandbox.session()?;
    assert!(!session.check(repo)?);
    assert!(run(repo, &["merge", "--ff-only", "feature"]).is_err());
    run(repo, &["checkout", "feature"])?;
    run(repo, &["merge", "--no-edit", "main"])?;
    assert!(!session.check(repo)?, "The requested branch is main");
    run(repo, &["checkout", "main"])?;
    run(repo, &["merge", "--no-edit", "feature"])?;
    assert!(session.check(repo)?);
    assert!(repo.join("main.txt").is_file() && repo.join("feature.txt").is_file());
    Ok(())
}

#[test]
fn detached_commit_needs_a_branch_that_reaches_it() -> Result<(), String> {
    let sandbox = Sandbox::new(Scenario::Detached)?;
    let repo = &sandbox.0;
    let session = sandbox.session()?;
    assert!(!session.check(repo)?);
    run(repo, &["checkout", "-b", "wrong-place", "main"])?;
    assert!(!session.check(repo)?);
    run(repo, &["checkout", "-b", "rescued", &session.target])?;
    assert!(session.check(repo)?);
    run(repo, &["checkout", "--detach"])?;
    assert!(
        !session.check(repo)?,
        "A branch exists, but HEAD must be attached too"
    );
    Ok(())
}

#[test]
fn published_undo_needs_a_new_pushed_commit_and_keeps_the_old_history() -> Result<(), String> {
    let sandbox = Sandbox::new(Scenario::Published)?;
    let repo = &sandbox.0;
    let session = sandbox.session()?;
    assert!(!session.check(repo)?);
    run(repo, &["reset", "--hard", &session.before])?;
    assert!(
        !session.check(repo)?,
        "Resetting published history is not a revert"
    );
    run(repo, &["reset", "--hard", &session.target])?;
    run(repo, &["revert", "--no-edit", &session.target])?;
    assert!(!session.check(repo)?, "The revert has not been pushed yet");
    assert_eq!(
        run(repo, &["config", "--local", "protocol.file.allow"])?.trim(),
        "always"
    );
    let origin = repo.parent().ok_or("No origin folder")?.join("origin.git");
    assert!(run(&origin, &["config", "--local", "core.hooksPath"])?.contains("disabled-hooks"));
    run(repo, &["push", "origin", "main"])?;
    assert!(session.check(repo)?);
    // The check reads the original local origin directly, never an edited remote URL.
    run(
        repo,
        &[
            "remote",
            "set-url",
            "origin",
            "https://never-contact.example.invalid/repo",
        ],
    )?;
    assert!(session.check(repo)?);
    assert_eq!(run(repo, &["rev-list", "--count", "HEAD"])?.trim(), "3");
    Ok(())
}

#[test]
fn setup_is_disposable_and_does_not_change_its_parent_or_other_repositories() -> Result<(), String>
{
    let parent = Sandbox::new(Scenario::Detached)?;
    let repo = &parent.0;
    let head = oid(repo, "HEAD")?;
    write(repo, "precious.txt", "Do not change me\n")?;
    let folder = repo.join("practice ü with spaces");
    std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let nested = Sandbox(create_in(&folder, Scenario::Published)?);
    assert_ne!(nested.0, *repo);
    assert_eq!(oid(repo, "HEAD")?, head);
    assert_eq!(
        std::fs::read_to_string(repo.join("precious.txt")).map_err(|e| e.to_string())?,
        "Do not change me\n"
    );
    assert_ne!(nested.0.parent(), parent.0.parent());
    Ok(())
}

#[test]
fn malformed_oversized_foreign_and_remote_markers_are_not_accepted() -> Result<(), String> {
    let sandbox = Sandbox::new(Scenario::Detached)?;
    let repo = &sandbox.0;
    let mut session = sandbox.session()?;
    session.target = "--output=somewhere".to_owned();
    std::fs::write(
        repo.join(MARKER),
        serde_json::to_vec(&session).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert!(load(repo).is_none());
    std::fs::write(repo.join(MARKER), vec![b' '; 8193]).map_err(|e| e.to_string())?;
    assert!(load(repo).is_none());
    assert!(load(Path::new("ssh://build/home/me/repository")).is_none());
    assert!(load(&std::env::temp_dir()).is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_symlink_in_place_of_the_marker_is_not_followed() -> Result<(), String> {
    let sandbox = Sandbox::new(Scenario::Detached)?;
    let repo = &sandbox.0;
    let marker = repo.join(MARKER);
    let elsewhere = repo.join(".git/marker-copy");
    std::fs::rename(&marker, &elsewhere).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&elsewhere, &marker).map_err(|e| e.to_string())?;
    assert!(load(repo).is_none());
    Ok(())
}

#[test]
fn an_opened_practice_path_keeps_its_safe_environment_if_the_marker_is_removed()
-> Result<(), String> {
    let sandbox = Sandbox::new(Scenario::Detached)?;
    assert!(is_repository(&sandbox.0));
    std::fs::remove_file(sandbox.0.join(MARKER)).map_err(|e| e.to_string())?;
    assert!(load(&sandbox.0).is_none());
    assert!(
        is_repository(&sandbox.0),
        "A missing marker must not reactivate caller Git routing mid-session"
    );
    Ok(())
}

#[test]
fn fluent_scenario_display_switches_without_changing_raw_or_serialized_identity()
-> Result<(), String> {
    // These are presentation-only calls: never create/load/check a practice repository here.
    let before: Vec<_> = Scenario::ALL
        .into_iter()
        .map(|s| {
            Ok((
                s.title(),
                s.goal(),
                s.hint(),
                serde_json::to_string(&s).map_err(|e| e.to_string())?,
            ))
        })
        .collect::<Result<_, String>>()?;
    for (locale, titles) in [
        (
            "en-US",
            [
                "Resolve a merge conflict",
                "Merge diverged branches",
                "Rescue a detached HEAD",
                "Undo a published commit",
            ],
        ),
        (
            "de",
            [
                "Einen Merge-Konflikt lösen",
                "Auseinanderentwickelte Branches zusammenführen",
                "Einen losgelösten HEAD retten",
                "Einen veröffentlichten Commit rückgängig machen",
            ],
        ),
        (
            "en-US",
            [
                "Resolve a merge conflict",
                "Merge diverged branches",
                "Rescue a detached HEAD",
                "Undo a published commit",
            ],
        ),
    ] {
        crate::i18n::set_language(locale);
        for ((scenario, expected), raw) in Scenario::ALL.into_iter().zip(titles).zip(&before) {
            assert_eq!(scenario.display_title(), expected);
            assert_eq!(scenario.title(), raw.0);
            assert_eq!(scenario.goal(), raw.1);
            assert_eq!(scenario.hint(), raw.2);
            assert_eq!(
                serde_json::to_string(&scenario).map_err(|e| e.to_string())?,
                raw.3
            );
            if locale == "en-US" {
                assert_eq!(scenario.display_goal(), raw.1);
                assert_eq!(scenario.display_hint(), raw.2);
            } else {
                assert_ne!(scenario.display_goal(), raw.1);
                assert_ne!(scenario.display_hint(), raw.2);
            }
        }
    }
    crate::i18n::set_language("de");
    assert!(Scenario::Conflict.display_goal().contains("greeting.txt"));
    assert!(
        Scenario::Published
            .display_hint()
            .contains("A published mistake")
    );
    assert!(Scenario::Published.display_goal().contains("origin"));
    assert!(Scenario::Detached.display_goal().contains("erreichbar"));
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn fluent_practice_errors_translate_only_known_wrappers_and_preserve_payloads() {
    let detail = "Berechtigung: /tmp/ü { $unknown }\nprovider: do not translate";
    let raw = format!("Could not make a practice folder: {detail}");
    for (locale, prefix, missing, stopped) in [
        (
            "en-US",
            "Could not make a practice folder: ",
            "Practice folder is missing",
            "Making the practice repository stopped unexpectedly. Try again.",
        ),
        (
            "de",
            "Der Übungsordner konnte nicht erstellt werden: ",
            "Der Übungsordner fehlt",
            "Das Erstellen des Übungsrepositorys wurde unerwartet beendet. Versuche es erneut.",
        ),
    ] {
        crate::i18n::set_language(locale);
        assert_eq!(display_error(&raw), format!("{prefix}{detail}"));
        assert_eq!(display_error("Practice folder is missing"), missing);
        assert_eq!(
            display_error("Making the practice repository stopped unexpectedly. Try again."),
            stopped
        );
        assert_eq!(display_error(detail), detail);
        assert_eq!(
            display_error("Could not make a practice folder (provider)"),
            "Could not make a practice folder (provider)"
        );
        let mismatch = display_error(
            "This practice repository no longer matches its scenario. Start a fresh one from Practice.",
        );
        assert!(mismatch.contains(if locale == "de" {
            "passt nicht mehr"
        } else {
            "no longer matches"
        }));
    }
    crate::i18n::set_language("en-US");
}
