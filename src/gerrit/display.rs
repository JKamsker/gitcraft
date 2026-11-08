//! Gerrit-owned display adapters. Raw validation, review ops and commit identity stay stable.
use super::{Server, change_id};

/// Known app-owned Gerrit reasons only; unknown tool/provider details remain opaque.
pub(crate) fn error(raw: &str) -> String {
    let id = match raw {
        "missing Change-Id in the message footer" => "interaction-gerrit-missing",
        "Change-Id must fit on one line" => "interaction-gerrit-one-line",
        "Change-Id must be in the final trailer paragraph" => "interaction-gerrit-footer",
        "more than one Change-Id in the footer" => "interaction-gerrit-multiple",
        "Change-Id must be I followed by 40 hexadecimal digits (not all zero)" => {
            "interaction-gerrit-format"
        }
        "Keep the original Change-Id when amending a Gerrit change" => "interaction-gerrit-keep",
        "Missing amend message" => "interaction-gerrit-amend-message",
        "Invalid Gerrit amendment" => "interaction-gerrit-amend-invalid",
        "Choose a remote, a fetched target branch and a valid topic" => "interaction-gerrit-choose",
        "The target branch is not an ancestor of this commit. Fetch and rebase onto the target before pushing for review." => {
            "interaction-gerrit-ancestor"
        }
        "There are no commits above the target branch to review" => "interaction-gerrit-empty",
        "Invalid gitcraft.gerritUrl: use an HTTP(S) URL without credentials, query or fragment. Using the remote's web address; Gerrit review submission remains enabled." => {
            "interaction-gerrit-web"
        }
        _ => return raw.to_owned(),
    };
    crate::i18n::text(id).to_owned()
}

impl Server {
    pub(crate) fn display_web_warning(&self) -> Option<String> {
        self.web_warning.as_deref().map(error)
    }
}

/// A finite owned projection of a failure witnessed by this stack check. Never retains
/// commit messages or the successful log; its private fields cannot be rebound to text.
#[derive(Clone, Debug)]
pub(crate) struct StackFailure {
    raw: String,
    source: StackSource,
}

#[derive(Clone, Debug)]
enum StackSource {
    Fixed,
    Merge(String),
    Hook { commit: String, reason: String },
    Duplicate(String),
}

impl StackFailure {
    pub(crate) fn matching(self, raw: &str) -> Option<Self> {
        (self.raw == raw).then_some(self)
    }

    pub(crate) fn display(&self, raw: &str) -> Option<String> {
        if self.raw != raw {
            return None;
        }
        Some(match &self.source {
            StackSource::Fixed => error(raw),
            StackSource::Merge(commit) => crate::i18n::message(
                "interaction-gerrit-merge",
                &[("commit", commit.as_str().into())],
            ),
            StackSource::Hook { commit, reason } => crate::i18n::message(
                "interaction-gerrit-hook",
                &[
                    ("commit", commit.as_str().into()),
                    ("reason", error(reason).into()),
                ],
            ),
            StackSource::Duplicate(id) => crate::i18n::message(
                "interaction-gerrit-duplicate",
                &[("id", id.as_str().into())],
            ),
        })
    }

    fn from_log(recorded_log: &str) -> Option<Self> {
        let mut ids = std::collections::HashSet::new();
        for fields in recorded_log
            .split('\0')
            .collect::<Vec<_>>()
            .as_chunks::<3>()
            .0
        {
            let hash = fields[0].trim();
            let short = hash.get(..hash.len().min(8))?;
            let (expected, source) = if fields[1].split_whitespace().count() > 1 {
                (
                    format!(
                        "Merge commit {short} cannot be pushed with this review flow; rebase the series first"
                    ),
                    StackSource::Merge(short.to_owned()),
                )
            } else {
                match change_id(fields[2]) {
                    Err(why) => (
                        format!(
                            "{short}: {why}. Install Gerrit's commit-msg hook from your server, then amend this commit to add its Change-Id."
                        ),
                        StackSource::Hook {
                            commit: short.to_owned(),
                            reason: why,
                        },
                    ),
                    Ok(id) if !ids.insert(id) => (
                        format!(
                            "Two commits have the same Change-Id ({id}); each change needs its own"
                        ),
                        StackSource::Duplicate(id.to_owned()),
                    ),
                    Ok(_) => continue,
                }
            };
            return Some(Self {
                raw: expected,
                source,
            });
        }
        None
    }
}

/// Runs the unchanged validator exactly once. Only a successful log from that very
/// invocation supplies dynamic context; a provider failure cannot impersonate it.
pub(crate) fn check_stack_presented_with(
    base: &str,
    head: &str,
    mut run: impl FnMut(&[&str]) -> Result<String, String>,
) -> (Result<(), String>, Option<StackFailure>) {
    let mut log_succeeded = false;
    let mut witness = None;
    let mut ancestor_failed = false;
    let result = super::check_stack_with(base, head, |args| {
        let result = run(args);
        if args[0] == "log" {
            if let Ok(out) = &result {
                log_succeeded = true;
                witness = StackFailure::from_log(out);
            }
        } else {
            ancestor_failed = result.as_ref().is_err_and(|e| e != crate::git::CANCELLED);
        }
        result
    });
    let source = result.as_ref().err().and_then(|raw| {
        if ancestor_failed
            || (log_succeeded && raw == "There are no commits above the target branch to review")
        {
            Some(StackFailure {
                raw: raw.clone(),
                source: StackSource::Fixed,
            })
        } else {
            witness.and_then(|source| source.matching(raw))
        }
    });
    (result, source)
}

/// Legacy display API: context must be the successful log from the same invocation.
pub(crate) fn stack_error(raw: &str, recorded_log: &str) -> String {
    StackFailure::from_log(recorded_log)
        .and_then(|source| source.display(raw))
        .unwrap_or_else(|| error(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "I0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn actual_validation_and_server_warning_render_without_mutating_identity_or_ops()
    -> Result<(), String> {
        let messages = [
            "Subject",
            "Subject\n\nChange-Id: Ibad",
            "Subject\n\nChange-Id: I0000000000000000000000000000000000000000",
            "Subject\n\nChange-Id: Ibad\n continued",
            "Subject\n\nChange-Id: Ibad\nChange-Id: Ibad",
            "Subject\n\nChange-Id: Ibad\nnot a trailer",
        ];
        for message in messages {
            let raw = change_id(message)
                .err()
                .ok_or("invalid fixture must fail")?;
            crate::i18n::set_language("de");
            assert_ne!(error(&raw), raw);
            assert_eq!(change_id(message).err().as_deref(), Some(raw.as_str()));
            crate::i18n::set_language("en-US");
            assert_eq!(error(&raw), raw);
        }
        let remotes = vec![crate::git::Remote {
            name: "origin".into(),
            url: "ssh://review.example/project".into(),
        }];
        let server = super::super::detect_with(
            &remotes,
            &std::collections::HashMap::from([
                ("gitcraft.forge".into(), "gerrit".into()),
                ("gitcraft.gerriturl".into(), "javascript:invalid".into()),
            ]),
        )
        .ok_or("server")?;
        let original = server.clone();
        let hash = "a".repeat(40);
        let ops =
            super::super::review_ops("origin", "main", "topic,submit%ready=yes", &hash, &hash)?;
        crate::i18n::set_language("de");
        assert!(
            server
                .display_web_warning()
                .is_some_and(|s| s.starts_with("Ungültige"))
        );
        assert_eq!(server, original);
        assert_eq!(
            super::super::review_ops("origin", "main", "topic,submit%ready=yes", &hash, &hash)?,
            ops
        );
        assert_eq!(ops[1][1], "--push-option=topic=topic,submit%ready=yes");
        assert_eq!(change_id(&format!("Subject\n\nChange-Id: {ID}")), Ok(ID));
        Ok(())
    }

    #[test]
    fn actual_recording_stack_validation_uses_same_log_context_and_keeps_unknowns_opaque()
    -> Result<(), String> {
        let hash = "a".repeat(40);
        let other = "b".repeat(40);
        let valid = format!("Subject\n\nChange-Id: {ID}");
        let cases = [
            (
                format!("{hash}\0parent\0Missing footer\0"),
                "commit-msg-Hook",
            ),
            (format!("{hash}\0first second\0{valid}\0"), "Merge-Commit"),
            (
                format!("{hash}\0parent\0{valid}\0{other}\0parent\0{valid}\0"),
                "dieselbe Change-Id",
            ),
        ];
        for (log, expected) in cases {
            let mut commands = Vec::new();
            let raw = super::super::check_stack_with("base", "head", |args| {
                commands.push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
                Ok(if args[0] == "log" {
                    log.clone()
                } else {
                    String::new()
                })
            })
            .err()
            .ok_or("invalid stack fixture")?;
            assert_eq!(commands.len(), 2);
            assert_eq!(commands[0], ["merge-base", "--is-ancestor", "base", "head"]);
            assert_eq!(commands[1][4], "--format=%H%x00%P%x00%B%x00");
            crate::i18n::set_language("de");
            assert!(stack_error(&raw, &log).contains(expected));
            assert_eq!(stack_error(&raw, ""), raw);
            crate::i18n::set_language("en-US");
            assert_eq!(stack_error(&raw, &log), raw);
        }
        crate::i18n::set_language("de");
        for raw in [
            crate::git::CANCELLED,
            "provider: missing Change-Id in the message footer",
            "deadbeef: external problem",
            "Two commits have the same Change-Id (opaque); each change needs its own",
        ] {
            assert_eq!(stack_error(raw, ""), raw);
        }
        Ok(())
    }
}
