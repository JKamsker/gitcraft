//! A retained raw status and its optional source-owned presentation travel together.
//!
//! Plain replacements have no presentation receipt, even when their text happens to be
//! identical. Classifiers, activity linkage and copy actions continue to read the raw tuple.

pub(super) mod operation;
mod parallel;

use crate::authorship::branch::Figures;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct Notice {
    raw: (String, bool),
    presentation: Option<Arc<Presentation>>,
}

#[derive(Clone, Debug)]
enum Presentation {
    Authorship {
        branch: String,
        figures: Figures,
    },
    Parallel(parallel::Summary),
    Operation(operation::Status),
    OperationFinished(operation::Finished),
    Custom(crate::custom_commands::CapturedToast),
    CustomStarted(String),
    CustomStartFailed {
        name: String,
        error: String,
    },
    Repository {
        name: String,
        source: Arc<Presentation>,
    },
}

impl Notice {
    pub(crate) fn plain(raw: (String, bool)) -> Self {
        Self {
            raw,
            presentation: None,
        }
    }

    pub(crate) fn authorship(branch: String, figures: Figures) -> Self {
        Self {
            raw: (figures.headline(&branch), false),
            presentation: Some(Arc::new(Presentation::Authorship { branch, figures })),
        }
    }

    pub(super) fn parallel(
        outcome: &crate::parallel_bisect::Outcome,
        machine: &crate::parallel_bisect::Machine,
        receipt: Option<crate::parallel_bisect::presentation::Presentation>,
    ) -> Self {
        Self {
            raw: super::parallel_bisect::outcome_message(outcome, &machine.label()),
            presentation: Some(Arc::new(Presentation::Parallel(
                parallel::Summary::capture(outcome, machine, receipt),
            ))),
        }
    }

    pub(super) fn operation(source: operation::Status) -> Self {
        Self {
            raw: source.raw(),
            presentation: Some(Arc::new(Presentation::Operation(source))),
        }
    }

    pub(super) fn operation_finished(
        name: &str,
        label: &str,
        result: &Result<String, String>,
        receipt: Option<crate::gerrit::StackFailure>,
    ) -> Self {
        let source = operation::Finished::capture(name, label, result, receipt);
        Self {
            raw: source.raw(),
            presentation: Some(Arc::new(Presentation::OperationFinished(source))),
        }
    }

    pub(super) fn custom_completed(name: &str, code: i32, stdout: &[u8], stderr: &[u8]) -> Self {
        let source = crate::custom_commands::CapturedToast::capture(name, code, stdout, stderr);
        Self {
            raw: source.raw(),
            presentation: Some(Arc::new(Presentation::Custom(source))),
        }
    }

    pub(super) fn custom_started(name: String) -> Self {
        Self {
            raw: (format!("Running {name}…"), false),
            presentation: Some(Arc::new(Presentation::CustomStarted(name))),
        }
    }

    pub(super) fn custom_start_failed(name: String, error: String) -> Self {
        Self {
            raw: (format!("{name} couldn't be run: {error}"), true),
            presentation: Some(Arc::new(Presentation::CustomStartFailed { name, error })),
        }
    }

    /// This prefix is supplied by the same completion, never recovered from raw text.
    pub(super) fn for_repository(mut self, name: &str) -> Self {
        self.raw.0 = format!("{name} · {}", self.raw.0);
        self.presentation = self.presentation.map(|source| {
            Arc::new(Presentation::Repository {
                name: name.to_owned(),
                source,
            })
        });
        self
    }

    /// No source classification is performed here: only the producer can create a receipt.
    pub(crate) fn projected(&self) -> Option<String> {
        self.presentation.as_ref().map(|source| source.display())
    }

    pub(crate) fn raw(&self) -> &(String, bool) {
        &self.raw
    }

    pub(super) fn into_raw(self) -> (String, bool) {
        self.raw
    }

    /// Any raw edit retires source provenance before the caller can mutate anything.
    pub(super) fn edit_raw(&mut self) -> &mut (String, bool) {
        self.presentation = None;
        &mut self.raw
    }
}

impl Presentation {
    fn display(&self) -> String {
        match self {
            Self::Authorship { branch, figures } => figures.display_headline(branch),
            Self::Parallel(summary) => summary.display(),
            Self::Operation(source) => source.display(),
            Self::OperationFinished(source) => source.display(),
            Self::Custom(source) => source.display().0,
            Self::CustomStarted(name) => crate::i18n::message(
                "repo-workflows-custom_commands-run-custom-captured-5215b5ab",
                &[("name", name.as_str().into())],
            ),
            Self::CustomStartFailed { name, error } => crate::i18n::message(
                "repo-workflows-custom_commands-run-custom-captured-471b45de",
                &[("name", name.as_str().into()), ("e", error.as_str().into())],
            ),
            Self::Repository { name, source } => format!("{name} · {}", source.display()),
        }
    }
}

impl std::ops::Deref for Notice {
    type Target = (String, bool);
    fn deref(&self) -> &Self::Target {
        self.raw()
    }
}

impl PartialEq for Notice {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}
impl Eq for Notice {}

impl PartialEq<(String, bool)> for Notice {
    fn eq(&self, other: &(String, bool)) -> bool {
        &self.raw == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supplied() -> Notice {
        Notice::authorship(
            "topic: $(keep)\nOpaque".to_owned(),
            Figures {
                commits: 1234,
                agent: 2000,
                human: 10,
                edited: 4,
                partial: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn source_notice_moves_through_queues_and_repaints_without_changing_raw() {
        let source = supplied();
        let raw = source.raw().clone();
        let mut parked = vec![source.clone()];
        let mut foreground = vec![Notice::plain(("opaque external".into(), true))];
        foreground.append(&mut parked);
        assert!(parked.is_empty());
        let retained = std::mem::take(&mut foreground);
        assert!(foreground.is_empty());
        assert_eq!(retained[0].projected(), None);
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::German,
            crate::i18n::Language::English,
        ] {
            crate::i18n::set_language(language.tag());
            let rendered = retained[1].projected().unwrap_or_default();
            assert!(rendered.contains("topic: $(keep)\nOpaque"));
            match language {
                crate::i18n::Language::English => {
                    assert!(rendered.contains("1,234 commits"), "{rendered}")
                }
                crate::i18n::Language::German => {
                    assert!(rendered.contains("1.234 Commits"), "{rendered}")
                }
            }
            assert_eq!(retained[1].raw(), &raw);
            assert_eq!(retained[1], source);
        }
        // Equal text from an unrelated source never acquires a presentation receipt.
        let replaced = Notice::plain(raw.clone());
        assert_eq!(replaced, source);
        assert_eq!(replaced.projected(), None);
        assert_eq!(replaced.raw(), &raw);
    }

    #[test]
    fn any_raw_edit_retires_origin_before_noop_or_unwind() {
        let mut notice = supplied();
        let original = notice.raw().clone();
        let _ = notice.edit_raw();
        assert_eq!(notice.projected(), None);
        assert_eq!(notice.raw(), &original);
        let mut notice = supplied();
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = notice.edit_raw();
            std::panic::resume_unwind(Box::new(()));
        }));
        assert!(unwind.is_err());
        assert_eq!(notice.projected(), None);
        assert_eq!(notice.raw(), &original);
        let mut notice = supplied();
        let raw = notice.edit_raw();
        raw.0.push_str(" (Push not run)");
        raw.1 = true;
        assert!(notice.0.ends_with(" (Push not run)"));
        assert!(notice.1);
        assert_eq!(notice.projected(), None);
    }
}

#[cfg(test)]
mod refusal_tests {
    use super::*;

    #[test]
    fn option_refusal_preserves_false_path_and_replaces_origin_on_true() {
        let source = Notice::authorship("main".into(), Figures::default());
        let raw = source.raw().clone();
        let mut status = Some(source);
        assert!(!crate::repo::commands::refuse_option_like(
            "feature/-x",
            &mut status
        ));
        assert_eq!(status.as_deref(), Some(&raw));
        assert!(status.as_ref().and_then(Notice::projected).is_some());
        assert!(crate::repo::commands::refuse_option_like(
            "--orphan=x",
            &mut status
        ));
        assert!(status.as_ref().is_some_and(|n| n.1));
        assert!(status.as_ref().and_then(Notice::projected).is_none());
        assert!(status.as_ref().is_some_and(|n| n.0.contains("--orphan=x")));
    }
}

#[cfg(test)]
mod custom_tests {
    use super::*;
    #[test]
    fn captured_completion_repaints_bounded_opaque_tail_after_queue_transfer() {
        let name = "custom failed\n$(opaque)";
        let output = "discarded\n".repeat(20000) + "\x1b[31mprovider finished\x1b[0m\n";
        for code in [0, 7] {
            let source = Notice::custom_completed(name, code, output.as_bytes(), b"last error\n");
            let raw = source.raw().clone();
            assert_eq!(
                raw,
                crate::custom_commands::toast_text(name, code, output.as_bytes(), b"last error\n")
            );
            assert!(raw.0.ends_with("provider finished\nlast error"));
            assert!(format!("{source:?}").len() < 2000);
            let mut queue = vec![source];
            let retained = queue.remove(0);
            for language in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(language);
                let shown = retained.projected().unwrap_or_default();
                assert!(shown.starts_with(name));
                assert!(shown.ends_with("provider finished\nlast error"));
                if language == "en-US" {
                    assert_eq!(shown, raw.0);
                } else {
                    assert!(shown.contains(if code == 0 {
                        "abgeschlossen"
                    } else {
                        "fehlgeschlagen"
                    }));
                }
                assert_eq!(retained.raw(), &raw);
            }
            assert_eq!(Notice::plain(raw).projected(), None);
        }
    }
    #[test]
    fn custom_launch_sources_keep_original_raw_envelopes_and_opaque_detail() {
        let started = Notice::custom_started("lookalike\nname".into());
        let failed = Notice::custom_start_failed(
            "lookalike\nname".into(),
            "Running external…\nprovider couldn't be run: opaque".into(),
        );
        assert_eq!(started.raw(), &("Running lookalike\nname…".into(), false));
        let raw = failed.raw().clone();
        assert_eq!(raw, ("lookalike\nname couldn't be run: Running external…\nprovider couldn't be run: opaque".into(), true));
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            let shown = failed.projected().unwrap_or_default();
            assert!(shown.ends_with("Running external…\nprovider couldn't be run: opaque"));
            if language == "en-US" {
                assert_eq!(shown, raw.0);
            }
            assert_eq!(failed.raw(), &raw);
        }
    }
}
