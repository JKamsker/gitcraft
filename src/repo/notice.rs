//! A retained raw status and its optional source-owned presentation travel together.
//!
//! Plain replacements have no presentation receipt, even when their text happens to be
//! identical. Classifiers, activity linkage and copy actions continue to read the raw tuple.

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
        let retained: Vec<_> = foreground.drain(..).collect();
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
