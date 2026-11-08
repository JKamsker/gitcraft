//! Submodule-specific preparation, sharing batch C's exact native publisher.
//! Provenance: 825d1f89 index_transaction/native_files, derived from 1535d831.
use super::{guard::Runner, submodule_conflict::Conflict};
use crate::git::drafts::index_transaction::{IndexSnapshot, IndexTransaction};
use std::{io::Write, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Prepared,
    BeforePublication,
}

pub(super) fn blocker(repo: &Path) -> Option<String> {
    if crate::remote::is_remote(repo) {
        Some(
            crate::i18n::text(
                "backend-recording-this-parent-index-over-ssh-wsl-is-unavailable-the-trans-e378bd",
            )
            .into(),
        )
    } else if !cfg!(any(target_os = "macos", target_os = "linux")) {
        Some(
            crate::i18n::text(
                "backend-recording-this-parent-index-requires-the-shared-macos-linux-nativ-c9a1cb",
            )
            .into(),
        )
    } else {
        None
    }
}
fn stages_match(observed: &[u8], expected: &[u8]) -> Result<(), String> {
    if observed != expected {
        return Err(crate::i18n::text(
            "backend-conflict-stages-changed-the-newer-parent-index-was-preserved-reop-cdb456",
        )
        .into());
    }
    Ok(())
}
fn publish_if_current<T>(
    current: bool,
    publish: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if !current {
        return Err(crate::i18n::text(
            "backend-parent-index-changed-during-submodule-validation-the-newer-index-20aca2",
        )
        .into());
    }
    publish()
}
pub(super) fn record(
    runner: &Runner<'_>,
    conflict: &Conflict,
    commit: &str,
) -> Result<String, String> {
    record_with_hook(runner, conflict, commit, &mut |_| Ok(()))
}
pub(super) fn record_with_hook(
    runner: &Runner<'_>,
    conflict: &Conflict,
    commit: &str,
    hook: &mut dyn FnMut(Phase) -> Result<(), String>,
) -> Result<String, String> {
    if let Some(why) = blocker(runner.repo) {
        return Err(why);
    }
    if runner.control.is_cancelled() {
        return Err(crate::git::CANCELLED.into());
    }
    let budget = super::ReadBudget::new(10, std::time::Duration::from_secs(60));
    let path = budget.text(
        runner.repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        Some(runner.control.clone()),
    )?;
    let before = IndexSnapshot::capture(Path::new(path.trim()))?.ok_or(crate::i18n::text(
        "backend-the-conflicted-parent-index-disappeared-01e809",
    ))?;
    if before.bytes().len() > crate::git::drafts::FILE_LIMIT {
        return Err(crate::i18n::text(
            "backend-safe-submodule-resolution-is-limited-to-an-8-mib-parent-index-use-246377",
        )
        .into());
    }
    // Child validation has already finished. Parse the captured immutable index,
    // prepare one private candidate, and recheck exact bytes/identity/policy at
    // the shared publisher's lock/capture boundary before any public installation.
    let private = tempfile::Builder::new()
        .prefix("gitcraft-submodule-index-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let candidate = private.path().join("candidate");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&candidate)
        .map_err(|e| e.to_string())?;
    file.write_all(before.bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    let env = [("GIT_INDEX_FILE", candidate.to_string_lossy().into_owned())];
    let run = |args: &[&str]| {
        super::capture_with_timeout(
            runner.repo,
            args,
            Vec::new(),
            crate::git::drafts::FILE_LIMIT,
            &env,
            Some(runner.control.clone()),
            budget.take()?,
        )?
        .checked(&[0])
    };
    stages_match(
        &run(&[
            "--literal-pathspecs",
            "ls-files",
            "--unmerged",
            "-z",
            "--",
            &conflict.path,
        ])?,
        conflict.stages.as_bytes(),
    )?;
    run(&[
        "--literal-pathspecs",
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{commit},{}", conflict.path),
    ])?;
    let active = || !runner.control.is_cancelled() && !crate::tasks::cancellation().is_cancelled();
    let after = super::object::files::read(&candidate, crate::git::drafts::FILE_LIMIT, &active)?
        .ok_or(crate::i18n::text(
            "backend-private-index-disappeared-40d3a9",
        ))?;
    let staged = run(&[
        "--literal-pathspecs",
        "ls-files",
        "--stage",
        "-z",
        "--",
        &conflict.path,
    ])?;
    stages_match(
        &staged,
        format!("160000 {commit} 0\t{}\0", conflict.path).as_bytes(),
    )?;
    if super::object::files::read(&candidate, crate::git::drafts::FILE_LIMIT, &active)?.as_deref()
        != Some(after.as_slice())
    {
        return Err(crate::i18n::text(
            "backend-private-index-changed-during-verification-parent-index-was-preser-455348",
        )
        .into());
    }
    hook(Phase::Prepared)?;
    conflict.snapshot.check(runner.repo)?;
    if !active() {
        return Err(crate::git::CANCELLED.into());
    }
    hook(Phase::BeforePublication)?;
    if !active() {
        return Err(crate::git::CANCELLED.into());
    }
    publish_if_current(before.unchanged(), || {
        let mut transaction = IndexTransaction::new(&before, after)?;
        // Publication may already be visible. Never abort after a lost receipt or
        // a post-publication cancellation; accept releases the captured owned lock.
        match transaction.accept() {
            Ok(evidence) => Ok(evidence),
            Err(e) => Err(crate::i18n::message(
                "backend-index-publication-needs-inspection-e-235dca",
                &[
                    ("detail", (e).to_string().into()),
                    ("transaction", (transaction.retain()).to_string().into()),
                ],
            )),
        }
    })
}

#[cfg(test)]
mod recording {
    use super::*;
    #[test]
    fn competing_stage_zero_resolution_refuses_before_publication() {
        let newer = "160000 newer-commit 0\tlib\0".to_owned();
        let mut current = newer.clone();
        let result = publish_if_current(false, || {
            current = "obsolete chosen resolution".into();
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(current, newer);
        assert!(stages_match(newer.as_bytes(), b"160000 old 2\tlib\0").is_err());
    }
    #[test]
    fn shared_acceptance_receipt_is_the_only_success_path() -> Result<(), String> {
        let mut publishes = 0;
        assert_eq!(
            publish_if_current(true, || {
                publishes += 1;
                Ok("retained evidence")
            })?,
            "retained evidence"
        );
        assert_eq!(publishes, 1);
        assert!(publish_if_current::<()>(true, || Err("unknown receipt: retain".into())).is_err());
        Ok(())
    }
}
