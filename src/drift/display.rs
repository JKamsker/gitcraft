//! Presentation adapters for this family. Raw producers remain authoritative.
//! Resolve only at the UI boundary; unknown diagnostics and supplied payloads stay opaque.
use crate::i18n::message;

/// Display a diagnostic from this family, after any raw matching.
pub fn diagnostic(raw: &str) -> String {
    match raw {
        "There is no main branch commit to compare the worktree with" => {
            return crate::i18n::text("interaction-guards-drift-no-base").into();
        }
        _ => {}
    }
    dynamic(raw).unwrap_or_else(|| raw.to_owned())
}

fn dynamic(raw: &str) -> Option<String> {
    if let Some(text) = raw
        .strip_prefix("Could not measure the worktree's changes: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-drift-measure-error",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" is not a revision"))
    {
        let rendered = (|| {
            let reference = text;
            Some(message(
                "interaction-guards-drift-not-revision",
                &[("reference", reference.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not find the worktree's git folder: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-drift-git-folder",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not read the worktree's task: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-drift-read-task",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not save the worktree's task: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-drift-save-task",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Issues of ")
        .and_then(|s| s.strip_suffix(" aren't read yet"))
    {
        let rendered = (|| {
            let forge = text;
            Some(message(
                "interaction-guards-drift-unsupported-forge",
                &[("forge", forge.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not read issue #")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (issue, text) = text.split_once(": ")?;
            let detail = text;
            Some(message(
                "interaction-guards-drift-issue-error",
                &[("issue", issue.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("No coding agent's command line is installed (")
        .and_then(|s| s.strip_suffix("): the figures above are the exact part"))
    {
        let rendered = (|| {
            let tools = text;
            Some(message(
                "interaction-guards-drift-missing-tools",
                &[("tools", tools.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not run ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (tool, text) = text.split_once(": ")?;
            if !matches!(tool, "claude" | "codex") {
                return None;
            }
            let detail = text;
            Some(message(
                "interaction-guards-drift-run-error",
                &[("tool", tool.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not wait for ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (tool, text) = text.split_once(": ")?;
            if !matches!(tool, "claude" | "codex") {
                return None;
            }
            let detail = text;
            Some(message(
                "interaction-guards-drift-wait-error",
                &[("tool", tool.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw.strip_prefix("").and_then(|s| s.strip_suffix(" s")) {
        let rendered = (|| {
            let (tool, text) = text.split_once(" took longer than ")?;
            if !matches!(tool, "claude" | "codex") {
                return None;
            }
            let seconds = text;
            if seconds.is_empty() || !seconds.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let seconds = seconds.parse::<u64>().ok()?;
            Some(message(
                "interaction-guards-drift-timeout",
                &[("tool", tool.into()), ("seconds", seconds.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw.strip_prefix("").and_then(|s| s.strip_suffix("")) {
        let rendered = (|| {
            let (tool, text) = text.split_once(" failed: ")?;
            if !matches!(tool, "claude" | "codex") {
                return None;
            }
            let detail = text;
            Some(message(
                "interaction-guards-drift-tool-failed",
                &[("tool", tool.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    None
}

impl super::Report {
    pub fn display_headline(&self) -> String {
        let unrelated: Vec<_> = self.unrelated().collect();
        let files: usize = unrelated.iter().map(|f| f.files.len()).sum();
        message(
            "interaction-guards-drift-headline",
            &[
                ("files", self.files.len().into()),
                ("unrelated_files", files.into()),
                ("folders", unrelated.len().into()),
                ("dependencies", self.dependencies.len().into()),
                ("signatures", self.signatures.len().into()),
                ("percent", self.formatting_percent().into()),
                (
                    "unrelated",
                    (if self.judged && !unrelated.is_empty() {
                        "yes"
                    } else {
                        "no"
                    })
                    .into(),
                ),
            ],
        )
    }
}
