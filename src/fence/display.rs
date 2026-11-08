//! Presentation adapters for this family. Raw producers remain authoritative.
//! Resolve only at the UI boundary; unknown diagnostics and supplied payloads stay opaque.
use crate::i18n::message;

/// Display a diagnostic from this family, after any raw matching.
pub fn diagnostic(raw: &str) -> String {
    match raw {
        "there is no main branch to compare with" => {
            return crate::i18n::text("interaction-guards-fence-no-base").into();
        }
        "it shares no history with the main branch" => {
            return crate::i18n::text("interaction-guards-fence-no-history").into();
        }
        "the worktree's folder isn't there" => {
            return crate::i18n::text("interaction-guards-fence-no-folder").into();
        }
        ".claude/settings.local.json isn't a JSON object" => {
            return crate::i18n::text("interaction-guards-fence-not-object").into();
        }
        "\"permissions\" in .claude/settings.local.json isn't an object" => {
            return crate::i18n::text("interaction-guards-fence-permissions-object").into();
        }
        "\"permissions.deny\" in .claude/settings.local.json isn't a list" => {
            return crate::i18n::text("interaction-guards-fence-deny-list").into();
        }
        _ => {}
    }
    dynamic(raw).unwrap_or_else(|| raw.to_owned())
}

fn dynamic(raw: &str) -> Option<String> {
    if let Some(text) = raw.strip_prefix("").and_then(|s| {
        s.strip_suffix(": a fence takes no exceptions (`!`); list what is fenced instead")
    }) {
        let rendered = (|| {
            let pattern = text;
            Some(message(
                "interaction-guards-fence-no-exceptions",
                &[("pattern", pattern.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" has a line break"))
    {
        let rendered = (|| {
            let entry = text;
            Some(message(
                "interaction-guards-fence-line-break",
                &[("entry", entry.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not check ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (target, text) = text.split_once(" against the agent fence: ")?;
            let detail = text;
            Some(message(
                "interaction-guards-fence-check-error",
                &[
                    ("target", target.into()),
                    ("detail", known_detail(detail).into()),
                ],
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
            let (base, text) = text.split_once(" or ")?;
            let tip = text;
            Some(message(
                "interaction-guards-fence-not-revisions",
                &[("base", base.into()), ("tip", tip.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix(".claude/settings.local.json isn't valid JSON: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-fence-invalid-json",
                &[("detail", detail.into())],
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
                "interaction-guards-fence-git-folder",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not read the agent fence: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-fence-read-fence",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not read ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (path, text) = text.split_once(": ")?;
            let detail = text;
            Some(message(
                "interaction-guards-fence-read-error",
                &[
                    (
                        "path",
                        (if path == "the fence's record" {
                            crate::i18n::text("interaction-guards-fence-record")
                        } else {
                            path
                        })
                        .into(),
                    ),
                    ("detail", detail.into()),
                ],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not create the folder of ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (path, text) = text.split_once(": ")?;
            let detail = text;
            Some(message(
                "interaction-guards-fence-folder-error",
                &[("path", path.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not write ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (path, text) = text.split_once(": ")?;
            let detail = text;
            Some(message(
                "interaction-guards-fence-write-error",
                &[("path", path.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not reach the worktree's folder: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-fence-reach-error",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not record the rules written: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-fence-record-error",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not save the agent fence: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-fence-save-fence",
                &[("detail", known_detail(detail).into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not save where the fence is written: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-fence-save-targets",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    None
}

fn known_detail(raw: &str) -> String {
    match raw {
        "there is no main branch to compare with" => {
            return crate::i18n::text("interaction-guards-fence-no-base").into();
        }
        "it shares no history with the main branch" => {
            return crate::i18n::text("interaction-guards-fence-no-history").into();
        }
        _ => {}
    }
    if let Some(pattern) =
        raw.strip_suffix(": a fence takes no exceptions (`!`); list what is fenced instead")
    {
        return message(
            "interaction-guards-fence-no-exceptions",
            &[("pattern", pattern.into())],
        );
    }
    if let Some(entry) = raw.strip_suffix(" has a line break") {
        return message(
            "interaction-guards-fence-line-break",
            &[("entry", entry.into())],
        );
    }
    if let Some((base, tip)) = raw
        .strip_suffix(" is not a revision")
        .and_then(|r| r.split_once(" or "))
    {
        return message(
            "interaction-guards-fence-not-revisions",
            &[("base", base.into()), ("tip", tip.into())],
        );
    }
    raw.to_owned()
}

impl super::check::How {
    pub fn display_label(self) -> &'static str {
        crate::i18n::text(match self {
            Self::Committed => "interaction-guards-fence-how-committed",
            Self::Uncommitted => "interaction-guards-fence-how-uncommitted",
            Self::Untracked => "interaction-guards-fence-how-untracked",
            Self::Ignored => "interaction-guards-fence-how-ignored",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::set_language;

    #[test]
    fn guard_display_fence_keeps_agent_config_and_actual_json_validation_raw() {
        use super::super::{configs, pattern::Pattern};
        let patterns = [Pattern::parse("/src/12345").unwrap()];
        let instructions = configs::agents_md("# User text 12345\n", &patterns);
        let cursor = configs::cursorignore("# User text 12345\n", &patterns);
        let rules = configs::claude_rules(&patterns[0]);
        let json = configs::claude_settings("", &[], &rules).unwrap().unwrap();
        let raw = configs::claude_settings("[]", &[], &rules).unwrap_err();
        assert_eq!(raw, ".claude/settings.local.json isn't a JSON object");
        let invalid = configs::claude_settings("{bad", &[], &rules).unwrap_err();
        let payload = invalid
            .strip_prefix(".claude/settings.local.json isn't valid JSON: ")
            .unwrap();
        for locale in ["en-US", "de"] {
            set_language(locale);
            assert_eq!(
                configs::agents_md("# User text 12345\n", &patterns),
                instructions
            );
            assert_eq!(
                configs::cursorignore("# User text 12345\n", &patterns),
                cursor
            );
            assert_eq!(
                configs::claude_settings("", &[], &rules).unwrap().unwrap(),
                json
            );
            assert_eq!(
                configs::claude_settings("[]", &[], &rules).unwrap_err(),
                raw
            );
            assert!(diagnostic(&invalid).ends_with(payload));
            assert_eq!(
                diagnostic(&raw),
                if locale == "de" {
                    ".claude/settings.local.json ist kein JSON-Objekt."
                } else {
                    raw.as_str()
                }
            );
            assert!(instructions.contains("Paths agents must not touch"));
            assert!(json.contains("Read(/src/12345)"));
        }
        set_language("en-US");
    }
}
