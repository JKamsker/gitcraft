use super::*;
#[test]
fn actual_filter_repo_rewrites_only_the_owned_fresh_clone_and_reports_refs() -> Result<(), String> {
    if crate::git::run(None, &["filter-repo", "--version"]).is_err() {
        eprintln!("skipped: git-filter-repo is not installed here");
        return Ok(());
    }
    struct Owned(PathBuf);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "gitcraft-cleanup-fixture-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let owned = Owned(root);
    let source = owned.0.join("source");
    std::fs::create_dir(&source).map_err(|e| e.to_string())?;
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.invalid"],
        vec!["config", "commit.gpgSign", "false"],
    ] {
        super::super::run(Some(&source), &args)?;
    }
    std::fs::write(source.join("secrets.txt"), "fixture-key").map_err(|e| e.to_string())?;
    std::fs::write(source.join("config.txt"), "fixture-token").map_err(|e| e.to_string())?;
    super::super::run(Some(&source), &["add", "."])?;
    super::super::run(Some(&source), &["commit", "-qm", "fixture"])?;
    let before = refs(&source)?;
    let report = execute(
        &source,
        &owned.0,
        &Config {
            paths: vec!["secrets.txt".into()],
            replacements: vec![Replacement {
                find: "fixture-token".into(),
                replace: "redacted".into(),
            }],
        },
    )?;
    assert_eq!(refs(&source)?, before);
    assert!(!report.changes.is_empty());
    assert!(
        report.checks.iter().all(|c| c.passed),
        "{:?}",
        report.checks
    );
    assert_eq!(
        git(&report.repo, &["show", "HEAD:config.txt"], 30)?.trim(),
        "redacted"
    );
    assert!(git(&report.repo, &["show", "HEAD:secrets.txt"], 30).is_err());
    assert!(git(&report.repo, &["remote"], 30)?.trim().is_empty());
    assert!(
        !report
            .repo
            .parent()
            .ok_or("No owned root")?
            .join("replacement-rules")
            .exists()
    );
    Ok(())
}
