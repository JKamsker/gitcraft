//! Presentation adapters for this family. Raw producers remain authoritative.
//! Resolve only at the UI boundary; unknown diagnostics and supplied payloads stay opaque.
use crate::i18n::message;

/// Display a diagnostic from this family, after any raw matching.
pub fn diagnostic(raw: &str) -> String {
    match raw {
        "Stopped" => return crate::i18n::text("interaction-guards-collision-stopped").into(),
        "Not merged" => return crate::i18n::text("interaction-guards-collision-not-merged").into(),
        "The collision matrix needs git 2.38 or later (git merge-tree --write-tree)" => {
            return crate::i18n::text("interaction-guards-collision-needs-git").into();
        }
        _ => {}
    }
    dynamic(raw).unwrap_or_else(|| raw.to_owned())
}

fn dynamic(raw: &str) -> Option<String> {
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" is not a branch"))
    {
        let rendered = (|| {
            let reference = text;
            Some(message(
                "interaction-guards-collision-not-branch",
                &[("reference", reference.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not merge ")
        .and_then(|s| s.strip_suffix(" in memory: one of them is not a branch or commit"))
    {
        let rendered = (|| {
            let (a, text) = text.split_once(" with ")?;
            let b = text;
            Some(message(
                "interaction-guards-collision-invalid-pair",
                &[("a", a.into()), ("b", b.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" share no history"))
    {
        let rendered = (|| {
            let (a, text) = text.split_once(" and ")?;
            let b = text;
            Some(message(
                "interaction-guards-collision-no-history",
                &[("a", a.into()), ("b", b.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not merge ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (a, text) = text.split_once(" with ")?;
            let (b, text) = text.split_once(" in memory: ")?;
            let detail = text;
            Some(message(
                "interaction-guards-collision-merge-error",
                &[("a", a.into()), ("b", b.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not merge ")
        .and_then(|s| s.strip_suffix(" in memory"))
    {
        let rendered = (|| {
            let (a, text) = text.split_once(" with ")?;
            let b = text;
            Some(message(
                "interaction-guards-collision-merge-empty",
                &[("a", a.into()), ("b", b.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    None
}
