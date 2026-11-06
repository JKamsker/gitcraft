//! The tab's side of the rich views: which is on, reading it in the background, and going
//! back from a part of one to its line in the plain diff.

use super::{Rich, RichKind, RichState};
use crate::repo::diff::{DiffLine, Kind};
use crate::repo::messages::{Msg, spawn};
use crate::repo::{DiffView, RepoTab};

impl RepoTab {
    /// The rich view the open diff shows, if one is on and its file can be read that way.
    pub(in crate::repo) fn diff_rich_kind(&self) -> Option<RichKind> {
        let kind = self.diff_rich?;
        let view = self.diff.as_ref()?;
        RichKind::of(view.key.path())
            .contains(&kind)
            .then_some(kind)
    }

    /// Reads the open diff as the rich view that is on, in the background.
    pub(in crate::repo) fn load_diff_rich(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.diff_rich_kind() else {
            return;
        };
        let Some(view) = self.diff.as_mut() else {
            return;
        };
        view.rich_load += 1;
        let ids = (view.request, view.rich_load);
        let (key, path) = (view.key.clone(), self.path.clone());
        spawn(&self.tx, ctx, move || {
            let result = super::load(&path, &key, kind);
            Msg::DiffRich(ids, kind, Box::new(result))
        });
    }

    /// Shows diffs as `kind` (`None`: as their lines).
    pub(in crate::repo) fn show_diff_rich(&mut self, kind: Option<RichKind>, ctx: &egui::Context) {
        self.diff_rich = kind;
        // One reading at a time: the Markdown rendered gives way.
        if kind.is_some() {
            self.diff_rendered = false;
        }
        if let Some(view) = &mut self.diff {
            view.rich = None;
            view.rich_state = RichState::default();
        }
        // Read again each time it's turned on: the file may have changed since.
        self.load_diff_rich(ctx);
    }

    /// Leaves the rich view for the plain diff, scrolled to the line numbered `old` before the
    /// change or `new` after it.
    pub(in crate::repo) fn diff_go_to_line(&mut self, old: Option<u32>, new: Option<u32>) {
        self.diff_rich = None;
        self.diff_as_file = false;
        if let Some(view) = &mut self.diff {
            view.rich = None;
            if let Some(Ok(data)) = &view.data {
                view.jump_line = line_index(&data.lines, old, new);
            }
        }
    }
}

impl DiffView {
    /// Shows the rich view read along with the read numbered `request`, unless a newer read
    /// was made since.
    pub(in crate::repo) fn rich_arrived(
        &mut self,
        (request, load): (u64, u64),
        kind: RichKind,
        result: Result<Rich, String>,
    ) {
        if request == self.request && load == self.rich_load {
            self.rich = Some((kind, result));
        }
    }
}

/// The line of a diff numbered `new` after the change (or `old` before it), or the nearest one
/// shown when the diff doesn't show it.
pub(super) fn line_index(lines: &[DiffLine], old: Option<u32>, new: Option<u32>) -> Option<usize> {
    let exact = lines.iter().position(|l| match (new, old) {
        (Some(n), _) if l.kind != Kind::Del && l.new == Some(n) => true,
        (None, Some(o)) => l.old == Some(o),
        _ => false,
    });
    exact.or_else(|| {
        let distance = |l: &DiffLine| match (new, old, l.new, l.old) {
            (Some(n), _, Some(x), _) => Some(n.abs_diff(x)),
            (None, Some(o), _, Some(x)) => Some(o.abs_diff(x)),
            _ => None,
        };
        lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| Some((distance(l)?, i)))
            .min()
            .map(|(_, i)| i)
    })
}
