//! Local-only publication preview; execution remains owned by root preflight.
use super::*;
pub(crate) fn prepare_publication(
    repo: &Path,
    branch: &str,
    remote: &str,
    destination: &str,
) -> Result<PreparedPush, String> {
    let snapshot = Snapshot::capture(repo)?;
    if branch == "HEAD" {
        return Err(crate::i18n::text("tracking-reserved-head").into());
    }
    let source = format!("refs/heads/{branch}");
    let destination = format!("refs/heads/{destination}");
    text(repo, &["check-ref-format", &source])?;
    text(repo, &["check-ref-format", &destination])?;
    if !text(repo, &["remote"])?.lines().any(|name| name == remote) {
        return Err(crate::i18n::text("tracking-stale").into());
    }
    let tracking = super::super::tracking::read(repo, branch)?;
    if let Some(error) = tracking.saving_blocked {
        return Err(error);
    }
    for field in ["remote", "merge"] {
        if values(repo, &format!("branch.{branch}.{field}"))?.len() > 1 {
            return Err(crate::i18n::text("publish-tracking-blocked").into());
        }
    }
    let commit = text(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &source],
    )?
    .trim()
    .to_owned();
    let urls: Vec<String> = text(
        repo,
        &["remote", "get-url", "--push", "--all", "--", remote],
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    if urls.is_empty() {
        return Err(crate::i18n::text("publish-no-remotes").into());
    }
    for url in &urls {
        super::super::remote_config::validate_url(url)?;
    }
    snapshot.check(repo)?;
    Ok(PreparedPush {
        snapshot,
        route: PushRoute {
            branch: branch.into(),
            commit,
            remote: remote.into(),
            destination,
            urls,
            lease: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Repo;
    #[test]
    fn preview_is_local_validates_names_and_preserves_tracking() -> Result<(), String> {
        let repo = Repo::new("publish-preview")?;
        repo.commit("base", &[])?;
        let host = repo.path().join("receiver.git");
        crate::git::run(None, &["init", "--bare", &host.to_string_lossy()])?;
        repo.git(&["remote", "add", "origin", &host.to_string_lossy()])?;
        repo.git(&["config", "branch.main.remote", "."])?;
        repo.git(&["config", "branch.main.merge", "refs/heads/main"])?;
        repo.git(&["update-ref", "refs/remotes/origin/main", "refs/heads/main"])?;
        repo.git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ])?;
        let before = text(repo.path(), &["config", "--null", "--list"])?;
        let prepared = prepare_publication(repo.path(), "main", "origin", "töpíc/new")?;
        assert!(!prepared.targets_known_default());
        assert!(
            prepare_publication(repo.path(), "main", "origin", "main")?.targets_known_default()
        );
        assert_eq!(prepared.route.destination, "refs/heads/töpíc/new");
        assert_eq!(prepared.route.urls, [host.to_string_lossy().into_owned()]);
        let ops = prepared.route.publication_operations();
        assert_eq!(ops[1][1], "--set-upstream");
        assert_eq!(
            ops[1].last().map(String::as_str),
            Some("refs/heads/main:refs/heads/töpíc/new")
        );
        assert!(!ops[1].iter().any(|word| word.starts_with("--force")));
        assert_eq!(text(repo.path(), &["config", "--null", "--list"])?, before);
        assert!(text(&host, &["for-each-ref"])?.is_empty());
        for destination in ["bad..name", "bad name"] {
            assert!(prepare_publication(repo.path(), "main", "origin", destination).is_err());
        }
        assert!(prepare_publication(repo.path(), "HEAD", "origin", "topic").is_err());
        assert!(prepare_publication(repo.path(), "main", "absent", "topic").is_err());
        repo.git(&[
            "remote",
            "set-url",
            "--push",
            "origin",
            "https://changed.invalid/repo",
        ])?;
        assert!(prepared.snapshot.check(repo.path()).is_err());
        Ok(())
    }

    #[test]
    fn publication_previews_effective_urls_after_push_and_instead_of_rewrites() -> Result<(), String>
    {
        let repo = Repo::new("publish-url-rewrites")?;
        repo.commit("base", &[])?;
        repo.git(&["remote", "add", "origin", "read:repo.git"])?;
        let base = repo.path().to_string_lossy();
        let read = format!("{base}/read/");
        let write = format!("{base}/write/");
        let explicit = format!("{base}/explicit/");
        repo.git(&["config", &format!("url.{read}.insteadOf"), "read:"])?;
        repo.git(&["config", &format!("url.{write}.pushInsteadOf"), "read:"])?;
        assert_eq!(
            prepare_publication(repo.path(), "main", "origin", "topic")?
                .route
                .urls,
            [format!("{write}repo.git")]
        );
        repo.git(&["remote", "set-url", "--push", "origin", "send:repo.git"])?;
        repo.git(&["config", &format!("url.{explicit}.insteadOf"), "send:"])?;
        assert_eq!(
            prepare_publication(repo.path(), "main", "origin", "topic")?
                .route
                .urls,
            [format!("{explicit}repo.git")]
        );
        Ok(())
    }
}
