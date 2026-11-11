use super::*;

impl Side {
    pub(in crate::repo) fn display_kind(&self) -> &'static str {
        let id = match self.kind() {
            "unchanged" => "repo-workflows-bakeoff-kind-unchanged",
            "deleted" => "repo-workflows-bakeoff-kind-deleted",
            "added" => "repo-workflows-bakeoff-kind-added",
            "changed" => "repo-workflows-bakeoff-kind-changed",
            other => return other,
        };
        crate::i18n::text(id)
    }
}

#[derive(Clone)]
pub(super) enum BakeNote {
    Unassembled { count: usize },
    Changed,
    CommandMissing,
    Made { branch: String, commit: String },
}
impl BakeNote {
    pub(in crate::repo) fn raw(&self) -> String {
        match self {
            Self::Unassembled { count } => format!(
                "{count} file{} picked and not assembled: close again to drop them",
                if *count == 1 { "" } else { "s" }
            ),
            Self::Changed => "Picks cleared: the attempts changed since they were made".into(),
            Self::CommandMissing => "Type the command that builds the project first".into(),
            Self::Made { branch, commit } => format!("Made {branch} at {commit}"),
        }
    }
    pub(in crate::repo) fn display(&self) -> String {
        match self {
            Self::Unassembled { count } => crate::i18n::message(
                "repo-workflows-bakeoff-unassembled",
                &[("count", (*count).into())],
            ),
            Self::Changed => {
                crate::i18n::text("repo-workflows-bakeoff-load-bakeoff-c6f043db").into()
            }
            Self::CommandMissing => {
                crate::i18n::text("repo-workflows-bakeoff-build-c18d3706").into()
            }
            Self::Made { branch, commit } => crate::i18n::message(
                "repo-workflows-bakeoff-assemble-c7f55e42",
                &[
                    ("branch", branch.as_str().into()),
                    ("crate-repo-short-commit-1", commit.as_str().into()),
                ],
            ),
        }
    }
}
impl Bakeoff {
    pub(super) fn remember_note(&mut self, note: BakeNote, error: bool) {
        let raw = note.raw();
        self.note = Some((raw.clone(), error));
        self.note_presentation = Some((raw, note));
    }
    pub(in crate::repo) fn display_note(&self, raw: &str) -> String {
        match &self.note_presentation {
            Some((old, note)) if old == raw => note.display(),
            _ => raw.into(),
        }
    }
}
