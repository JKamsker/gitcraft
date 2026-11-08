use super::guard::{Runner, Snapshot, text};
use std::path::Path;

#[derive(Clone, Debug)]
pub(crate) struct Storage {
    pub format: String,
    pub git: String,
    pub migrates: bool,
    pub blocked: Option<String>,
    pub snapshot: Snapshot,
}
pub(crate) fn inspect(repo: &Path) -> Result<Storage, String> {
    let git = text(repo, &["--version"])?.trim().to_owned();
    let reported = text(repo, &["rev-parse", "--show-ref-format"])?
        .trim()
        .to_owned();
    let format = if matches!(reported.as_str(), "files" | "reftable") {
        reported
    } else {
        super::text_codes(repo, &["config", "--get", "extensions.refStorage"], &[0, 1])?
            .trim()
            .to_owned()
    };
    let format = if format.is_empty() {
        "files".into()
    } else {
        format
    };
    let help = super::captured(repo, &["refs", "migrate", "-h"], None)?;
    let help = format!("{}{}", help.stdout, help.stderr);
    let migrates = help.contains("--ref-format") && help.contains("--reflog");
    let snapshot = Snapshot::capture(repo)?;
    let git_dir = snapshot.identity.trim();
    let common = text(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let blocked = if snapshot
        .worktrees
        .lines()
        .filter(|l| l.starts_with("worktree "))
        .count()
        != 1
        || common.trim() != git_dir
    {
        Some(
            crate::i18n::text(
                "backend-migration-refuses-repositories-with-linked-worktrees-remove-them-a777da",
            )
            .into(),
        )
    } else if !migrates {
        Some(
            crate::i18n::text(
                "backend-this-git-cannot-migrate-refs-while-preserving-reflogs-install-a-c-8bf101",
            )
            .into(),
        )
    } else {
        None
    };
    Ok(Storage {
        format,
        git,
        migrates,
        blocked,
        snapshot,
    })
}
pub(crate) fn migrate(
    runner: &Runner<'_>,
    expected: &Storage,
    target: &str,
) -> Result<String, String> {
    if !matches!(target, "files" | "reftable") || expected.format == target {
        return Err(
            crate::i18n::text("backend-choose-a-different-supported-ref-format-fa9436").into(),
        );
    }
    expected.snapshot.check(runner.repo)?;
    if let Some(why) = &inspect(runner.repo)?.blocked {
        return Err(why.clone());
    }
    let refs = ref_witness(runner.repo)?;
    let reflogs = text(
        runner.repo,
        &[
            "reflog",
            "show",
            "--all",
            "--date=raw",
            "--format=%gD%x00%H%x00%gn%x00%ge%x00%gs",
        ],
    )?;
    verified_migration(
        target,
        |args| runner.run(args),
        || expected.snapshot.check(runner.repo),
    )?;
    let after = inspect(runner.repo)?;
    if after.format != target || ref_witness(runner.repo)? != refs {
        return Err(crate::i18n::text(
            "backend-migration-completed-but-ref-verification-differs-do-not-continue-1cba00",
        )
        .into());
    }
    let after_logs = text(
        runner.repo,
        &[
            "reflog",
            "show",
            "--all",
            "--date=raw",
            "--format=%gD%x00%H%x00%gn%x00%ge%x00%gs",
        ],
    )?;
    let mut before: Vec<_> = reflogs.lines().collect();
    let mut after: Vec<_> = after_logs.lines().collect();
    before.sort();
    after.sort();
    if before != after {
        return Err(crate::i18n::text(
            "backend-migration-changed-reflog-contents-inspect-the-repository-before-c-fcb84c",
        )
        .into());
    }
    // Available together with modern migration; errors are reported rather than hidden.
    runner.run(&["refs", "verify", "--strict"])?;
    Ok(crate::i18n::message(
        "backend-migrated-to-target-refs-and-reflogs-match-refs-verify-passed-09724a",
        &[("target", (target).to_string().into())],
    ))
}

fn verified_migration(
    target: &str,
    mut run: impl FnMut(&[&str]) -> Result<String, String>,
    recheck: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // Git can accept an invalid symbolic target in the files backend but omit
    // its root when migrating. Refuse that graph before changing its backend.
    run(&["refs", "verify", "--strict"])?;
    recheck()?;
    run(&[
        "refs",
        "migrate",
        &format!("--ref-format={target}"),
        "--reflog",
    ])?;
    Ok(())
}

fn ref_witness(repo: &Path) -> Result<Vec<super::all_refs::Reference>, String> {
    let (refs, fallback) = super::all_refs::references(repo)?;
    if fallback {
        return Err(crate::i18n::text(
            "backend-migration-requires-root-ref-enumeration-for-verification-update-g-675e0c",
        )
        .into());
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_strict_verification_and_snapshot_refusal_precede_migration() {
        for failure in 0..3 {
            let calls = std::cell::RefCell::new(Vec::new());
            let result = verified_migration(
                "reftable",
                |args| {
                    calls
                        .borrow_mut()
                        .push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
                    if failure == 1 {
                        Err("badReferentName".into())
                    } else {
                        Ok(String::new())
                    }
                },
                || {
                    calls.borrow_mut().push(vec!["snapshot-check".into()]);
                    if failure == 2 {
                        Err("snapshot changed".into())
                    } else {
                        Ok(())
                    }
                },
            );
            let calls = calls.into_inner();
            assert_eq!(calls[0], ["refs", "verify", "--strict"]);
            match failure {
                1 => {
                    assert_eq!(result, Err("badReferentName".into()));
                    assert_eq!(calls.len(), 1);
                }
                2 => {
                    assert_eq!(result, Err("snapshot changed".into()));
                    assert_eq!(calls.len(), 2);
                }
                _ => {
                    assert_eq!(result, Ok(()));
                    assert_eq!(calls.len(), 3);
                    assert_eq!(
                        calls[2],
                        ["refs", "migrate", "--ref-format=reftable", "--reflog"]
                    );
                }
            }
            if failure != 1 {
                assert_eq!(calls[1], ["snapshot-check"]);
            }
        }
    }

    #[test]
    fn round_trip_preserves_refs_and_refuses_linked_or_stale() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("reftable-migration")?;
        let commit = r.commit("base", &[])?;
        r.git(&["update-ref", "APP_ROOT", &commit])?;
        r.git(&["symbolic-ref", "refs/gitcraft/root", "APP_ROOT"])?;
        let refs = ref_witness(r.path());
        let before = inspect(r.path())?;
        if !before.migrates {
            return Ok(());
        }
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        let invalid_refs = refs?;
        let error = migrate(&runner, &before, "reftable")
            .err()
            .ok_or("Invalid symbolic root target was migrated")?;
        assert!(error.contains("badReferentName"), "{error}");
        assert_eq!(inspect(r.path())?.format, "files");
        assert_eq!(ref_witness(r.path())?, invalid_refs);
        assert_eq!(Snapshot::capture(r.path())?, before.snapshot);
        assert_eq!(r.rev("APP_ROOT")?, commit);
        assert_eq!(
            r.git(&["symbolic-ref", "--no-recurse", "refs/gitcraft/root"])?
                .trim(),
            "APP_ROOT"
        );
        // Preserve the original invalid graph as the negative control above;
        // a supported root name must still migrate successfully in both ways.
        r.git(&["update-ref", "APP_HEAD", &commit])?;
        r.git(&["symbolic-ref", "refs/gitcraft/root", "APP_HEAD"])?;
        r.git(&["update-ref", "--no-deref", "-d", "APP_ROOT", &commit])?;
        let refs = ref_witness(r.path())?;
        assert!(refs.iter().any(|r| r.name == "APP_HEAD" && r.oid == commit));
        assert!(
            refs.iter()
                .any(|r| r.name == "refs/gitcraft/root" && r.symbolic == "APP_HEAD")
        );
        let before = inspect(r.path())?;
        migrate(&runner, &before, "reftable").map_err(|error| {
            format!(
                "{error}\nRef witness before migration: {refs:#?}\nRef witness after migration: {:#?}",
                ref_witness(r.path())
            )
        })?;
        assert_eq!(ref_witness(r.path())?, refs);
        assert_eq!(inspect(r.path())?.format, "reftable");
        assert!(migrate(&runner, &before, "reftable").is_err());
        migrate(&runner, &inspect(r.path())?, "files")?;
        assert_eq!(ref_witness(r.path())?, refs);
        let linked = r.path().join("linked");
        r.git(&["worktree", "add", "--detach", &linked.to_string_lossy()])?;
        assert!(inspect(r.path())?.blocked.is_some());
        assert!(inspect(&linked)?.blocked.is_some());
        Ok(())
    }
}
