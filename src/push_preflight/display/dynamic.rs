//! Formatted diagnostics and safe variable substitution.
use super::*;
use crate::i18n::message;

pub(super) fn dynamic(raw: &str) -> Option<String> {
    if let Some(text) = raw
        .strip_prefix("Author email differs from the profile policy for ")
        .and_then(|s| s.strip_suffix("."))
    {
        let rendered = {
            let target = target_label(text);
            Some(message(
                "interaction-guards-push-identity",
                &[("target", target.into())],
            ))
        };
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" Git blob; consider Git LFS."))
    {
        let rendered = {
            let size = text;
            Some(message(
                "interaction-guards-push-large",
                &[("size", size.into())],
            ))
        };
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
        let rendered = {
            let directory = text;
            Some(message("interaction-guards-push-local-uncertain", &[("directory", directory.into())]))
        };
        if rendered.is_some() { return rendered; }
    }
    if let Some(text) = raw.strip_prefix("A previous remote repair has no confirmed terminal outcome. Recheck after the host finishes, or inspect recovery in ").and_then(|s| s.strip_suffix(". No replacement repair was dispatched.")) {
        let rendered = {
            let directory = text;
            Some(message("interaction-guards-push-remote-held", &[("directory", directory.into())]))
        };
        if rendered.is_some() { return rendered; }
    }
    if let Some(text) = raw.strip_prefix("The remote repair outcome is unknown. It may still be running; its owned guard and recovery remain in ").and_then(|s| s.strip_suffix(". Recheck after the host finishes; no replacement or cleanup was dispatched.")) {
        let rendered = {
            let directory = text;
            Some(message("interaction-guards-push-remote-uncertain", &[("directory", directory.into())]))
        };
        if rendered.is_some() { return rendered; }
    }
    if let Some(text) = raw.strip_prefix("Remote repair did not establish complete success. Inspect actual HEAD and concurrent edits, plus recovery in ").and_then(|s| s.strip_suffix("; no rollback overwrote later work.")) {
        let rendered = {
            let directory = text;
            Some(message("interaction-guards-push-remote-incomplete", &[("directory", directory.into())]))
        };
        if rendered.is_some() { return rendered; }
    }
    None
}
