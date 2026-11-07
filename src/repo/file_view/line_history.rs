//! The file view showing the history of a few lines of the file (`git log -L`): every commit
//! that touched them, and beside them the part of the one clicked that changed them.

use egui::{Stroke, Ui};

use super::{FileTab, FileView};
use crate::git::{self, LineRange, LinesAt};
use crate::icons;
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// Shows the history of lines `range` of `path`, numbered as they are `at`, and reads it.
    pub(in crate::repo) fn open_line_history(
        &mut self,
        path: String,
        at: LinesAt,
        range: LineRange,
        ctx: &egui::Context,
    ) {
        self.tour = None;
        let range = (range.0.min(range.1), range.0.max(range.1));
        self.diff = None;
        self.conflict = None;
        let rev = match &at {
            LinesAt::Rev(rev) => Some(rev.clone()),
            LinesAt::Index | LinesAt::WorkTree => None,
        };
        let mut view = FileView::new(&path, rev, FileTab::History);
        view.lines = Some((at.clone(), range));
        self.file_view = Some(view);
        let repo = self.path.clone();
        let asked = (path.clone(), at.clone(), range);
        self.then(
            ctx,
            move || git::line_history(&repo, &at, &path, range),
            move |tab, result, _| {
                // Only for the lines still shown: another history may have been opened since.
                if let Some(view) = &mut tab.file_view
                    && view.path == asked.0
                    && view.lines == Some((asked.1, asked.2))
                {
                    view.history = Some(result);
                }
            },
        );
    }

    /// The header (the file, which lines, the whole file's history) and the commits.
    pub(super) fn line_history_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &self.file_view else { return };
        let Some((at, range)) = &view.lines else {
            return;
        };
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "history", 16.0, t.icon);
                    ui.label(egui::RichText::new(&view.path).font(theme::medium(13.0)));
                    widgets::chip(ui, &lines_label(*range), t.accent);
                    widgets::chip(ui, &where_label(at), t.text_dim);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                            acts.push(Action::CloseFileView);
                        }
                        let whole = ui.button("Whole file").on_hover_text(
                            "Every commit that changed the file, not only these lines",
                        );
                        if whole.clicked() {
                            acts.push(Action::FileView {
                                path: view.path.clone(),
                                rev: view.rev.clone(),
                                tab: FileTab::History,
                            });
                        }
                    });
                });
            });
        self.file_history(ui, acts);
    }
}

/// "Line 4" or "Lines 4–9".
pub(super) fn lines_label((start, end): LineRange) -> String {
    if start == end {
        format!("Line {start}")
    } else {
        format!("Lines {start}–{end}")
    }
}

/// Where the lines' numbers count from.
fn where_label(at: &LinesAt) -> String {
    match at {
        LinesAt::Rev(rev) => match rev.strip_suffix('^') {
            Some(commit) => format!("before {}", short(commit)),
            None => format!("at {}", short(rev)),
        },
        LinesAt::Index => "as staged".to_owned(),
        LinesAt::WorkTree => "in the working tree".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_say_which_lines_and_where() {
        assert_eq!(lines_label((4, 4)), "Line 4");
        assert_eq!(lines_label((4, 9)), "Lines 4–9");
        let hash = "0123456789abcdef".to_owned();
        assert_eq!(where_label(&LinesAt::Rev(hash.clone())), "at 01234567");
        assert_eq!(
            where_label(&LinesAt::Rev(format!("{hash}^"))),
            "before 01234567"
        );
        assert_eq!(where_label(&LinesAt::WorkTree), "in the working tree");
    }
}
