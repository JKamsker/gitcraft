//! Working changes use retained parents, capture/inspect and create-only install or
//! restore. All recovery entries remain addressable; Drop never mutates a public name.
use super::native_files::{self, CommitOutcome, Policies};
use super::*;
pub(super) type Contents = BTreeMap<String, Option<(String, Vec<u8>)>>;

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn native_apply(
    repo: &Path,
    before: &Contents,
    after: &Contents,
    policies: &Policies,
    check: &dyn Fn() -> Result<(), String>,
    commit: &dyn Fn() -> Result<CommitOutcome, String>,
    restored_policy: Option<&Policies>,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    use native_files::{Directory, Replacement, Snapshot};
    use std::sync::Arc;
    let identity = crate::git::recovery::pinned_worktree(repo, active)?;
    if identity.host.is_some() {
        return Err("Working-folder publication requires a native macOS/Linux checkout; no remote file changed".into());
    }
    let pin = identity.root_pin.as_ref().ok_or("Missing root identity")?;
    let mut items = Vec::new();
    for (path, target) in after {
        valid_path(path)?;
        let expected = before.get(path).ok_or("Missing before-file snapshot")?;
        if expected == target {
            continue;
        }
        let (held, name) = crate::git::recovery::pinned_parent(
            Path::new(&pin.physical),
            Path::new(&identity.common),
            path,
            pin,
        )?;
        let parent = Path::new(&pin.physical)
            .join(path)
            .parent()
            .ok_or("Missing parent")?
            .to_owned();
        let directory = Arc::new(Directory::from_file(parent, held).map_err(|e| e.to_string())?);
        let old = Snapshot::read(&directory, &name, FILE_LIMIT)?;
        if old.as_ref().map(|s| &s.bytes) != expected.as_ref().map(|(_, bytes)| bytes)
            || old.as_ref().map(|s| &s.policy) != policies.get(path)
        {
            return Err(format!(
                "{path} contents or full access policy changed since preview"
            ));
        }
        let policy = restored_policy
            .and_then(|p| p.get(path))
            .or_else(|| old.as_ref().map(|s| &s.policy))
            .cloned();
        if target.is_some() && restored_policy.is_some_and(|saved| !saved.contains_key(path)) {
            return Err(format!(
                "{path} has no saved full access policy; legacy mode-only undo is unavailable; no file changed"
            ));
        }
        if let Some(policy) = &policy {
            policy.validate()?;
        }
        items.push(Replacement::prepare(
            directory,
            name,
            old,
            target.as_ref().map(|(_, bytes)| bytes.as_slice()),
            policy.as_ref(),
            target.as_ref().map(|(mode, _)| mode.as_str()),
        )?);
    }
    let evidence = items
        .iter()
        .map(Replacement::evidence)
        .collect::<Vec<_>>()
        .join("; ");
    let result = (|| {
        check()?;
        for item in &mut items {
            if !active() {
                return Err("Working-folder publication cancelled".into());
            }
            check()?;
            item.publish()?;
        }
        check()?;
        for item in &items {
            item.verify_installed()?;
        }
        commit()
    })();
    match result {
        Ok(CommitOutcome::Committed) => Ok(format!("Native recovery entries retained: {evidence}")),
        Ok(CommitOutcome::Unknown(message)) => Err(format!(
            "{message}. Publication is uncertain: current materialization and all original/candidate entries retained; no rollback attempted. Recovery: {evidence}"
        )),
        Err(error) => {
            let failures = items
                .iter_mut()
                .rev()
                .filter_map(|item| item.rollback().err())
                .collect::<Vec<_>>();
            if failures.is_empty() {
                Err(format!(
                    "{error}. Owned changes restored create-only. Recovery entries retained: {evidence}"
                ))
            } else {
                Err(format!(
                    "{error}. Attention required: {}. All recovery entries retained: {evidence}",
                    failures.join("; ")
                ))
            }
        }
    }
}
pub(super) fn apply(
    repo: &Path,
    before: &Contents,
    after: &Contents,
    policies: &Policies,
    check: &dyn Fn() -> Result<(), String>,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    apply_with_commit(
        repo,
        before,
        after,
        policies,
        check,
        &|| Ok(CommitOutcome::Committed),
        None,
        active,
    )
}
pub(super) fn apply_with_commit(
    repo: &Path,
    before: &Contents,
    after: &Contents,
    policies: &Policies,
    check: &dyn Fn() -> Result<(), String>,
    commit: &dyn Fn() -> Result<CommitOutcome, String>,
    restored_policy: Option<&Policies>,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        native_apply(
            repo,
            before,
            after,
            policies,
            check,
            commit,
            restored_policy,
            active,
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (
            repo,
            before,
            after,
            policies,
            check,
            commit,
            restored_policy,
            active,
        );
        Err("Native working-folder publication is unavailable; no files changed".into())
    }
}
