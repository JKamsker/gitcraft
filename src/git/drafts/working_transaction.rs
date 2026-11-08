//! Working changes use retained parents, capture/inspect and create-only install or
//! restore. All recovery entries remain addressable; Drop never mutates a public name.
use super::native_files::{self, CommitOutcome, Policies};
use super::*;
pub(super) type Contents = BTreeMap<String, Option<(String, Vec<u8>)>>;

// Resolve saved policy before the content no-op decision. Equal bytes can still
// require a full-policy restore, and a legacy snapshot must not clear its metadata.
fn intended_policy(
    path: &str,
    target: &Option<(String, Vec<u8>)>,
    current: &Policies,
    restored: Option<&Policies>,
) -> Result<Option<native_files::Policy>, String> {
    let policy = if target.is_some() && restored.is_some() {
        let saved = restored.and_then(|saved| saved.get(path)).ok_or_else(|| {
            crate::i18n::message(
                "backend-path-has-no-saved-full-access-policy-legacy-mode-only-undo-is-una-62db30",
                &[("path", (path).to_string().into())],
            )
        })?;
        saved.validate()?;
        if let Some((mode, _)) = target
            && (saved.mode() & 0o111 != 0) != (mode == "100755")
        {
            return Err(crate::i18n::message(
                "backend-path-saved-full-policy-conflicts-with-its-git-mode-snapshot-retai-b8a745",
                &[("path", (path).to_string().into())],
            ));
        }
        Some(saved.clone())
    } else {
        current.get(path).map(|policy| {
            target.as_ref().map_or_else(
                || policy.clone(),
                |(mode, _)| {
                    if (policy.mode() & 0o111 != 0) == (mode == "100755") {
                        policy.clone()
                    } else {
                        policy.with_git_mode(mode)
                    }
                },
            )
        })
    };
    if let Some(policy) = &policy {
        policy.validate()?;
    }
    Ok(policy)
}
fn replacement_needed(
    expected: &Option<(String, Vec<u8>)>,
    target: &Option<(String, Vec<u8>)>,
    current: Option<&native_files::Policy>,
    intended: Option<&native_files::Policy>,
) -> bool {
    expected != target || current != intended
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn native_apply(
    repo: &Path,
    before: &Contents,
    after: &Contents,
    policies: &Policies,
    check: &dyn Fn() -> Result<(), String>,
    commit: &dyn Fn(&str) -> Result<CommitOutcome, String>,
    restored_policy: Option<&Policies>,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    use native_files::{Directory, Replacement, Snapshot};
    use std::sync::Arc;
    let identity = crate::git::recovery::pinned_worktree(repo, active)?;
    if identity.host.is_some() {
        return Err(crate::i18n::text(
            "backend-working-folder-publication-requires-a-native-macos-linux-checkout-98c78b",
        )
        .into());
    }
    let pin = identity
        .root_pin
        .as_ref()
        .ok_or(crate::i18n::text("backend-missing-root-identity-d5af53"))?;
    let mut items = Vec::new();
    for (path, target) in after {
        valid_path(path)?;
        let expected = before.get(path).ok_or(crate::i18n::text(
            "backend-missing-before-file-snapshot-f90126",
        ))?;
        let policy = intended_policy(path, target, policies, restored_policy)?;
        if restored_policy.is_none()
            && !replacement_needed(expected, target, policies.get(path), policy.as_ref())
        {
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
            .ok_or(crate::i18n::text("backend-missing-parent-27f5e2"))?
            .to_owned();
        let directory = Arc::new(Directory::from_file(parent, held).map_err(|e| e.to_string())?);
        let old = Snapshot::read(&directory, &name, FILE_LIMIT)?;
        if old.as_ref().map(|s| &s.bytes) != expected.as_ref().map(|(_, bytes)| bytes)
            || old.as_ref().map(|s| &s.policy) != policies.get(path)
        {
            return Err(crate::i18n::message(
                "backend-path-contents-or-full-access-policy-changed-since-preview-983e44",
                &[("path", (path).to_string().into())],
            ));
        }
        if !replacement_needed(
            expected,
            target,
            old.as_ref().map(|snapshot| &snapshot.policy),
            policy.as_ref(),
        ) {
            continue;
        }
        items.push(Replacement::prepare(
            directory,
            name,
            old,
            target.as_ref().map(|(_, bytes)| bytes.as_slice()),
            policy.as_ref(),
            // A supplied full policy is already exact. Only a new file without
            // policy needs the candidate's default mode adjusted to its Git mode.
            if policy.is_none() {
                target.as_ref().map(|(mode, _)| mode.as_str())
            } else {
                None
            },
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
                return Err(crate::i18n::text(
                    "backend-working-folder-publication-cancelled-a8f541",
                )
                .into());
            }
            check()?;
            item.publish()?;
        }
        check()?;
        for item in &items {
            item.verify_installed()?;
        }
        commit(&evidence)
    })();
    match result {
        Ok(CommitOutcome::Committed) => Ok(crate::i18n::message(
            "backend-native-recovery-entries-retained-evidence-44a484",
            &[("evidence", (evidence).to_string().into())],
        )),
        Ok(CommitOutcome::Unknown(message)) => Err(crate::i18n::message(
            "backend-message-publication-is-uncertain-current-materialization-and-all-e89e19",
            &[
                ("message", (message).to_string().into()),
                ("evidence", (evidence).to_string().into()),
            ],
        )),
        Err(error) => {
            let failures = items
                .iter_mut()
                .rev()
                .filter_map(|item| item.rollback().err())
                .collect::<Vec<_>>();
            if failures.is_empty() {
                Err(crate::i18n::message(
                    "backend-error-owned-changes-restored-create-only-recovery-entries-retaine-d8c785",
                    &[
                        ("detail", (error).to_string().into()),
                        ("evidence", (evidence).to_string().into()),
                    ],
                ))
            } else {
                Err(crate::i18n::message(
                    "backend-error-attention-required-all-recovery-entries-retained-evidence-e5c552",
                    &[
                        ("detail", (error).to_string().into()),
                        ("failures", (failures.join("; ")).to_string().into()),
                        ("evidence", (evidence).to_string().into()),
                    ],
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
        &|_| Ok(CommitOutcome::Committed),
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
    commit: &dyn Fn(&str) -> Result<CommitOutcome, String>,
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
        Err(crate::i18n::text(
            "backend-native-working-folder-publication-is-unavailable-no-files-changed-254613",
        )
        .into())
    }
}

#[cfg(test)]
mod policy_noop_tests {
    use super::*;
    fn policy(platform: &str, gid: u32) -> native_files::Policy {
        let mut value = serde_json::json!({"platform":platform,"mode":384,"uid":1,"gid":gid,"access":{"entries":[]}});
        #[cfg(target_os = "macos")]
        {
            value["access"]["acl"] = serde_json::json!([]);
        }
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn pure_native_equal_contents_still_require_supported_saved_policy_and_restore_policy_changes()
    {
        let target = Some(("100644".into(), b"same bytes".to_vec()));
        let current: Policies = [("f".into(), policy(std::env::consts::OS, 10))].into();
        let saved: Policies = [("f".into(), policy(std::env::consts::OS, 20))].into();
        let desired = intended_policy("f", &target, &current, Some(&saved)).unwrap();
        assert!(replacement_needed(
            &target,
            &target,
            current.get("f"),
            desired.as_ref()
        ));
        let equal = intended_policy("f", &target, &current, Some(&current)).unwrap();
        assert!(!replacement_needed(
            &target,
            &target,
            current.get("f"),
            equal.as_ref()
        ));
        let missing = Policies::new();
        assert!(
            intended_policy("f", &target, &current, Some(&missing))
                .unwrap_err()
                .contains("no saved full access policy")
        );
        let unsupported: Policies = [("f".into(), policy("unsupported", 20))].into();
        assert!(intended_policy("f", &target, &current, Some(&unsupported)).is_err());
        // Metadata publication happens only after this required policy planning.
        // No filesystem/Git adapter is called by these equal-content regressions.
    }
}
