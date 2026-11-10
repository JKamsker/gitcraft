use super::*;
use crate::git::fixture::Repo;

fn parity(repo: &Path) -> Result<(), String> {
    assert_eq!(native::refs(repo)?, super::super::refs::refs_cli(repo)?);
    assert_eq!(
        native::remotes(repo)?,
        super::super::refs::remotes_cli(repo)?
    );
    let expected = super::super::run(Some(repo), &["rev-parse", "--absolute-git-dir"])?;
    assert_eq!(
        native::git_dir(repo)?,
        std::fs::canonicalize(expected.trim()).map_err(|e| e.to_string())?
    );
    let expected =
        super::super::run(Some(repo), &["rev-parse", "--verify", "-q", "HEAD"]).unwrap_or_default();
    assert_eq!(native::head(repo)?, expected.trim());
    Ok(())
}
#[test]
fn native_refs_cover_unborn_packed_tags_detached_and_tracking() -> Result<(), String> {
    let repo = Repo::new("native-refs-parity")?;
    parity(repo.path())?;
    let base = repo.commit("base", &[("file", Some("one\n"))])?;
    for args in [
        &["branch", "tëst/with-slash"][..],
        &["tag", "main"],
        &["tag", "-a", "annotated", "-m", "tag"],
        &["tag", "-a", "nested", "annotated", "-m", "nested"],
        &["remote", "add", "my/fork", "/a remote path with spaces"],
        &["update-ref", "refs/remotes/my/fork/main", &base],
        &["config", "branch.main.remote", "my/fork"],
        &["config", "branch.main.merge", "refs/heads/main"],
    ] {
        repo.git(args)?;
    }
    parity(repo.path())?;
    repo.commit("ahead", &[])?;
    parity(repo.path())?;
    repo.git(&["checkout", "-q", "--detach", &base])?;
    repo.commit("divergent", &[])?;
    repo.git(&["update-ref", "refs/remotes/my/fork/main", "HEAD"])?;
    repo.git(&["checkout", "-q", "refs/heads/main"])?;
    parity(repo.path())?;
    repo.git(&["pack-refs", "--all"])?;
    parity(repo.path())?;
    repo.git(&["update-ref", "-d", "refs/remotes/my/fork/main"])?;
    parity(repo.path())?;
    Ok(())
}

/// Both readers resolve push settings independently of tracking tips and reopen config.
#[test]
fn native_push_remotes_match_git_and_follow_config_changes() -> Result<(), String> {
    let repo = Repo::new("native-push-remote-parity")?;
    repo.commit("base", &[])?;
    repo.git(&["remote", "add", "origin", "https://github.com/up/repo.git"])?;
    repo.git(&["remote", "add", "my/fork", "https://github.com/me/repo.git"])?;
    parity(repo.path())?;
    assert!(native::refs(repo.path())?.push_remotes.is_empty());
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"])?;
    repo.git(&["branch", "--set-upstream-to", "origin/main"])?;
    parity(repo.path())?;
    repo.git(&["config", "remote.pushDefault", "my/fork"])?;
    parity(repo.path())?;
    assert_eq!(native::refs(repo.path())?.push_remotes["main"], "my/fork");
    repo.git(&["config", "branch.main.pushRemote", "origin"])?;
    parity(repo.path())?;
    assert_eq!(native::refs(repo.path())?.push_remotes["main"], "origin");
    repo.git(&["config", "--unset", "branch.main.pushRemote"])?;
    repo.git(&["config", "--unset", "remote.pushDefault"])?;
    parity(repo.path())?;
    assert_eq!(native::refs(repo.path())?.push_remotes["main"], "origin");
    Ok(())
}
#[test]
fn ambiguous_upstream_spellings_fall_back_to_git() -> Result<(), String> {
    let repo = Repo::new("native-upstream-ambiguity")?;
    let id = repo.commit("base", &[])?;
    repo.git(&["branch", "origin/main"])?;
    repo.git(&["update-ref", "refs/remotes/origin/main", &id])?;
    repo.git(&["config", "branch.main.remote", "."])?;
    repo.git(&["config", "branch.main.merge", "refs/heads/origin/main"])?;
    assert_eq!(
        native::refs(repo.path()),
        Err("local upstream requires Git".to_owned())
    );
    let expected = super::super::refs::refs_cli(repo.path())?;
    assert_eq!(
        expected
            .local
            .iter()
            .find(|b| b.name == "main")
            .map(|b| b.upstream.as_str()),
        Some("heads/origin/main")
    );
    assert_eq!(super::super::refs(repo.path())?, expected);
    repo.git(&["branch", "-D", "origin/main"])?;
    repo.git(&["remote", "add", "origin", "/a remote"])?;
    repo.git(&["config", "branch.main.remote", "origin"])?;
    repo.git(&["config", "branch.main.merge", "refs/heads/main"])?;
    for candidate in [
        "refs/origin/main",
        "refs/tags/origin/main",
        "refs/remotes/origin/main/HEAD",
    ] {
        // A ref cannot coexist with a ref beneath it in the files backend.
        if candidate.ends_with("/HEAD") {
            repo.git(&["update-ref", "-d", "refs/remotes/origin/main"])?;
        }
        repo.git(&["update-ref", candidate, &id])?;
        assert_eq!(
            native::refs(repo.path()),
            Err("ambiguous upstream short name".to_owned()),
            "{candidate}"
        );
        assert_eq!(
            super::super::refs(repo.path())?,
            super::super::refs::refs_cli(repo.path())?
        );
        repo.git(&["update-ref", "-d", candidate])?;
    }
    Ok(())
}

#[test]
fn tracking_walk_budget_is_shared_across_branches() -> Result<(), String> {
    let repo = Repo::new("native-tracking-budget")?;
    let base = repo.commit("base", &[])?;
    repo.commit("one", &[])?;
    repo.commit("two", &[])?;
    repo.git(&["remote", "add", "origin", "/a remote"])?;
    repo.git(&["update-ref", "refs/remotes/origin/main", &base])?;
    repo.git(&["config", "branch.main.remote", "origin"])?;
    repo.git(&["config", "branch.main.merge", "refs/heads/main"])?;
    // Three local ancestors and one upstream ancestor fit once.
    assert!(native::refs_bounded(repo.path(), 4).is_ok());
    repo.git(&["branch", "other"])?;
    repo.git(&["config", "branch.other.remote", "origin"])?;
    repo.git(&["config", "branch.other.merge", "refs/heads/main"])?;
    assert_eq!(
        native::refs_bounded(repo.path(), 4),
        Err("tracking graph exceeds native bound".to_owned())
    );
    assert!(native::refs_bounded(repo.path(), 8).is_ok());
    parity(repo.path())?;
    Ok(())
}

#[test]
fn sha256_and_non_commit_tags_match_git() -> Result<(), String> {
    let repo = Repo::new("native-sha256")?;
    std::fs::remove_dir_all(repo.path().join(".git")).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "--object-format=sha256", "-b", "main"][..],
        &["config", "user.name", "Native parity"],
        &["config", "user.email", "native@example.invalid"],
        &["config", "commit.gpgSign", "false"],
    ] {
        repo.git(args)?;
    }
    repo.commit("sha256", &[("file", Some("content\n"))])?;
    let blob = repo.rev("HEAD:file")?;
    let tree = repo.rev("HEAD^{tree}")?;
    repo.git(&["tag", "blob", &blob])?;
    repo.git(&["tag", "-a", "annotated-blob", &blob, "-m", "blob tag"])?;
    repo.git(&["tag", "tree", &tree])?;
    parity(repo.path())?;
    assert_eq!(native::head(repo.path())?.len(), 64);
    Ok(())
}

#[test]
fn included_config_and_config_changes_are_reopened() -> Result<(), String> {
    let repo = Repo::new("native-included-config")?;
    repo.commit("base", &[])?;
    let include = repo.path().join("included config");
    std::fs::write(&include, "[remote \"nested/fork\"]\nurl = /path with spaces\nfetch = +refs/heads/*:refs/remotes/nested/fork/*\n").map_err(|e| e.to_string())?;
    repo.git(&["config", "include.path", &include.to_string_lossy()])?;
    repo.git(&["update-ref", "refs/remotes/nested/fork/main", "HEAD"])?;
    parity(repo.path())?;
    std::fs::write(&include, "[remote \"nested/fork\"]\nurl = https://example.invalid/changed\nfetch = +refs/heads/*:refs/remotes/nested/fork/*\n").map_err(|e| e.to_string())?;
    parity(repo.path())?;
    assert_eq!(
        native::remotes(repo.path())?[0].url,
        "https://example.invalid/changed"
    );
    Ok(())
}

#[test]
fn linked_worktrees_use_their_own_head_and_state_directory() -> Result<(), String> {
    let repo = Repo::new("native-linked-worktree")?;
    repo.commit("base", &[])?;
    let worktree = repo.path().join("linked worktree ü");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "other",
        &worktree.to_string_lossy(),
    ])?;
    parity(&worktree)?;
    assert_ne!(native::git_dir(repo.path())?, native::git_dir(&worktree)?);
    Ok(())
}
#[test]
fn remote_context_cannot_invoke_native_local_reads() {
    let path = Path::new("ssh://host/tmp/repo");
    assert_eq!(capability(path), Err("remote-context"));
    let value = dispatch(
        path,
        Read::Head,
        |_| Err("native must not run".into()),
        || Ok("remote result".to_owned()),
    );
    assert_eq!(value.as_deref(), Ok("remote result"));
}
#[test]
fn control_characters_in_remote_config_retain_cli_display_parsing() -> Result<(), String> {
    let repo = Repo::new("native-remote-display")?;
    for (name, url) in [("tab\tname", "/tmp/path"), ("normal", "/tmp/line\nbreak")] {
        repo.git(&["config", &format!("remote.{name}.url"), url])?;
        assert_eq!(
            native::remotes(repo.path()),
            Err("remote display requires Git".to_owned())
        );
        assert_eq!(
            super::super::remotes(repo.path())?,
            super::super::refs::remotes_cli(repo.path())?
        );
        repo.git(&["config", "--remove-section", &format!("remote.{name}")])?;
    }
    Ok(())
}

#[test]
fn unsupported_and_broken_native_reads_retain_cli_result() -> Result<(), String> {
    let repo = Repo::new("native-fallback")?;
    repo.commit("base", &[])?;
    repo.git(&["remote", "add", "origin", "short:repo"])?;
    repo.git(&["config", "url.https://example.invalid/.insteadOf", "short:"])?;
    assert!(native::remotes(repo.path()).is_err());
    assert_eq!(
        super::super::remotes(repo.path())?,
        super::super::refs::remotes_cli(repo.path())?
    );
    let value = dispatch(
        repo.path(),
        Read::Head,
        |_| Err("simulated corrupt object".into()),
        || Ok("CLI remains authoritative"),
    );
    assert_eq!(value, Ok("CLI remains authoritative"));
    assert!(
        metrics()
            .iter()
            .any(|m| m.operation == "head" && m.fallback_calls > 0)
    );
    Ok(())
}
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_reference_names_fall_back_without_loss() -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let repo = Repo::new("native-non-utf8-ref")?;
    let id = repo.commit("base", &[])?;
    let path = repo
        .path()
        .join(".git/refs/heads")
        .join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
    std::fs::write(path, format!("{id}\n")).map_err(|e| e.to_string())?;
    assert!(native::refs(repo.path()).is_err());
    assert_eq!(
        super::super::refs(repo.path())?,
        super::super::refs::refs_cli(repo.path())?
    );
    Ok(())
}
