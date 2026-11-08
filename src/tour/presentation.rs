//! Paint adapters for raw, locale-independent tour snapshots. Never localize producers:
//! deferred results can outlive a language change, and classifiers compare their raw values.
use super::{Day, Folder};

/// Only exact application diagnostics are recognized. Unknown/Git/provider output is opaque.
pub(crate) fn diagnostic(raw: &str) -> String {
    let id = match raw {
        "Invalid repository HEAD" => "interaction-tour-invalid-head",
        "Unsupported current calendar day" => "interaction-tour-unsupported-day",
        "Repository tour cancelled" => "interaction-tour-cancelled",
        "Invalid repository tree record" => "interaction-tour-invalid-tree",
        "Invalid document object ID" => "interaction-tour-invalid-document-id",
        "Invalid object ID" => "interaction-tour-invalid-id",
        "Invalid document size" => "interaction-tour-invalid-document-size",
        "Document is larger than the 256 KiB reading limit" => {
            "interaction-tour-document-too-large"
        }
        "Analysis cancelled" => "interaction-tour-analysis-cancelled",
        "Some commit metadata was unavailable locally or exceeded the reading limit; it was not downloaded" => {
            "interaction-tour-metadata-unavailable"
        }
        "Incomplete commit object header" => "interaction-tour-incomplete-header",
        "Invalid commit header" => "interaction-tour-invalid-header",
        "Missing commit ID" => "interaction-tour-missing-commit-id",
        "Commit object is unavailable" => "interaction-tour-commit-unavailable",
        "Missing object size" => "interaction-tour-missing-size",
        "Invalid object size" => "interaction-tour-invalid-size",
        "Truncated commit object" => "interaction-tour-truncated-object",
        "Missing commit message" => "interaction-tour-missing-message",
        "Invalid commit size" => "interaction-tour-invalid-commit-size",
        "Missing commit record separator" => "interaction-tour-missing-separator",
        "Unexpected commit object output" => "interaction-tour-unexpected-output",
        "Invalid tour history record" => "interaction-tour-invalid-history",
        "Invalid commit date" => "interaction-tour-invalid-date",
        _ => return raw.to_owned(),
    };
    crate::i18n::text(id).to_owned()
}

/// The only dynamic document status produced by the bounded reader. Preserve the path,
/// including newlines, Unicode and anything resembling Fluent syntax, as an opaque argument.
pub(crate) fn document_unavailable(raw: &str) -> String {
    if raw == "Additional documents omitted by the 20-document reading limit" {
        return crate::i18n::message(
            "interaction-tour-documents-omitted",
            &[("limit", 20.into())],
        );
    }
    if let Some(path) = raw
        .strip_suffix(": content unavailable locally or above the reading limit (not downloaded)")
    {
        return crate::i18n::message(
            "interaction-tour-document-unavailable",
            &[("path", path.into())],
        );
    }
    raw.to_owned()
}

/// Called only for Tour::needs, never for arbitrary document or repository text.
pub(crate) fn need(raw: &str) -> &str {
    let id = match raw {
        "Submodules" => "interaction-tour-submodules",
        "Husky hooks (review project instructions)" => "interaction-tour-husky",
        "Lefthook hooks (review project instructions)" => "interaction-tour-lefthook",
        "pre-commit hooks (review project instructions)" => "interaction-tour-pre-commit",
        _ => return raw, // Product names (Git LFS/EditorConfig) and unknown markers.
    };
    crate::i18n::text(id)
}

impl Day {
    /// Keep Display as the original ISO/UTC snapshot representation; localize only paint.
    pub(crate) fn display(self) -> String {
        let raw = self.to_string();
        let iso = raw.strip_suffix(" UTC").unwrap_or(&raw);
        crate::i18n::message(
            "interaction-tour-utc-day",
            &[("date", crate::i18n::date_iso(iso).into())],
        )
    }
}

impl Folder {
    pub(crate) fn display_path(&self) -> String {
        if self.root_files {
            crate::i18n::text("interaction-tour-root-files").to_owned()
        } else {
            String::from_utf8_lossy(&self.path).into_owned()
        }
    }
}
