//! Presentation adapters for this family. Raw producers remain authoritative.
//! Resolve only at the UI boundary; unknown diagnostics and supplied payloads stay opaque.
use crate::i18n::message;

/// Display a diagnostic from this family, after any raw matching.
pub fn diagnostic(raw: &str) -> String {
    match raw {
        _ => {}
    }
    dynamic(raw).unwrap_or_else(|| raw.to_owned())
}

fn dynamic(raw: &str) -> Option<String> {
    if let Some(text) = raw
        .strip_prefix("Could not read who wrote the commits: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let detail = text;
            Some(message(
                "interaction-guards-authorship-read-all",
                &[("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not read who wrote ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (oid, text) = text.split_once(": ")?;
            let detail = text;
            Some(message(
                "interaction-guards-authorship-read-one",
                &[("oid", oid.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not work out who wrote ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = (|| {
            let (branch, text) = text.split_once(": ")?;
            let detail = text;
            Some(message(
                "interaction-guards-authorship-figures-error",
                &[("branch", branch.into()), ("detail", detail.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" is not a branch"))
    {
        let rendered = (|| {
            let (branch, text) = text.split_once(" or ")?;
            let base = text;
            Some(message(
                "interaction-guards-authorship-not-branches",
                &[("branch", branch.into()), ("base", base.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" is not a commit"))
    {
        let rendered = (|| {
            let oid = text;
            Some(message(
                "interaction-guards-authorship-not-commit",
                &[("oid", oid.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    None
}

impl super::Source {
    pub fn display_label(self) -> &'static str {
        crate::i18n::text(match self {
            Self::GitAi => "interaction-guards-authorship-source-git-ai",
            Self::AgentTrace => "interaction-guards-authorship-source-trace",
            Self::Author => "interaction-guards-authorship-source-author",
            Self::Trailer => "interaction-guards-authorship-source-trailer",
        })
    }
}

impl super::Attribution {
    /// Localized hover; author detection, source selection and line attribution stay raw.
    pub fn display_description(&self) -> String {
        use super::Who;
        let id = match (self.who, self.lines.is_empty(), self.agents.is_empty()) {
            (Who::Agent, true, true) => "interaction-guards-authorship-agent",
            (Who::Agent, true, false) => "interaction-guards-authorship-agent-agents",
            (Who::Agent, false, true) => "interaction-guards-authorship-agent-lines",
            (Who::Agent, false, false) => "interaction-guards-authorship-agent-lines-agents",
            (Who::Mixed, true, true) => "interaction-guards-authorship-mixed",
            (Who::Mixed, true, false) => "interaction-guards-authorship-mixed-agents",
            (Who::Mixed, false, true) => "interaction-guards-authorship-mixed-lines",
            (Who::Mixed, false, false) => "interaction-guards-authorship-mixed-lines-agents",
            (Who::Human, _, true) => "interaction-guards-authorship-human",
            (Who::Human, _, false) => "interaction-guards-authorship-human-agents",
            (Who::Unknown, _, true) => "interaction-guards-authorship-unknown",
            (Who::Unknown, _, false) => "interaction-guards-authorship-unknown-agents",
        };
        message(
            id,
            &[
                ("agents", self.agents.join(", ").into()),
                ("source", self.source.display_label().into()),
            ],
        )
    }
}

impl super::branch::Figures {
    pub fn display_headline(&self, branch: &str) -> String {
        let total = self.total();
        if total == 0 {
            return message(
                "interaction-guards-authorship-no-lines",
                &[("branch", branch.into())],
            );
        }
        // The same percentage arithmetic as the canonical presentation, no attribution changes.
        let percent = |part: u64, of: u64| (part * 100 + of / 2).checked_div(of).unwrap_or(0);
        let yes = |value: bool| if value { "yes" } else { "no" };
        message(
            "interaction-guards-authorship-headline",
            &[
                ("branch", branch.into()),
                ("percent", percent(self.agent, total).into()),
                ("agent", self.agent.into()),
                ("total", total.into()),
                ("commits", self.commits.into()),
                ("edited", percent(self.edited, self.agent).into()),
                ("mixed", percent(self.mixed, total).into()),
                ("unknown", percent(self.unknown, total).into()),
                ("has_agent", yes(self.agent > 0).into()),
                ("has_mixed", yes(self.mixed > 0).into()),
                ("has_unknown", yes(self.unknown > 0).into()),
                ("partial", yes(self.partial).into()),
            ],
        )
    }
}
