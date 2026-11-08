//! Presentation only. Call after raw classification/receipt handling, at the paint boundary.
//! Unknown/provider/user text is returned byte-for-byte; never feed display text to guards.
use crate::i18n::message;

/// The finite application diagnostics and finding details emitted by this family.
/// Authentication receipts deliberately stay opaque: consume their proof before displaying.
pub(crate) fn diagnostic(raw: &str) -> String {
    match raw {
        ".gitattributes is not a regular file; resolve that before converting to LFS." => {
            return crate::i18n::text("interaction-guards-push-lfs-attributes-file").into();
        }
        "git-lfs did not produce a pointer; install git-lfs before converting history." => {
            return crate::i18n::text("interaction-guards-push-lfs-no-pointer").into();
        }
        "push.default=nothing: choose an explicit branch push." => {
            return crate::i18n::text("interaction-guards-push-default-nothing").into();
        }
        ".lfsconfig must not pass through a content filter when reviewing endpoint configuration." =>
        {
            return crate::i18n::text("interaction-guards-push-lfs-filter").into();
        }
        "The local push preview could not be completed. Fetch missing history explicitly, then try again; no push was run." =>
        {
            return crate::i18n::text("interaction-guards-push-preview-failed").into();
        }
        "The local push configuration could not be read; no push was run." => {
            return crate::i18n::text("interaction-guards-push-config-read").into();
        }
        "Choose one complete destination ref, such as refs/heads/main." => {
            return crate::i18n::text("interaction-guards-push-complete-destination").into();
        }
        "The push routing configuration exceeds the preview's size limit." => {
            return crate::i18n::text("interaction-guards-push-routing-limit").into();
        }
        "An identity or routing value contains control characters. Correct it before opening a push preview." =>
        {
            return crate::i18n::text("interaction-guards-push-control-characters").into();
        }
        "This push option needs an explicit single-ref preview; multi-ref, matching, mirror, atomic and recursive pushes are not sent by this checklist." =>
        {
            return crate::i18n::text("interaction-guards-push-single-ref").into();
        }
        "Preview one destination at a time; no push was run." => {
            return crate::i18n::text("interaction-guards-push-one-destination").into();
        }
        "Preview explicit refs without temporary push configuration overrides." => {
            return crate::i18n::text("interaction-guards-push-no-overrides").into();
        }
        "Complete this local operation first, then preview its push separately." => {
            return crate::i18n::text("interaction-guards-push-finish-operation").into();
        }
        "Preview one push at a time; no push was run." => {
            return crate::i18n::text("interaction-guards-push-one-push").into();
        }
        "Choose an explicit remote and source ref when HEAD is detached." => {
            return crate::i18n::text("interaction-guards-push-detached-route").into();
        }
        "Choose a remote before pushing this branch." => {
            return crate::i18n::text("interaction-guards-push-choose-remote").into();
        }
        "The push remote is no longer configured; reopen the preview." => {
            return crate::i18n::text("interaction-guards-push-remote-removed").into();
        }
        "Mirror pushes need their own multi-ref review; no push was run." => {
            return crate::i18n::text("interaction-guards-push-mirror").into();
        }
        "Preview at most eight configured push addresses; split larger mirror groups into separate remotes." =>
        {
            return crate::i18n::text("interaction-guards-push-address-limit").into();
        }
        "The remote has no supported push address." => {
            return crate::i18n::text("interaction-guards-push-no-address").into();
        }
        "Choose an explicit source ref when HEAD is detached." => {
            return crate::i18n::text("interaction-guards-push-detached-source").into();
        }
        "The remote pushes multiple refs. Choose one explicit branch in its menu for preflight." => {
            return crate::i18n::text("interaction-guards-push-multiple-refs").into();
        }
        "Matching pushes need a separate preview for each branch." => {
            return crate::i18n::text("interaction-guards-push-matching").into();
        }
        "The upstream branch has a different name; choose its explicit branch push from the menu." =>
        {
            return crate::i18n::text("interaction-guards-push-upstream-name").into();
        }
        "Choose one complete ref; wildcard pushes are not sent." => {
            return crate::i18n::text("interaction-guards-push-no-wildcards").into();
        }
        "A deletion cannot set an upstream." => {
            return crate::i18n::text("interaction-guards-push-delete-upstream").into();
        }
        "Deletion needs an exact ref lease" => {
            return crate::i18n::text("interaction-guards-push-delete-lease-required").into();
        }
        "The deletion lease must name this exact ref and object." => {
            return crate::i18n::text("interaction-guards-push-delete-lease").into();
        }
        "The fetched target differs from the deletion lease; reopen its original action." => {
            return crate::i18n::text("interaction-guards-push-delete-target-changed").into();
        }
        "Fetch this exact target before deleting it; no remote tip was inspected." => {
            return crate::i18n::text("interaction-guards-push-delete-fetch").into();
        }
        "The outgoing history exceeds 10,000 commits; split the push or fetch its exact target before checking." =>
        {
            return crate::i18n::text("interaction-guards-push-commit-limit").into();
        }
        "Committed .lfsconfig must be a regular file before its endpoint can be reviewed." => {
            return crate::i18n::text("interaction-guards-push-lfs-file").into();
        }
        "The LFS endpoint contains control characters." => {
            return crate::i18n::text("interaction-guards-push-lfs-control").into();
        }
        "No push target" => return crate::i18n::text("interaction-guards-push-no-target").into(),
        "Checkpoint reference list exceeds 1 MiB; narrow publication before pushing" => {
            return crate::i18n::text("interaction-guards-push-checkpoint-limit").into();
        }
        "Checkpoint publication may disclose prompts, local paths, assistant summaries and tool activity. This exact object or route contains agent checkpoint history. Review its contents and recipients explicitly before sending; local prompt/session viewing itself never publishes transcripts." =>
        {
            return crate::i18n::text("interaction-guards-push-transcript").into();
        }
        "Likely secret in annotated tag message; value withheld. Recreate the tag before pushing." =>
        {
            return crate::i18n::text("interaction-guards-push-tag-secret").into();
        }
        "Outgoing commit metadata exceeds the bounded preview size; shorten large messages or split the push." =>
        {
            return crate::i18n::text("interaction-guards-push-metadata-limit").into();
        }
        "Commit metadata could not be read safely." => {
            return crate::i18n::text("interaction-guards-push-metadata-unsafe").into();
        }
        "Commit has no embedded signature; this profile signs commits." => {
            return crate::i18n::text("interaction-guards-push-unsigned").into();
        }
        "Temporary WIP/fixup message remains in outgoing history." => {
            return crate::i18n::text("interaction-guards-push-wip").into();
        }
        "Likely secret in commit message (value withheld)." => {
            return crate::i18n::text("interaction-guards-push-message-secret").into();
        }
        "More than 2,000 findings need review. Repair or split this history before pushing." => {
            return crate::i18n::text("interaction-guards-push-finding-limit").into();
        }
        "More than 50,000 outgoing objects need checking. Split the push or fetch the exact target." =>
        {
            return crate::i18n::text("interaction-guards-push-object-limit").into();
        }
        "Unknown outgoing blob size" => {
            return crate::i18n::text("interaction-guards-push-blob-size").into();
        }
        "Unsupported LFS object hash" => {
            return crate::i18n::text("interaction-guards-push-lfs-hash-unsupported").into();
        }
        "Malformed LFS object hash" => {
            return crate::i18n::text("interaction-guards-push-lfs-hash-malformed").into();
        }
        "The push configuration changed while checking. Reopen preflight." => {
            return crate::i18n::text("interaction-guards-push-config-changed").into();
        }
        "The source changed while checking. Reopen preflight." => {
            return crate::i18n::text("interaction-guards-push-source-changed").into();
        }
        "The selected push endpoint, commit or advertised lease changed. Fetch the exact target if needed, then reopen the original push action; nothing was sent." =>
        {
            return crate::i18n::text("interaction-guards-push-review-changed").into();
        }
        "Invalid outgoing object" => {
            return crate::i18n::text("interaction-guards-push-object-invalid").into();
        }
        "The content scan failed or exceeded its finding limit; no push was run." => {
            return crate::i18n::text("interaction-guards-push-scan-failed").into();
        }
        "Reopen push preflight" => {
            return crate::i18n::text("interaction-guards-push-reopen").into();
        }
        "This preview expired; reopen push preflight." => {
            return crate::i18n::text("interaction-guards-push-expired").into();
        }
        "Commit or remove .lfsconfig changes, and clear skip/assume-unchanged flags on it before reviewing a push." =>
        {
            return crate::i18n::text("interaction-guards-push-lfs-working").into();
        }
        "The working .lfsconfig differs from the inspected commit. Commit or stash that routing change before pushing." =>
        {
            return crate::i18n::text("interaction-guards-push-lfs-differs").into();
        }
        "The checked-out branch moved. Reopen push preflight." => {
            return crate::i18n::text("interaction-guards-push-branch-moved").into();
        }
        "The branch, checkout, identity or push configuration changed. Reopen push preflight; nothing was pushed." =>
        {
            return crate::i18n::text("interaction-guards-push-state-changed").into();
        }
        "The push addresses changed. Reopen push preflight." => {
            return crate::i18n::text("interaction-guards-push-addresses-changed").into();
        }
        "Fetch this exact target before force pushing: no remote lease was inspected." => {
            return crate::i18n::text("interaction-guards-push-force-fetch").into();
        }
        "The fetched target is not an ancestor. Keep its commits or deliberately choose force-with-lease." =>
        {
            return crate::i18n::text("interaction-guards-push-not-ancestor").into();
        }
        "Reviewer echo belongs to a different exact push; prepare again" => {
            return crate::i18n::text("interaction-guards-push-echo-mismatch").into();
        }
        "Reviewed route was not captured" => {
            return crate::i18n::text("interaction-guards-push-route-missing").into();
        }
        "Push completed to the reviewed target(s)." => {
            return crate::i18n::text("interaction-guards-push-completed").into();
        }
        "No clock" => return crate::i18n::text("interaction-guards-push-clock").into(),
        "Could not prepare the history fix" => {
            return crate::i18n::text("interaction-guards-push-prepare-fix").into();
        }
        "Could not prepare the remote history fix" => {
            return crate::i18n::text("interaction-guards-push-prepare-remote-fix").into();
        }
        "No fix folder" => {
            return crate::i18n::text("interaction-guards-push-no-fix-folder").into();
        }
        "Check out this branch before fixing its history, then reopen preflight." => {
            return crate::i18n::text("interaction-guards-push-checkout-fix").into();
        }
        "Commit or stash all working changes and untracked files before rewriting outgoing history." =>
        {
            return crate::i18n::text("interaction-guards-push-clean-fix").into();
        }
        "Fetch the target first" => {
            return crate::i18n::text("interaction-guards-push-fetch-first").into();
        }
        "Merged the fetched target; reopen preflight. Ctrl+Z undoes this merge." => {
            return crate::i18n::text("interaction-guards-push-merged").into();
        }
        "The merge needs attention. Resolve conflicts or abort it before reopening preflight." => {
            return crate::i18n::text("interaction-guards-push-merge-attention").into();
        }
        "Unknown history fix" => {
            return crate::i18n::text("interaction-guards-push-unknown-fix").into();
        }
        "Resolve likely-secret findings before moving files to LFS; LFS must not conceal a known secret." =>
        {
            return crate::i18n::text("interaction-guards-push-secrets-before-lfs").into();
        }
        "History rewrite failed before the branch was updated; check signing availability or missing local objects." =>
        {
            return crate::i18n::text("interaction-guards-push-rewrite-failed").into();
        }
        "Malformed commit" => {
            return crate::i18n::text("interaction-guards-push-malformed-commit").into();
        }
        "No commit tree" => return crate::i18n::text("interaction-guards-push-no-tree").into(),
        "LFS conversion supports regular files only; no history was changed." => {
            return crate::i18n::text("interaction-guards-push-lfs-regular").into();
        }
        "Could not quote attributes" => {
            return crate::i18n::text("interaction-guards-push-quote-attributes").into();
        }
        "Malformed author metadata" => {
            return crate::i18n::text("interaction-guards-push-malformed-author").into();
        }
        "Set the expected host author name and email in Profile settings before amending history." =>
        {
            return crate::i18n::text("interaction-guards-push-set-author").into();
        }
        "There are no outgoing commits to fix" => {
            return crate::i18n::text("interaction-guards-push-no-commits").into();
        }
        "Outgoing history fixed. Reopen preflight to verify it; Ctrl+Z restores the previous history." =>
        {
            return crate::i18n::text("interaction-guards-push-fixed").into();
        }
        "Automatic remote repair requires completion-aware ownership; no remote repair was dispatched. Preview and reviewed push remain available." =>
        {
            return crate::i18n::text("interaction-guards-push-remote-ownership").into();
        }
        "Safe repair requires UTF-8 metadata paths" => {
            return crate::i18n::text("interaction-guards-push-utf8-metadata").into();
        }
        "Safe repair requires metadata paths without control characters" => {
            return crate::i18n::text("interaction-guards-push-metadata-controls").into();
        }
        "The guarded Git repair command was refused; tool diagnostics were withheld." => {
            return crate::i18n::text("interaction-guards-push-repair-refused").into();
        }
        "Could not snapshot the original index" => {
            return crate::i18n::text("interaction-guards-push-index-snapshot").into();
        }
        "Could not read the original index" => {
            return crate::i18n::text("interaction-guards-push-index-read").into();
        }
        "Index recovery snapshot exceeds 64 MiB" => {
            return crate::i18n::text("interaction-guards-push-index-limit").into();
        }
        "Could not save the owned index recovery snapshot" => {
            return crate::i18n::text("interaction-guards-push-index-save").into();
        }
        "Saved index dependency was not an absolute path; repair refused" => {
            return crate::i18n::text("interaction-guards-push-index-dependency").into();
        }
        "Guarded repair requires Git 2.50 or later plus a successful native HEAD/ref capability proof. No repair reference update was launched." =>
        {
            return crate::i18n::text("interaction-guards-push-git-capability").into();
        }
        "This Git reference/object backend cannot be proven safe for automatic repair" => {
            return crate::i18n::text("interaction-guards-push-backend-proof").into();
        }
        "Automatic repair requires a direct local branch reference" => {
            return crate::i18n::text("interaction-guards-push-direct-branch").into();
        }
        "Git did not provide absolute metadata paths; safe repair was refused" => {
            return crate::i18n::text("interaction-guards-push-absolute-metadata").into();
        }
        "Clear sparse/skip-worktree/assume-unchanged index flags before automatic tree repair" => {
            return crate::i18n::text("interaction-guards-push-index-flags").into();
        }
        "Safe repair requires UTF-8 changed paths" => {
            return crate::i18n::text("interaction-guards-push-utf8-changed").into();
        }
        "Could not create owned repair evidence" => {
            return crate::i18n::text("interaction-guards-push-create-evidence").into();
        }
        "Owned index path is not UTF-8" => {
            return crate::i18n::text("interaction-guards-push-index-utf8").into();
        }
        "Could not write recovery instructions" => {
            return crate::i18n::text("interaction-guards-push-recovery-write").into();
        }
        "Could not write the private repair hook" => {
            return crate::i18n::text("interaction-guards-push-hook-write").into();
        }
        "Could not protect repair hook" => {
            return crate::i18n::text("interaction-guards-push-hook-protect").into();
        }
        "Could not arm local repair" => {
            return crate::i18n::text("interaction-guards-push-arm-local").into();
        }
        "Could not write capability input" => {
            return crate::i18n::text("interaction-guards-push-capability-input").into();
        }
        "Could not arm capability proof" => {
            return crate::i18n::text("interaction-guards-push-arm-capability").into();
        }
        "Could not write owned hook-command proof" => {
            return crate::i18n::text("interaction-guards-push-hook-command-proof").into();
        }
        "Could not protect owned capability hook" => {
            return crate::i18n::text("interaction-guards-push-capability-protect").into();
        }
        "Could not prepare hook stdin proof" => {
            return crate::i18n::text("interaction-guards-push-hook-stdin").into();
        }
        "Git did not prove original-hook command/stdin support; no user update was launched" => {
            return crate::i18n::text("interaction-guards-push-hook-support").into();
        }
        "This Git/backend did not prove private hook selection and an atomic current-HEAD/branch update. No user reference update was launched." =>
        {
            return crate::i18n::text("interaction-guards-push-native-proof").into();
        }
        "Missing native transaction input proof" => {
            return crate::i18n::text("interaction-guards-push-missing-proof").into();
        }
        "The native HEAD/branch transaction result was not proven; no user update was launched" => {
            return crate::i18n::text("interaction-guards-push-transaction-proof").into();
        }
        "No native checkout/ref capability acknowledgement; repair refused before dispatch" => {
            return crate::i18n::text("interaction-guards-push-capability-ack").into();
        }
        "Could not establish the remote repair metadata result; no repair transaction was dispatched" =>
        {
            return crate::i18n::text("interaction-guards-push-remote-metadata-result").into();
        }
        "Remote repair metadata is unavailable or oversized" => {
            return crate::i18n::text("interaction-guards-push-remote-metadata").into();
        }
        "Remote repair requires absolute host metadata paths" => {
            return crate::i18n::text("interaction-guards-push-remote-absolute").into();
        }
        "Guarded remote repair requires Git 2.50 or later on the repository host" => {
            return crate::i18n::text("interaction-guards-push-remote-git").into();
        }
        "Repair object identity is invalid" => {
            return crate::i18n::text("interaction-guards-push-repair-oid").into();
        }
        "Shared-index path is not absolute" => {
            return crate::i18n::text("interaction-guards-push-shared-absolute").into();
        }
        "Unsupported remote repair backend" => {
            return crate::i18n::text("interaction-guards-push-remote-backend").into();
        }
        "Remote repair requires UTF-8 changed paths" => {
            return crate::i18n::text("interaction-guards-push-remote-utf8").into();
        }
        "Remote repair wrapper exceeds its size limit" => {
            return crate::i18n::text("interaction-guards-push-wrapper-limit").into();
        }
        "No index parent" => {
            return crate::i18n::text("interaction-guards-push-index-parent").into();
        }
        "Resolve pending remote repairs before starting another" => {
            return crate::i18n::text("interaction-guards-push-pending").into();
        }
        _ => {}
    }
    dynamic(raw).unwrap_or_else(|| raw.to_owned())
}

fn dynamic(raw: &str) -> Option<String> {
    if let Some(text) = raw
        .strip_prefix("Author email differs from the profile policy for ")
        .and_then(|s| s.strip_suffix("."))
    {
        let rendered = (|| {
            let target = target_label_display(text);
            Some(message(
                "interaction-guards-push-identity",
                &[("target", target.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" Git blob; consider Git LFS."))
    {
        let rendered = (|| {
            let size = text;
            Some(message(
                "interaction-guards-push-large",
                &[("size", size.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Likely secret at line ")
        .and_then(|s| s.strip_suffix("; value withheld."))
    {
        let rendered = (|| {
            let line = text;
            if line.is_empty() || !line.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let line = line.parse::<u64>().ok()?;
            Some(message(
                "interaction-guards-push-secret-line",
                &[("line", line.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw.strip_prefix("Force push would drop ").and_then(|s| {
        s.strip_suffix(
            " by other authors. Based on the fetched target; the lease must still match.",
        )
    }) {
        let rendered = (|| {
            let (count, text) = text.split_once(" remote commits, ")?;
            if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let count = count.parse::<u64>().ok()?;
            let others = text;
            if others.is_empty() || !others.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let others = others.parse::<u64>().ok()?;
            Some(message(
                "interaction-guards-push-remote-history",
                &[("count", count.into()), ("others", others.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw.strip_prefix("The local repair did not establish complete owned acknowledgement. Preserve concurrent edits; inspect recovery.txt and saved index/dependency in ").and_then(|s| s.strip_suffix(". No automatic rollback overwrote later work.")) {
        let rendered = (|| {
            let directory = text;
            Some(message("interaction-guards-push-local-uncertain", &[("directory", directory.into())]))
        })();
        if rendered.is_some() { return rendered; }
    }
    if let Some(text) = raw.strip_prefix("A previous remote repair has no confirmed terminal outcome. Recheck after the host finishes, or inspect recovery in ").and_then(|s| s.strip_suffix(". No replacement repair was dispatched.")) {
        let rendered = (|| {
            let directory = text;
            Some(message("interaction-guards-push-remote-held", &[("directory", directory.into())]))
        })();
        if rendered.is_some() { return rendered; }
    }
    if let Some(text) = raw.strip_prefix("The remote repair outcome is unknown. It may still be running; its owned guard and recovery remain in ").and_then(|s| s.strip_suffix(". Recheck after the host finishes; no replacement or cleanup was dispatched.")) {
        let rendered = (|| {
            let directory = text;
            Some(message("interaction-guards-push-remote-uncertain", &[("directory", directory.into())]))
        })();
        if rendered.is_some() { return rendered; }
    }
    if let Some(text) = raw.strip_prefix("Remote repair did not establish complete success. Inspect actual HEAD and concurrent edits, plus recovery in ").and_then(|s| s.strip_suffix("; no rollback overwrote later work.")) {
        let rendered = (|| {
            let directory = text;
            Some(message("interaction-guards-push-remote-incomplete", &[("directory", directory.into())]))
        })();
        if rendered.is_some() { return rendered; }
    }
    None
}

/// Paint only after authentication receipt classification/extraction has completed.
pub(crate) fn reviewed_auth_failure() -> &'static str {
    crate::i18n::text("interaction-guards-push-authentication-failed")
}

/// Uses the canonical redaction helper before translating its application-owned label.
pub(crate) fn target(url: &str) -> String {
    target_label_display(&super::target_label(url))
}

fn target_label_display(raw: &str) -> String {
    if let Some(path) = raw.strip_prefix("Local repository: ") {
        let path = if path == "[sensitive path]" {
            crate::i18n::text("interaction-guards-push-sensitive-path")
        } else {
            path
        };
        message(
            "interaction-guards-push-local-target",
            &[("path", path.into())],
        )
    } else if let Some(host) = raw.strip_suffix("/[sensitive path]") {
        format!(
            "{host}/{}",
            crate::i18n::text("interaction-guards-push-sensitive-path")
        )
    } else {
        raw.to_owned()
    }
}

/// Canonical path redaction/escaping with a localized redaction placeholder.
pub(crate) fn path(path: &str) -> String {
    let raw = super::display_path(path);
    if raw == "[sensitive path]" {
        crate::i18n::text("interaction-guards-push-sensitive-path").into()
    } else {
        raw
    }
}

#[cfg(test)]
mod tests;
