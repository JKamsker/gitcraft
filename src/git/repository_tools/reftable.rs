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
    expected.snapshot.check(runner.repo)?;
    runner.run(&[
        "refs",
        "migrate",
        &format!("--ref-format={target}"),
        "--reflog",
    ])?;
    let after = inspect(runner.repo)?;
    if after.format != target || ref_witness(runner.repo)? != refs {
        return Err("Migration completed but ref verification differs. Do not continue writing; inspect git refs verify and the migration output.".into());
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

fn ref_witness(repo: &Path) -> Result<Vec<super::all_refs::Reference>, String> {
    let (refs, fallback) = super::all_refs::references(repo)?;
    if fallback {
        return Err("Migration requires root-ref enumeration for verification; update Git before continuing".into());
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let refs = refs?;
        migrate(&runner, &before, "reftable")?;
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
