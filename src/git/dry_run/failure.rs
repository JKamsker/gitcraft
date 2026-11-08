//! Locale-independent failures retained by a cached preview. Only explicit producers create
//! owned variants; tool/provider output is opaque even when it resembles an owned message.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    Opaque(String),
    PartialClone,
    NoTree {
        oid: String,
    },
    ReplayFallback {
        first: Box<Self>,
        scratch: Box<Self>,
    },
    ReplayNeedsBranch,
    NotPlainRange,
    NothingReplayed,
    ReplayMismatch,
    MergeTree {
        detail: String,
    },
    NotCommit {
        commit: String,
    },
    FirstCommit {
        commit: String,
    },
    CommitTree {
        made: String,
    },
    InvalidCommitId {
        bad: String,
    },
    InvalidReachability,
    InvalidCommitPair {
        old: String,
        new: String,
    },
    ScratchClone {
        detail: String,
    },
    UnknownVersion {
        printed: String,
    },
}

impl From<String> for Failure {
    fn from(raw: String) -> Self {
        Self::Opaque(raw)
    }
}

impl Failure {
    /// Deterministic original English/control witness, independent of the current locale.
    pub(crate) fn raw(&self) -> String {
        match self {
            Self::Opaque(raw) => raw.clone(),
            Self::PartialClone => {
                "this is a partial clone: working it out would download files from the remote"
                    .into()
            }
            Self::NoTree { oid } => format!("{oid} has no tree"),
            Self::ReplayFallback { first, scratch } => {
                format!("{}; in a scratch clone: {}", first.raw(), scratch.raw())
            }
            Self::ReplayNeedsBranch => "git replay needs a branch".into(),
            Self::NotPlainRange => "the commits aren't a plain range".into(),
            Self::NothingReplayed => "nothing was replayed".into(),
            Self::ReplayMismatch => "git replay's commits don't match the originals".into(),
            Self::MergeTree { detail } => format!("git merge-tree said: {detail}"),
            Self::NotCommit { commit } => format!("{commit} is not a commit"),
            Self::FirstCommit { commit } => {
                format!("{commit} is a first commit, with nothing to replay from")
            }
            Self::CommitTree { made } => format!("git commit-tree said: {made}"),
            Self::InvalidCommitId { bad } => format!("{bad} is not a commit id"),
            Self::InvalidReachability => "Invalid local reachability output".into(),
            Self::InvalidCommitPair { old, new } => format!("{old} or {new} is not a commit id"),
            Self::ScratchClone { detail } => format!("could not make a scratch clone: {detail}"),
            Self::UnknownVersion { printed } => format!("unknown git version: {printed}"),
        }
    }

    /// Resolve only at a presentation boundary; never store this result in the worker cache.
    pub(crate) fn display(&self) -> String {
        use crate::i18n::{message, text};
        match self {
            Self::Opaque(raw) => raw.clone(),
            Self::PartialClone => text(
                "backend-this-is-a-partial-clone-working-it-out-would-download-files-from-7e1a13",
            )
            .into(),
            Self::NoTree { oid } => message(
                "backend-oid-has-no-tree-0ea71d",
                &[("oid", oid.as_str().into())],
            ),
            Self::ReplayFallback { first, scratch } => message(
                "backend-first-in-a-scratch-clone-e-5541cd",
                &[
                    ("first", first.display().into()),
                    ("detail", scratch.display().into()),
                ],
            ),
            Self::ReplayNeedsBranch => text("backend-git-replay-needs-a-branch-f898a0").into(),
            Self::NotPlainRange => text("backend-the-commits-aren-t-a-plain-range-b0cf7b").into(),
            Self::NothingReplayed => text("backend-nothing-was-replayed-0e02b4").into(),
            Self::ReplayMismatch => {
                text("backend-git-replay-s-commits-don-t-match-the-originals-b428f7").into()
            }
            Self::MergeTree { detail } => message(
                "backend-git-merge-tree-said-9223d7",
                &[("detail", detail.as_str().into())],
            ),
            Self::NotCommit { commit } => message(
                "backend-commit-is-not-a-commit-9e1c80",
                &[("commit", commit.as_str().into())],
            ),
            Self::FirstCommit { commit } => message(
                "backend-is-a-first-commit-with-nothing-to-replay-from-b32065",
                &[("commit", commit.as_str().into())],
            ),
            Self::CommitTree { made } => message(
                "backend-git-commit-tree-said-made-c61f36",
                &[("made", made.as_str().into())],
            ),
            Self::InvalidCommitId { bad } => message(
                "backend-bad-is-not-a-commit-id-e58577",
                &[("bad", bad.as_str().into())],
            ),
            Self::InvalidReachability => {
                text("backend-invalid-local-reachability-output-108ac6").into()
            }
            Self::InvalidCommitPair { old, new } => message(
                "backend-old-or-new-is-not-a-commit-id-acc42a",
                &[("old", old.as_str().into()), ("new", new.as_str().into())],
            ),
            Self::ScratchClone { detail } => message(
                "backend-could-not-make-a-scratch-clone-e-94509c",
                &[("detail", detail.as_str().into())],
            ),
            Self::UnknownVersion { printed } => message(
                "backend-unknown-git-version-757618",
                &[("printed", printed.as_str().into())],
            ),
        }
    }

    /// The existing raw sentinel still controls cancellation, including through String APIs.
    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Opaque(raw) if raw == super::CANCELLED)
    }
}

#[cfg(test)]
mod tests {
    use super::Failure;
    use std::sync::Arc;

    #[test]
    fn backend_ghost_cached_owned_failures_repaint_en_de_en_without_losing_witness()
    -> Result<(), String> {
        let raw_value = "raw/ß-{ $detail }/git replay needs a branch";
        let cases = [
            (Failure::PartialClone,
             "this is a partial clone: working it out would download files from the remote".to_owned(),
             "Dies ist ein partieller Klon: Die Berechnung würde Dateien vom Remote herunterladen.".to_owned()),
            (Failure::NoTree { oid: raw_value.into() },
             format!("{raw_value} has no tree"), format!("{raw_value} hat keinen Baum.")),
            (Failure::ReplayFallback { first: Box::new(Failure::ReplayNeedsBranch), scratch: Box::new(Failure::Opaque(raw_value.into())) },
             format!("git replay needs a branch; in a scratch clone: {raw_value}"),
             format!("git replay benötigt einen Branch.; in einem temporären Klon: {raw_value}")),
            (Failure::ReplayNeedsBranch, "git replay needs a branch".into(), "git replay benötigt einen Branch.".into()),
            (Failure::NotPlainRange, "the commits aren't a plain range".into(), "Die Commits bilden keinen einfachen Bereich.".into()),
            (Failure::NothingReplayed, "nothing was replayed".into(), "Nichts wurde erneut angewendet.".into()),
            (Failure::ReplayMismatch, "git replay's commits don't match the originals".into(), "Die Commits von git replay passen nicht zu den Originalen.".into()),
            (Failure::MergeTree { detail: raw_value.into() }, format!("git merge-tree said: {raw_value}"), format!("git merge-tree meldet: {raw_value}")),
            (Failure::NotCommit { commit: raw_value.into() }, format!("{raw_value} is not a commit"), format!("{raw_value} ist kein Commit.")),
            (Failure::FirstCommit { commit: raw_value.into() }, format!("{raw_value} is a first commit, with nothing to replay from"), format!("{raw_value} ist ein erster Commit ohne Ausgangspunkt zum erneuten Anwenden.")),
            (Failure::CommitTree { made: raw_value.into() }, format!("git commit-tree said: {raw_value}"), format!("git commit-tree meldet: {raw_value}")),
            (Failure::InvalidCommitId { bad: raw_value.into() }, format!("{raw_value} is not a commit id"), format!("{raw_value} ist keine Commit-ID.")),
            (Failure::InvalidReachability, "Invalid local reachability output".into(), "Ungültige lokale Erreichbarkeitsausgabe".into()),
            (Failure::InvalidCommitPair { old: raw_value.into(), new: "--opaque-new".into() }, format!("{raw_value} or --opaque-new is not a commit id"), format!("{raw_value} oder --opaque-new ist keine Commit-ID.")),
            (Failure::ScratchClone { detail: raw_value.into() }, format!("could not make a scratch clone: {raw_value}"), format!("Ein temporärer Klon konnte nicht erstellt werden: {raw_value}")),
            (Failure::UnknownVersion { printed: raw_value.into() }, format!("unknown git version: {raw_value}"), format!("Unbekannte Git-Version: {raw_value}")),
        ];
        for (failure, english, german) in cases {
            // This exact Arc is retained through all three paints; no worker or job runs.
            let cached: Arc<Result<(), Failure>> = Arc::new(Err(failure.clone()));
            let retained = Arc::clone(&cached);
            for language in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(language);
                let current = cached
                    .as_ref()
                    .as_ref()
                    .err()
                    .ok_or("supplied failure became a success")?;
                assert_eq!(current.raw(), english);
                assert_eq!(
                    current.display(),
                    if language == "de" {
                        german.as_str()
                    } else {
                        english.as_str()
                    }
                );
                assert_eq!(current, &failure);
                assert!(!current.is_cancelled());
                assert!(Arc::ptr_eq(&cached, &retained));
            }
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }

    #[test]
    fn backend_ghost_opaque_errors_are_never_inferred_from_owned_looking_text() {
        let opaque = [
            "git replay needs a branch",
            "git replay benötigt einen Branch.",
            "cancelled: provider detail",
            "backend-git-replay-needs-a-branch-f898a0",
            "raw/provider\n{ $detail }\0/de/repo --arg refs/heads/ß",
        ];
        for raw in opaque {
            let failure = Failure::from(raw.to_owned());
            assert!(matches!(&failure, Failure::Opaque(value) if value == raw));
            for language in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(language);
                assert_eq!(failure.raw(), raw);
                assert_eq!(failure.display(), raw);
                assert!(!failure.is_cancelled());
                let wrapped = Failure::ScratchClone { detail: raw.into() };
                assert!(wrapped.display().ends_with(raw));
                assert!(!wrapped.is_cancelled());
            }
        }
        let cancelled = Failure::from(super::super::CANCELLED.to_owned());
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            assert!(cancelled.is_cancelled());
            assert_eq!(cancelled.raw(), "cancelled");
            assert_eq!(cancelled.display(), "cancelled");
        }
        crate::i18n::set_language("en-US");
    }

    #[test]
    fn backend_ghost_real_parser_and_early_guards_preserve_payloads_without_io() {
        use super::super::{leaving, leaving_with_witness, picks, unreachable_with_witness};
        let input = "  tool: git replay needs a branch\n/de/raw/{ $detail }  ";
        let failure = picks::parse_merge_tree(input).err();
        assert_eq!(
            failure,
            Some(Failure::MergeTree {
                detail: input.trim().into()
            })
        );
        let oid = "a".repeat(40);
        let payload = format!("{oid}\0raw/ß.rs\0--option-looking\0raw/ß.rs\0\0opaque status");
        assert_eq!(
            picks::parse_merge_tree(&payload),
            Ok((
                oid.clone(),
                vec!["raw/ß.rs".into(), "--option-looking".into()]
            ))
        );
        // Invalid names are rejected before the first run/constructor/discovery call.
        let unused_path = std::path::Path::new("supplied-unused-path");
        let bad = "--raw-ref/ß";
        let pair = leaving_with_witness(unused_path, bad, &oid, 42).err();
        assert_eq!(
            pair,
            Some(Failure::InvalidCommitPair {
                old: bad.into(),
                new: oid.clone()
            })
        );
        let unreachable = unreachable_with_witness(unused_path, &[bad.into()], &[], &[], 42).err();
        assert_eq!(
            unreachable,
            Some(Failure::InvalidCommitId { bad: bad.into() })
        );
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            assert_eq!(
                leaving(unused_path, bad, &oid, 42).err(),
                pair.as_ref().map(Failure::display)
            );
            assert_eq!(picks::parse_merge_tree(input).err(), failure);
        }
        crate::i18n::set_language("en-US");
    }

    #[test]
    fn backend_ghost_nested_replay_witness_keeps_cancellation_and_opaque_detail() {
        use super::super::picks::replay_failure;
        let first = Failure::NoTree {
            oid: "raw/ref/ß".into(),
        };
        let opaque = "cancelled? git replay needs a branch /de/provider";
        let nested = replay_failure(
            Some(first.clone()),
            Failure::ScratchClone {
                detail: opaque.into(),
            },
        );
        let before = nested.clone();
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            assert_eq!(
                nested.raw(),
                format!(
                    "raw/ref/ß has no tree; in a scratch clone: could not make a scratch clone: {opaque}"
                )
            );
            assert_eq!(
                nested.display(),
                if language == "de" {
                    format!(
                        "raw/ref/ß hat keinen Baum.; in einem temporären Klon: Ein temporärer Klon konnte nicht erstellt werden: {opaque}"
                    )
                } else {
                    nested.raw()
                }
            );
            assert_eq!(nested, before);
            assert_eq!(
                replay_failure(Some(first.clone()), Failure::Opaque("cancelled".into())),
                Failure::Opaque("cancelled".into())
            );
            assert_eq!(
                replay_failure(None, Failure::Opaque(opaque.into())),
                Failure::Opaque(opaque.into())
            );
        }
        crate::i18n::set_language("en-US");
    }
}
