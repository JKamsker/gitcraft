//! Finite presentation adapter for raw operation identities. Never feed the result back to
//! run_ops, operation classifiers, rewrite matching, timeline persistence or retry dispatch.
//! Unknown/custom labels remain opaque. Ref/path arguments are interpolated without changes.

pub(crate) fn display(raw: &str) -> String {
    known(raw, 0).unwrap_or_else(|| raw.to_owned())
}

fn known(raw: &str, depth: usize) -> Option<String> {
    let id = match raw {
        "Fetch" => "interaction-operation-fetch",
        "Pull" => "interaction-operation-pull",
        "Push" => "interaction-operation-push",
        "Commit" => "interaction-operation-commit",
        "Amend" => "interaction-operation-amend",
        "Stage" => "interaction-operation-stage",
        "Stage all" => "interaction-operation-stage-all",
        "Unstage" => "interaction-operation-unstage",
        "Unstage all" => "interaction-operation-unstage-all",
        "Discard" => "interaction-operation-discard",
        "Stash" => "interaction-operation-stash",
        "Pop stash" => "interaction-operation-pop-stash",
        "Apply stash" => "interaction-operation-apply-stash",
        "Drop stash" => "interaction-operation-drop-stash",
        "Set identity" => "interaction-operation-set-identity",
        "Use the profile's author" => "interaction-operation-profile-author",
        "Mark resolved" => "interaction-operation-mark-resolved",
        "Reload" => "interaction-operation-reload",
        "Sync" => "interaction-operation-sync",
        "Push for review" => "interaction-operation-push-review",
        "Push, skipping hooks" => "interaction-operation-skip-hooks",
        "Apply patch" => "interaction-operation-apply-patch",
        "Cherry-pick" => "interaction-operation-cherry-pick",
        "Revert" => "interaction-operation-revert",
        "Rebase" => "interaction-operation-rebase",
        "Merge" => "interaction-operation-merge",
        "DVC: Pull" => "interaction-operation-dvc-pull",
        "Abort merge" => "interaction-operation-abort-merge",
        "Abort rebase" => "interaction-operation-abort-rebase",
        "Abort cherry-pick" => "interaction-operation-abort-cherry-pick",
        "Abort revert" => "interaction-operation-abort-revert",
        "Abort am" => "interaction-operation-abort-am",
        "Continue merge" => "interaction-operation-continue-merge",
        "Continue rebase" => "interaction-operation-continue-rebase",
        "Continue cherry-pick" => "interaction-operation-continue-cherry-pick",
        "Continue revert" => "interaction-operation-continue-revert",
        "Continue am" => "interaction-operation-continue-am",
        _ => "",
    };
    if !id.is_empty() {
        return Some(crate::i18n::text(id).to_owned());
    }
    // Wrappers translate only when the enclosed label is itself known. Custom commands
    // named e.g. "Undo deployment script" retain their original text in full.
    if depth < 8 {
        for (prefix, id) in [
            ("Undo ", "interaction-operation-undo"),
            ("Redo ", "interaction-operation-redo"),
            ("Preparing ", "interaction-operation-preparing"),
            ("Push after ", "interaction-operation-push-after"),
        ] {
            if let Some(inner) = raw.strip_prefix(prefix)
                && let Some(operation) = known(inner, depth + 1)
            {
                return Some(crate::i18n::message(id, &[("operation", operation.into())]));
            }
        }
    }
    for (prefix, delimiter, id) in [
        ("Push ", " to ", "interaction-operation-push-to"),
        ("Merge ", " into ", "interaction-operation-merge-into"),
        ("Rebase ", " onto ", "interaction-operation-rebase-onto"),
        (
            "Fast-forward ",
            " to ",
            "interaction-operation-fast-forward",
        ),
    ] {
        if let Some((branch, target)) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.split_once(delimiter))
            && !branch.is_empty()
            && !target.is_empty()
        {
            let target_name = if prefix == "Push " {
                "remote"
            } else {
                "target"
            };
            return Some(crate::i18n::message(
                id,
                &[("branch", branch.into()), (target_name, target.into())],
            ));
        }
    }
    for (prefix, suffix, id) in [
        (
            "Checkout and pull ",
            "",
            "interaction-operation-checkout-pull",
        ),
        ("Checkout ", "", "interaction-operation-checkout"),
        ("Create branch ", "", "interaction-operation-create-branch"),
        ("Delete branch ", "", "interaction-operation-delete-branch"),
        ("Rename branch ", "", "interaction-operation-rename-branch"),
        (
            "Create release tag ",
            "",
            "interaction-operation-release-tag",
        ),
        ("Create tag ", "", "interaction-operation-create-tag"),
        ("Delete tag ", "", "interaction-operation-delete-tag"),
        ("Cherry-pick ", "", "interaction-operation-pick"),
        ("Revert ", "", "interaction-operation-revert-item"),
        ("Stop tracking ", "", "interaction-operation-stop-tracking"),
        ("Ignore ", "", "interaction-operation-ignore"),
        ("Track ", " in LFS", "interaction-operation-track-lfs"),
        ("Apply ", "", "interaction-operation-apply-file"),
    ] {
        if let Some(value) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
            && !value.is_empty()
        {
            return Some(crate::i18n::message(id, &[("value", value.into())]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_labels_keep_raw_identity_refs_and_unknown_commands() {
        crate::i18n::set_language("de");
        let raw = "Push feature/日本語 to origin".to_owned();
        assert_eq!(display(&raw), "feature/日本語 nach origin pushen");
        assert_eq!(raw, "Push feature/日本語 to origin");
        assert_eq!(display("Abort rebase"), "Rebase abbrechen");
        assert_eq!(display("Continue merge"), "Merge fortsetzen");
        assert_eq!(display("Track *.psd in LFS"), "*.psd in LFS verwalten");
        assert_eq!(
            display("Undo Create branch feature/ü"),
            "Rückgängig: Branch feature/ü erstellen"
        );
        for unknown in [
            "deploy --target=de",
            "Undo deployment script",
            "Push ",
            "外部コマンド",
        ] {
            let shown = display(unknown);
            assert_eq!(shown, unknown);
        }
        crate::i18n::set_language("en-US");
        assert_eq!(display(&raw), raw);
        assert_eq!(
            display("Undo Create branch feature/ü"),
            "Undo Create branch feature/ü"
        );
    }
}
