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
        .strip_prefix("Could not read the changes to look for weakened checks: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = {
            let detail = text;
            Some(message(
                "interaction-guards-weakened-read-error",
                &[("detail", detail.into())],
            ))
        };
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("Could not list the untracked files to look for weakened checks: ")
        .and_then(|s| s.strip_suffix(""))
    {
        let rendered = {
            let detail = text;
            Some(message(
                "interaction-guards-weakened-list-error",
                &[("detail", detail.into())],
            ))
        };
        if rendered.is_some() {
            return rendered;
        }
    }
    if let Some(text) = raw
        .strip_prefix("These changes are too large to look for weakened checks (")
        .and_then(|s| s.strip_suffix(" MB)"))
    {
        let rendered = (|| {
            let megabytes = text;
            if megabytes.is_empty() || !megabytes.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let megabytes = megabytes.parse::<u64>().ok()?;
            Some(message(
                "interaction-guards-weakened-size-error",
                &[("megabytes", megabytes.into())],
            ))
        })();
        if rendered.is_some() {
            return rendered;
        }
    }
    None
}

impl super::Reason {
    pub fn display_label(self) -> &'static str {
        crate::i18n::text(match self {
            Self::RemovedAssertions => "interaction-guards-weakened-reason-assertions",
            Self::RemovedTests => "interaction-guards-weakened-reason-tests",
            Self::DeletedTestFile => "interaction-guards-weakened-reason-file",
            Self::SkipsTests => "interaction-guards-weakened-reason-skip",
            Self::TrivialAssertion => "interaction-guards-weakened-reason-trivial",
            Self::LoosenedTolerance => "interaction-guards-weakened-reason-tolerance",
            Self::SnapshotRewrite => "interaction-guards-weakened-reason-snapshot",
            Self::CiAllowsFailure => "interaction-guards-weakened-reason-ci-failure",
            Self::CiRemovedJob => "interaction-guards-weakened-reason-ci-job",
            Self::SkipsHooks => "interaction-guards-weakened-reason-hooks",
        })
    }
}

pub fn badge() -> &'static str {
    crate::i18n::text("interaction-guards-weakened-badge")
}

impl super::Flag {
    /// Preserve quoted source, tolerance lexemes and job/trigger keys exactly.
    pub fn display_evidence(&self) -> String {
        use super::Reason;
        if let Some(line) = self.line {
            let side = if self.removed { "old line" } else { "line" };
            let prefix = format!("{side} {line}");
            if let Some(rest) = self.evidence.strip_prefix(&prefix) {
                if let Some(source) = rest.strip_prefix(": ") {
                    let id = if self.removed {
                        "interaction-guards-weakened-old-line"
                    } else {
                        "interaction-guards-weakened-line"
                    };
                    return message(id, &[("line", line.into()), ("source", source.into())]);
                }
                if let Some((detail, source)) =
                    rest.strip_prefix(" (").and_then(|r| r.split_once("): "))
                {
                    let shown = if self.reason == Reason::CiRemovedJob {
                        if let Some(key) = detail
                            .strip_prefix("the `")
                            .and_then(|r| r.strip_suffix("` trigger"))
                        {
                            message(
                                "interaction-guards-weakened-trigger",
                                &[("key", key.into())],
                            )
                        } else if let Some(key) = detail
                            .strip_prefix("the job `")
                            .and_then(|r| r.strip_suffix('`'))
                        {
                            message("interaction-guards-weakened-job", &[("key", key.into())])
                        } else {
                            detail.to_owned()
                        }
                    } else {
                        detail.to_owned()
                    };
                    let id = if self.removed {
                        "interaction-guards-weakened-old-line-detail"
                    } else {
                        "interaction-guards-weakened-line-detail"
                    };
                    return message(
                        id,
                        &[
                            ("line", line.into()),
                            ("detail", shown.into()),
                            ("source", source.into()),
                        ],
                    );
                }
            }
        } else {
            match self.reason {
                Reason::DeletedTestFile => {
                    if let Some(path) = self.evidence.strip_prefix("deletes ") {
                        return message(
                            "interaction-guards-weakened-deletes",
                            &[("path", path.into())],
                        );
                    }
                }
                Reason::SnapshotRewrite => {
                    if let Some(path) = self.evidence.strip_prefix("deletes the snapshot ") {
                        return message(
                            "interaction-guards-weakened-deletes-snapshot",
                            &[("path", path.into())],
                        );
                    }
                    if let Some((removed, old)) = self
                        .evidence
                        .strip_prefix("replaces ")
                        .and_then(|r| r.strip_suffix(" lines"))
                        .and_then(|r| r.split_once(" of its "))
                        .and_then(|(a, b)| Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?)))
                    {
                        return message(
                            "interaction-guards-weakened-replaces",
                            &[("removed", removed.into()), ("old", old.into())],
                        );
                    }
                }
                _ => {}
            }
        }
        self.evidence.clone()
    }
}

impl super::FileFlags {
    pub fn display_summary(&self) -> String {
        summary(self.flags.iter())
    }
}

/// The canonical summary's ordering and five-evidence limit, with presentation resolved now.
pub fn summary<'a>(flags: impl Iterator<Item = &'a super::Flag>) -> String {
    let mut sorted: Vec<_> = flags.collect();
    sorted.sort_by_key(|f| (f.reason, !f.removed, f.line));
    let mut out = Vec::new();
    let mut at = 0;
    while at < sorted.len() {
        let reason = sorted[at].reason;
        let same = sorted[at..]
            .iter()
            .take_while(|f| f.reason == reason)
            .count();
        out.push(message(
            "interaction-guards-weakened-heading",
            &[("reason", reason.display_label().into())],
        ));
        for flag in &sorted[at..at + same.min(super::LISTED)] {
            out.push(format!("  {}", flag.display_evidence()));
        }
        if same > super::LISTED {
            out.push(format!(
                "  {}",
                message(
                    "interaction-guards-weakened-more",
                    &[("count", (same - super::LISTED).into())]
                )
            ));
        }
        at += same;
    }
    out.join("\n")
}
