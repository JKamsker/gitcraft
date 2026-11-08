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
        crate::git::run_ok_codes(
            Some(repo),
            &["config", "--get", "extensions.refStorage"],
            &[0, 1],
        )?
        .trim()
        .to_owned()
    };
    let format = if format.is_empty() {
        "files".into()
    } else {
        format
    };
    let help = crate::git::run_captured(repo, &["refs", "migrate", "-h"], None)?;
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
        Some("Migration refuses repositories with linked worktrees. Remove them with Git and reopen this guide.".into())
    } else if !migrates {
        Some("This Git cannot migrate refs while preserving reflogs. Install a current Git (refs migrate was introduced in 2.46).".into())
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
        return Err("Choose a different supported ref format".into());
    }
    expected.snapshot.check(runner.repo)?;
    if let Some(why) = &inspect(runner.repo)?.blocked {
        return Err(why.clone());
    }
    let refs = expected.snapshot.refs.clone();
    let reflogs = text(
        runner.repo,
        &["reflog", "show", "--all", "--format=%gD%x00%H%x00%gs"],
    )?;
    runner.run(&[
        "refs",
        "migrate",
        &format!("--ref-format={target}"),
        "--reflog",
    ])?;
    let after = inspect(runner.repo)?;
    if after.format != target || after.snapshot.refs != refs {
        return Err("Migration completed but ref verification differs. Do not continue writing; inspect git refs verify and the migration output.".into());
    }
    let after_logs = text(
        runner.repo,
        &["reflog", "show", "--all", "--format=%gD%x00%H%x00%gs"],
    )?;
    let mut before: Vec<_> = reflogs.lines().collect();
    let mut after: Vec<_> = after_logs.lines().collect();
    before.sort();
    after.sort();
    if before != after {
        return Err(
            "Migration changed reflog contents; inspect the repository before continuing.".into(),
        );
    }
    // Available together with modern migration; errors are reported rather than hidden.
    runner.run(&["refs", "verify", "--strict"])?;
    Ok(format!(
        "Migrated to {target}; refs and reflogs match, refs verify passed."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_preserves_refs_and_refuses_linked_or_stale() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("reftable-migration")?;
        r.commit("base", &[])?;
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
        migrate(&runner, &before, "reftable")?;
        assert_eq!(inspect(r.path())?.format, "reftable");
        assert!(migrate(&runner, &before, "reftable").is_err());
        migrate(&runner, &inspect(r.path())?, "files")?;
        let linked = r.path().join("linked");
        r.git(&["worktree", "add", "--detach", &linked.to_string_lossy()])?;
        assert!(inspect(r.path())?.blocked.is_some());
        assert!(inspect(&linked)?.blocked.is_some());
        Ok(())
    }
}
