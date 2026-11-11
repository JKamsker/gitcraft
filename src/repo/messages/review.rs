//! Finite review-family paint data from the accepted 204-entry source inventory.
//! Stage provenance and storyboard commands remain owned by their existing adapters.
//! Observation command/output and compare captions have no origin discriminator here:
//! never infer their meaning from spelling. The broader workflow map is owner supplied.

mod fixed;
pub(super) use fixed::ROWS as FIXED;

pub(super) fn fixed(raw: &str) -> Option<String> {
    FIXED
        .iter()
        .find(|(source, _)| *source == raw)
        .map(|(_, id)| crate::i18n::text(id).to_owned())
}

// Only these complete, application-owned envelopes enter the canonical regex map.
pub(super) const TEMPLATES: &[super::TemplateSpec] = &[
    (
        "(?s)^Choose the repository for (.*?)$",
        "repo-workflows-review-raw-choose-the-repository-for-3ff8de",
        &[("component", false)],
    ),
    (
        "(?s)^(.*?) needs 2–4 ordered stages: deployed, optional bridge, next$",
        "repo-workflows-review-raw-needs-2-4-ordered-stages-deployed-optional-bri-560f7e",
        &[("component", false)],
    ),
    (
        "(?s)^Hunk refused: (.*?)$",
        "repo-workflows-review-raw-hunk-refused-error-dbed1f",
        &[("detail", false)],
    ),
    (
        "(?s)^Commit (.*?)$",
        "repo-workflows-review-raw-commit-53bf7c",
        &[("commit", false)],
    ),
    (
        "(?s)^Comparison (.*?) → (.*?)$",
        "repo-workflows-review-raw-comparison-from-987fec",
        &[("from", false), ("to", false)],
    ),
    (
        "(?s)^Pull request \\#(.*?) · (.*?) → (.*?)$",
        "repo-workflows-review-raw-pull-request-number-from-to-c364ad",
        &[("number", false), ("from", false), ("to", false)],
    ),
    (
        "(?s)^Invalid rulebook from model: (.*?)$",
        "repo-workflows-review-raw-invalid-rulebook-from-model-e-133045",
        &[("detail", false)],
    ),
    (
        "(?s)^Reviewer echo cannot infer push option (.*?); review this operation explicitly$",
        "repo-workflows-review-raw-reviewer-echo-cannot-infer-push-option-word-re-fc6421",
        &[("option", false)],
    ),
    (
        "(?s)^Push mode (.*?) has no inferable source; review it explicitly$",
        "repo-workflows-review-raw-push-mode-mode-has-no-inferable-source-review-93d0aa",
        &[("mode", false)],
    ),
    (
        "(?s)^Push source (.*?) has no inspectable commit tree$",
        "repo-workflows-review-raw-push-source-src-has-no-inspectable-commit-tree-92780c",
        &[("source", false)],
    ),
    (
        "(?s)^Review receipt: (.*?)$",
        "repo-workflows-review-raw-review-receipt-e-4a5210",
        &[("detail", false)],
    ),
    (
        "(?s)^Invalid intent JSON: (.*?)$",
        "repo-workflows-review-raw-invalid-intent-json-e-96d3b1",
        &[("detail", false)],
    ),
    (
        "(?s)^Intent blob exceeds the ([0-9]+)\\-byte budget$",
        "repo-workflows-review-raw-intent-blob-exceeds-the-budget-byte-budget-259d7d",
        &[("budget", true)],
    ),
    (
        "(?s)^Latest independent cleanup (.*?) \\((.*?)\\) · (.*?)$",
        "repo-workflows-review-raw-latest-independent-cleanup-c07c47",
        &[("name", false), ("id", false), ("detail", false)],
    ),
    (
        "(?s)^Container ownership inspection failed: (.*?)$",
        "repo-workflows-review-raw-container-ownership-inspection-failed-b2504e",
        &[("detail", false)],
    ),
    (
        "(?s)^Cleanup needs attention: (.*?)$",
        "repo-workflows-review-raw-cleanup-needs-attention-e-9cf993",
        &[("detail", false)],
    ),
    (
        "(?s)^Could not preserve reproducer cleanup evidence for (.*?): (.*?); (.*?)$",
        "repo-workflows-review-raw-could-not-preserve-reproducer-cleanup-evidence-c910e4",
        &[("name", false), ("detail", false), ("result", false)],
    ),
    (
        "(?s)^Sandbox did not complete: (.*?)$",
        "repo-workflows-review-raw-sandbox-did-not-complete-error-95c251",
        &[("detail", false)],
    ),
    (
        "(?s)^Invalid preserved reproducer evidence: (.*?)$",
        "repo-workflows-review-raw-invalid-preserved-reproducer-evidence-e-4bd6de",
        &[("detail", false)],
    ),
    // reproducer_source validates a colon-free path and a full 40/64-digit hex OID.
    (
        "(?s)^Submodule ([^:]+?) at ([0-9A-Fa-f]{40}|[0-9A-Fa-f]{64}): (.*?)\\. Its pinned objects must already be present locally; gitcraft never clones or checks it out\\.$",
        "repo-workflows-review-raw-submodule-path-at-oid-e-its-pinned-objects-mus-6a547c",
        &[("path", false), ("oid", false), ("detail", false)],
    ),
    (
        "(?s)^Unsafe or unsupported sandbox source path: (.*?)$",
        "repo-workflows-review-raw-unsafe-or-unsupported-sandbox-source-path-path-6890f5",
        &[("path", false)],
    ),
    (
        "(?s)^Unsafe symlink (.*?)$",
        "repo-workflows-review-raw-unsafe-symlink-path-f052a0",
        &[("path", false)],
    ),
    (
        "(?s)^Symlink (.*?) escapes the source root$",
        "repo-workflows-review-raw-symlink-path-escapes-the-source-root-fb2203",
        &[("path", false)],
    ),
    (
        "(?s)^Archive member (.*?) traverses a symlink$",
        "repo-workflows-review-raw-archive-member-path-traverses-a-symlink-d96181",
        &[("path", false)],
    ),
    (
        "(?s)^Invalid review evidence: (.*?)$",
        "repo-workflows-review-raw-invalid-review-evidence-e-d265ce",
        &[("detail", false)],
    ),
    (
        "(?s)^Invalid shared storyboard: (.*?)$",
        "repo-workflows-review-raw-invalid-shared-storyboard-e-94d7b6",
        &[("detail", false)],
    ),
    (
        "(?s)^([0-9]+) additional invalid records omitted$",
        "repo-workflows-review-raw-additional-invalid-records-omitted-f1c696",
        &[("count", true)],
    ),
];

/// Source-owned Rust dependency witnesses/limitations; never a generic diagnostic parser.
/// The only caller paints the report produced by rust_dependencies. Each value is opaque.
pub(super) fn dependency(raw: &str) -> String {
    if let Some((path, detail)) = raw
        .strip_suffix(')')
        .and_then(|s| s.split_once(": Rust syntax not parsed ("))
    {
        // The same separator is legal inside a Rust path or parse diagnostic.
        // A second occurrence leaves the source fields ambiguous; keep the whole raw value.
        if detail.contains(": Rust syntax not parsed (") {
            return raw.to_owned();
        }
        return crate::i18n::message(
            "repo-workflows-review-raw-path-rust-syntax-not-parsed-e-110a0e",
            &[("path", path.into()), ("detail", detail.into())],
        );
    }
    if let Some(chain) = raw.strip_suffix(": static call targets changed") {
        return crate::i18n::message(
            "repo-workflows-review-raw-static-call-targets-changed-8fc13b",
            &[("chain", chain.into())],
        );
    }
    if let Some((chain, dependency)) = raw
        .strip_suffix(" changed")
        .and_then(|s| s.rsplit_once(" → "))
    {
        return crate::i18n::message(
            "repo-workflows-review-raw-changed-41e60a",
            &[("chain", chain.into()), ("dependency", dependency.into())],
        );
    }
    fixed(raw).unwrap_or_else(|| raw.to_owned())
}

/// The quarantine vector is the producer's reference/error envelope (or omitted count).
/// Its inner diagnostics use only this review family's finite envelopes. Do not apply
/// operation recognition to refs, imported JSON, provider diagnostics or arbitrary commands.
pub(super) fn intent_warning(raw: &str) -> String {
    if let Some((reference, detail)) = raw.split_once(": ") {
        let detail = super::review_owned(detail);
        return crate::i18n::message(
            "repo-workflows-review-raw-reference-error-8c337e",
            &[("reference", reference.into()), ("detail", detail.into())],
        );
    }
    super::review_owned(raw)
}

#[cfg(test)]
mod tests;
