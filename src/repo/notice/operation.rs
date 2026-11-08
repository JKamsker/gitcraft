//! Operation-owned outer envelopes. Raw identities are captured before display_operation.
//! Successful Git logs never enter these receipts; Gerrit retains only its finite witness.
use crate::gerrit::StackFailure;

#[derive(Clone, Debug)]
pub(in crate::repo) struct Failure {
    raw: String,
    gerrit: Option<StackFailure>,
}

impl Failure {
    pub(in crate::repo) fn capture(raw: &str, gerrit: Option<StackFailure>) -> Self {
        Self {
            raw: raw.to_owned(),
            gerrit: gerrit.and_then(|source| source.matching(raw)),
        }
    }

    pub(in crate::repo) fn matching(self, raw: &str) -> Option<Self> {
        (self.raw == raw).then_some(self)
    }

    pub(in crate::repo) fn display(&self, raw: &str) -> Option<String> {
        (self.raw == raw).then(|| self.projected())
    }

    fn projected(&self) -> String {
        self.gerrit
            .as_ref()
            .and_then(|source| source.display(&self.raw))
            .unwrap_or_else(|| canonical_error(&self.raw))
    }
}

fn canonical_error(raw: &str) -> String {
    crate::git::display_error(&crate::push_preflight::display::diagnostic(raw))
}

#[derive(Clone, Debug)]
pub(in crate::repo) enum Status {
    Done {
        label: String,
        note: Option<String>,
    },
    Cancelled {
        label: String,
        error: Failure,
        left_locks: bool,
    },
    SignIn {
        label: String,
    },
    MissingSignInProof,
    Failed {
        label: String,
        error: Failure,
    },
}

impl Status {
    pub(super) fn raw(&self) -> (String, bool) {
        match self {
            Self::Done { label, note: None } => (format!("{label}: done"), false),
            Self::Done { label, note: Some(note) } => (format!("{label}: done, but {note}"), true),
            Self::Cancelled { label, error, left_locks } => (format!("{label}: {}", error.raw), *left_locks),
            Self::SignIn { label } => (format!("{label}: sign-in needed"), true),
            Self::MissingSignInProof => ("The reviewed push's sign-in proof is unavailable. Reopen push preflight before signing in.".into(), true),
            Self::Failed { label, error } => (format!("{label} failed: {}", error.raw), true),
        }
    }

    pub(super) fn display(&self) -> String {
        use crate::i18n::{message, text};
        use crate::repo::messages::display_operation;
        match self {
            Self::Done { label, note: None } => message(
                "repo-core-0269",
                &[("label", display_operation(label).into())],
            ),
            Self::Done {
                label,
                note: Some(note),
            } => message(
                "repo-core-2959",
                &[
                    ("label", display_operation(label).into()),
                    ("note", note.as_str().into()),
                ],
            ),
            Self::Cancelled { label, error, .. } => {
                format!("{}: {}", display_operation(label), error.projected())
            }
            Self::SignIn { label } => message(
                "repo-core-2962",
                &[("label", display_operation(label).into())],
            ),
            Self::MissingSignInProof => text("repo-core-2963").into(),
            Self::Failed { label, error } => message(
                "repo-core-0270",
                &[
                    ("label", display_operation(label).into()),
                    ("e", error.projected().into()),
                ],
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Finished {
    name: String,
    label: String,
    error: Option<Headline>,
}

impl Finished {
    pub(super) fn capture(
        name: &str,
        label: &str,
        result: &Result<String, String>,
        source: Option<StackFailure>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            label: label.to_owned(),
            error: result
                .as_ref()
                .err()
                .map(|error| Headline::capture(error, source)),
        }
    }

    pub(super) fn raw(&self) -> (String, bool) {
        match &self.error {
            None => (format!("{} · {}: done", self.name, self.label), false),
            Some(error) => (
                format!("{} · {} failed: {}", self.name, self.label, error.raw),
                true,
            ),
        }
    }

    pub(super) fn display(&self) -> String {
        let label = crate::repo::messages::display_operation(&self.label);
        match &self.error {
            None => crate::i18n::message(
                "repo-core-2965",
                &[("name", self.name.as_str().into()), ("label", label.into())],
            ),
            Some(error) => crate::i18n::message(
                "repo-core-2966",
                &[
                    ("name", self.name.as_str().into()),
                    ("label", label.into()),
                    ("e", error.display().into()),
                ],
            ),
        }
    }
}

/// Only the selected line or conflict counters survive a long completion. This is the
/// original 771 headline selection, with the finite conflict fields retained for paint.
#[derive(Clone, Debug)]
struct Headline {
    raw: String,
    line: Option<Failure>,
    conflicts: Option<(String, usize, usize)>,
}

impl Headline {
    fn capture(error: &str, source: Option<StackFailure>) -> Self {
        let raw = raw_headline(error);
        let (label, body) = error.split_once(": ").unwrap_or(("", error));
        let count = body.lines().filter(|l| l.starts_with("CONFLICT")).count();
        let conflicts = (count > 0).then(|| {
            (
                label.to_owned(),
                count,
                crate::repo::conflict::rerere::reused_count(body),
            )
        });
        // A full-error witness is never attached to a changed/truncated child line.
        let line = conflicts.is_none().then(|| Failure::capture(&raw, source));
        Self {
            raw,
            line,
            conflicts,
        }
    }

    fn display(&self) -> String {
        if let Some((label, count, reused)) = &self.conflicts {
            let label = crate::repo::messages::display_operation(label);
            let id = if *reused == 0 {
                "repo-core-3702"
            } else if reused >= count {
                "repo-core-3703"
            } else {
                "repo-core-3704"
            };
            return crate::i18n::message(
                id,
                &[
                    ("label", label.into()),
                    ("conflicts", (*count).into()),
                    ("n", (*reused).into()),
                ],
            );
        }
        self.line
            .as_ref()
            .map_or_else(|| self.raw.clone(), Failure::projected)
    }
}

fn raw_headline(msg: &str) -> String {
    let (label, body) = msg.split_once(": ").unwrap_or(("", msg));
    let conflicts = body.lines().filter(|l| l.starts_with("CONFLICT")).count();
    if conflicts > 0 {
        let files = if conflicts == 1 { "file" } else { "files" };
        let them = |n: usize| if n == 1 { "it" } else { "them" };
        return match crate::repo::conflict::rerere::reused_count(body) {
            0 => format!("{label}: conflicts in {conflicts} {files}, resolve them in Changes"),
            n if n >= conflicts => format!(
                "{label}: {n} {} resolved as before, check {} in Changes",
                if n == 1 { "file" } else { "files" },
                them(n)
            ),
            n => format!(
                "{label}: conflicts in {conflicts} {files}, {n} resolved as before: check {}, \
                 resolve the rest in Changes",
                them(n)
            ),
        };
    }
    // A hook named as what stopped it (see `git::FailedHook`) says more than git's own
    // `error: failed to push some refs` after it.
    let hook = body.lines().next().filter(|l| {
        (l.starts_with("the ") || l.starts_with("it was done, but the ")) && l.contains(" hook ")
    });
    let line = hook
        .or_else(|| {
            body.lines()
                .find(|l| l.starts_with("error:") || l.starts_with("fatal:"))
        })
        .or_else(|| body.lines().find(|l| !l.trim().is_empty()))
        .unwrap_or_default();
    if label.is_empty() {
        line.to_owned()
    } else {
        format!("{label}: {line}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Notice;

    #[test]
    fn retained_operation_envelopes_keep_771_raw_fields_and_repaint_at_current_locale() {
        crate::i18n::set_language("de");
        let label = "Commit";
        let note = "provider: done, but opaque $(keep)\nCommit failed: untouched";
        let cases = [
            (Status::Done { label: label.into(), note: None }, ("Commit: done".to_owned(), false)),
            (Status::Done { label: label.into(), note: Some(note.into()) }, (format!("Commit: done, but {note}"), true)),
            (Status::Cancelled { label: label.into(), error: Failure::capture(crate::git::CANCELLED, None), left_locks: false }, ("Commit: cancelled".into(), false)),
            (Status::Cancelled { label: label.into(), error: Failure::capture("cancelled, but opaque locks", None), left_locks: true }, ("Commit: cancelled, but opaque locks".into(), true)),
            (Status::SignIn { label: label.into() }, ("Commit: sign-in needed".into(), true)),
            (Status::MissingSignInProof, ("The reviewed push's sign-in proof is unavailable. Reopen push preflight before signing in.".into(), true)),
            (Status::Failed { label: label.into(), error: Failure::capture(note, None) }, (format!("Commit failed: {note}"), true)),
        ];
        for (source, raw) in cases {
            let status = Notice::operation(source);
            assert_eq!(status.raw(), &raw);
            let mut parked = vec![status.clone()];
            let mut queue = Vec::new();
            queue.append(&mut parked);
            let retained = queue.remove(0);
            let mut english = None;
            for locale in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(locale);
                let shown = retained.projected().unwrap();
                if locale == "en-US" {
                    assert_eq!(shown, raw.0);
                    if let Some(previous) = &english {
                        assert_eq!(&shown, previous);
                    }
                    english = Some(shown);
                } else {
                    assert_ne!(shown, raw.0);
                    if raw.0.contains(note) {
                        assert!(shown.contains(note));
                    }
                }
                assert_eq!(retained.raw(), &raw);
                assert_eq!(retained, status);
            }
            assert_eq!(Notice::plain(raw).projected(), None);
        }
    }

    #[test]
    fn retained_finished_uses_original_headline_and_discards_unselected_output() {
        let name = "repo · $(opaque)\nCommit failed: raw";
        let conflicts =
            "Merge: start\nCONFLICT (content): path with Commit\nCONFLICT (content): $(opaque)\n";
        let huge = format!(
            "Command: progress\n{}\nfatal: provider Commit failed: untouched",
            "unselected\n".repeat(200000)
        );
        let cases = [
            (Ok("ignored output".into()), "Commit: done".to_owned()),
            (
                Err(conflicts.into()),
                "Commit failed: Merge: conflicts in 2 files, resolve them in Changes".into(),
            ),
            (
                Err(huge),
                "Commit failed: Command: fatal: provider Commit failed: untouched".into(),
            ),
        ];
        for (result, expected) in cases {
            crate::i18n::set_language("de");
            let retained = Notice::operation_finished(name, "Commit", &result, None);
            let raw = retained.raw().clone();
            assert_eq!(raw, (format!("{name} · {expected}"), result.is_err()));
            assert!(format!("{retained:?}").len() < 2000);
            for locale in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(locale);
                let shown = retained.projected().unwrap();
                assert!(shown.starts_with(&format!("{name} · ")));
                if locale == "en-US" {
                    assert_eq!(shown, raw.0);
                } else {
                    assert_ne!(shown, raw.0);
                }
                if expected.contains("provider") {
                    assert!(shown.ends_with("fatal: provider Commit failed: untouched"));
                }
                assert_eq!(retained.raw(), &raw);
            }
        }
    }
}
