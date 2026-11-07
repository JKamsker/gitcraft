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
