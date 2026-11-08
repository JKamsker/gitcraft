//! Approval previews use the same dry-run planner as menu ghost runs, in the planning
//! worker. They cannot execute the requested write or approve it.
use crate::repo::ghost::{self, GhostOp, PreviewFailure};
use crate::{
    git,
    mcp::wire::{ResetMode, WriteOp},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Preview {
    pub moves: Vec<(String, String, String)>,
    pub leaving: Vec<String>,
    pub notes: Vec<String>,
    note_receipts: Vec<(usize, Note)>,
}

pub(super) fn read(
    repo: &std::path::Path,
    op: &WriteOp,
    cancelled: &dyn Fn() -> bool,
) -> Option<Preview> {
    let result = (|| {
        let refs = git::refs(repo)?;
        let op = match op {
            WriteOp::Reset {
                commit,
                mode: ResetMode::Hard,
            } => GhostOp::ResetHard { to: commit.clone() },
            WriteOp::DeleteBranch { name, .. } => GhostOp::DeleteBranch {
                branch: name.clone(),
                remote: false,
            },
            WriteOp::DropCommit { commit } => GhostOp::DropCommit {
                hash: commit.clone(),
            },
            WriteOp::Push { force: true } => GhostOp::ForcePush {
                branch: refs
                    .current
                    .clone()
                    .ok_or_else(|| PreviewFailure::RawPlan("Detached HEAD".to_owned()))?,
            },
            _ => return Ok(None),
        };
        let facts = ghost::op::Facts {
            refs,
            remotes: git::remotes(repo)?.into_iter().map(|r| r.name).collect(),
            dirty: ghost::op::tracked_changes(&git::status(repo)?),
        };
        ghost::approval_preview_with_witness(repo, &op, &facts, cancelled).map(Some)
    })();
    match result {
        Ok(preview) => preview,
        Err(failure) => {
            let mut preview = Preview::default();
            preview.push(Note::Unavailable { failure });
            Some(preview)
        }
    }
}

/// Private receipts originate only beside the actual owned note producers.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Note {
    RawPlan(String),
    DiscardsWork,
    FileChanges { count: usize },
    Conflict { step: usize, files: Vec<String> },
    LostMore,
    Unavailable { failure: PreviewFailure },
}

impl Note {
    fn raw(&self) -> String {
        match self {
            Self::RawPlan(raw) => raw.clone(),
            Self::DiscardsWork => "Discards uncommitted working changes".into(),
            Self::FileChanges { count } => format!("{count} working files change"),
            Self::Conflict { step, files } => format!("Stops at step {step}: {}", files.join(", ")),
            Self::LostMore => "Leaving-commit list is truncated at 200".into(),
            Self::Unavailable { failure } => {
                format!("Ghost preview unavailable: {}", failure.raw())
            }
        }
    }
    fn display(&self) -> String {
        match self {
            Self::RawPlan(raw) => ghost::display_preview_note(raw),
            Self::DiscardsWork => {
                crate::i18n::text("repo-workflows-ghost-approval-preview-675d236b").into()
            }
            Self::FileChanges { count } => crate::i18n::message(
                "repo-workflows-ghost-approval-preview-1426bc08",
                &[("files-len-1", (*count).into())],
            ),
            Self::Conflict { step, files } => crate::i18n::message(
                "repo-workflows-ghost-approval-preview-e74a52f6",
                &[
                    ("conflict-step-1", (*step).into()),
                    ("conflict-files-join-2", files.join(", ").into()),
                ],
            ),
            Self::LostMore => {
                crate::i18n::text("repo-workflows-ghost-approval-preview-70be46e5").into()
            }
            Self::Unavailable { failure } => {
                // This raw-plan sentinel is emitted by read's owned detached-HEAD branch.
                // Backend/provider failures retain their Backend::Opaque source and bypass it.
                let detail = match failure {
                    PreviewFailure::RawPlan(raw) if raw == "Detached HEAD" => {
                        crate::i18n::text("repo-workflows-agent-read-23b028a2").to_owned()
                    }
                    _ => failure.display(),
                };
                crate::i18n::message(
                    "repo-workflows-agent-read-e72061c5",
                    &[("e", detail.into())],
                )
            }
        }
    }
}

impl Preview {
    fn push(&mut self, note: Note) {
        self.note_receipts.push((self.notes.len(), note.clone()));
        self.notes.push(note.raw());
    }

    pub(in crate::repo) fn from_outcome(outcome: ghost::Outcome) -> Self {
        let mut preview = Self::default();
        for raw in outcome
            .notes
            .into_iter()
            .chain(outcome.warnings)
            .chain(outcome.refused)
        {
            preview.push(Note::RawPlan(raw));
        }
        if outcome.discards_work {
            preview.push(Note::DiscardsWork);
        }
        if let Some(files) = outcome.files {
            preview.push(Note::FileChanges { count: files.len() });
        }
        if let Some(conflict) = outcome.conflict {
            preview.push(Note::Conflict {
                step: conflict.step,
                files: conflict.files,
            });
        }
        if outcome.lost_more {
            preview.push(Note::LostMore);
        }
        preview.moves = outcome
            .moves
            .into_iter()
            .map(|m| {
                (
                    m.full,
                    m.from.unwrap_or_else(|| "absent".into()),
                    m.to.unwrap_or_else(|| "absent".into()),
                )
            })
            .collect();
        preview.leaving = outcome
            .lost
            .into_iter()
            .map(|c| format!("{} {}", c.hash, c.summary))
            .collect();
        preview
    }

    /// A transient paint snapshot; the retained planning/approval value is never modified.
    pub(super) fn for_display(&self) -> Self {
        let mut shown = self.clone();
        for (index, note) in &self.note_receipts {
            if let Some(raw) = shown.notes.get_mut(*index) {
                if *raw == note.raw() {
                    *raw = note.display();
                }
            }
        }
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_pending_delete_previews_its_exact_ref_without_mutating() -> Result<(), String> {
        let r = git::fixture::Repo::new("agent-ghost-preview")?;
        let oid = r.commit("base", &[])?;
        r.git(&["branch", "candidate"])?;
        let p = read(
            r.path(),
            &WriteOp::DeleteBranch {
                name: "candidate".into(),
                force: true,
            },
            &|| false,
        )
        .ok_or("No preview")?;
        assert!(
            p.moves
                .iter()
                .any(|(name, from, to)| name == "refs/heads/candidate"
                    && from == &oid
                    && to == "absent")
        );
        assert_eq!(r.rev("candidate")?, oid);
        let reset = read(
            r.path(),
            &WriteOp::Reset {
                commit: oid.clone(),
                mode: ResetMode::Hard,
            },
            &|| false,
        )
        .ok_or("No reset preview")?;
        assert!(
            !reset.notes.iter().any(|s| s.contains("unavailable")),
            "{reset:?}"
        );
        assert_eq!(r.rev("HEAD")?, oid);
        Ok(())
    }
}

#[cfg(test)]
mod fluent_approval_note_tests {
    use super::*;
    use crate::git::dry_run::Failure;
    use crate::repo::agent::{CardView, plan::Ask};
    use egui_kittest::{Harness, kittest::Queryable};
    use std::time::Duration;

    #[test]
    fn fluent_partition_actual_approval_card_projection_repaints_receipts_without_changing_snapshot()
     {
        let opaque = "git replay needs a branch; 日本語\n{ $detail } | --literal-pathspecs";
        let mut preview = Preview::from_outcome(ghost::Outcome {
            notes: vec![
                "Already up to date: nothing to replay, nothing changes.".into(),
                opaque.into(),
            ],
            refused: Some("Invalid timeline commit ID".into()),
            discards_work: true,
            files: Some(Vec::new()),
            lost_more: true,
            ..Default::default()
        });
        preview.push(Note::Conflict {
            step: 1234,
            files: vec!["raw/path;\nß".into()],
        });
        preview.moves.push((
            "refs/heads/ß\nopaque".into(),
            "a".repeat(40),
            "b".repeat(40),
        ));
        preview
            .leaving
            .push("rawOID original subject 日本語".into());
        preview.push(Note::Unavailable {
            failure: PreviewFailure::Backend(Failure::ReplayFallback {
                first: Box::new(Failure::ReplayNeedsBranch),
                scratch: Box::new(Failure::Opaque(opaque.into())),
            }),
        });
        preview.push(Note::Unavailable {
            failure: PreviewFailure::RawPlan("Detached HEAD".into()),
        });
        // A note with no producer receipt stays opaque even if it resembles an owned note.
        preview
            .notes
            .push("Discards uncommitted working changes".into());
        let raw = preview.clone();
        let card = CardView {
            agent: "agent raw/ß".into(),
            preview: Some(preview),
            ask: Ask {
                title: "Stable supplied title".into(),
                body: "Stable supplied body".into(),
                undo_note: "Stable supplied undo note".into(),
            },
            commands: vec!["git --literal-pathspecs reset --hard -- 'refs/heads/raw/ß'".into()],
            undoable: false,
            left: Duration::from_secs(100),
            shown_for: Duration::from_secs(2),
        };
        let retained = card.clone();
        let mut installed = false;
        let mut h = Harness::new_ui_state(
            |ui, cached: &mut CardView| {
                if !installed {
                    crate::theme::install_fonts(ui.ctx());
                    installed = true;
                    ui.ctx().request_discard("install supplied fonts");
                    return;
                }
                let shown = cached.clone().for_display();
                for note in &shown.preview.as_ref().unwrap().notes {
                    ui.label(note);
                }
            },
            card,
        );
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            h.run();
            let shown = h.state().clone().for_display();
            let painted = shown.preview.as_ref().unwrap();
            for note in &painted.notes {
                assert_eq!(
                    h.get_all_by_label(note).count(),
                    painted.notes.iter().filter(|value| *value == note).count()
                );
            }
            assert_eq!(h.state(), &retained);
            assert_eq!(h.state().preview.as_ref(), Some(&raw));
            assert_eq!(shown.commands, retained.commands);
            assert_eq!(shown.ask, retained.ask);
            assert_eq!(shown.undoable, retained.undoable);
            assert_eq!(shown.left, retained.left);
            assert_eq!(shown.shown_for, retained.shown_for);
            assert_eq!(painted.moves, raw.moves);
            assert_eq!(painted.leaving, raw.leaving);
            assert_eq!(painted.notes[1], opaque);
            assert_eq!(
                painted.notes.last().unwrap(),
                "Discards uncommitted working changes"
            );
            assert!(painted.notes.iter().any(|note| note.contains(opaque)));
            let conflict = &painted.notes[6];
            assert!(conflict.contains(if language == "de" { "1.234" } else { "1,234" }));
            assert!(painted.notes.iter().any(|note| note
                == if language == "de" {
                    "Verwirft uncommittete Arbeitsänderungen"
                } else {
                    "Discards uncommitted working changes"
                }));
        }
        let mut mismatched = raw.clone();
        mismatched.notes[0] = "provider replaced note: 日本語\nopaque".into();
        assert_eq!(mismatched.for_display().notes[0], mismatched.notes[0]);
        for count in [1, 1234] {
            let mut p = Preview::default();
            p.push(Note::FileChanges { count });
            let raw = p.notes.clone();
            for language in ["en-US", "de"] {
                crate::i18n::set_language(language);
                let painted = p.for_display();
                assert_eq!(p.notes, raw);
                assert!(painted.notes[0].contains(if count == 1 {
                    if language == "de" {
                        "Eine Datei"
                    } else {
                        "One working file"
                    }
                } else if language == "de" {
                    "1.234"
                } else {
                    "1,234"
                }));
            }
        }
        crate::i18n::set_language("en-US");
    }
}
