//! The file view showing the history of a few lines of the file (`git log -L`): every commit
//! that touched them, and beside them the part of the one clicked that changed them.

use egui::{Stroke, Ui};

use super::{FileTab, FileView};
use crate::git::{self, LineRange, LinesAt};
use crate::icons;
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

type LineHistoryOutcome = Result<
    (
        Option<(String, String, LineRange)>,
        Result<Vec<git::FileCommit>, String>,
    ),
    String,
>;

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
        self.load_line_history(ctx);
    }

    pub(in crate::repo) fn load_line_history(&mut self, ctx: &egui::Context) {
        let Some(view) = &self.file_view else { return };
        let Some((at, range)) = view.lines.clone() else {
            return;
        };
        let generation = view.generation;
        let path = view.path.clone();
        let repo = self.path.clone();
        let Some(token) = self.begin_file_load(FileTab::History) else {
            return;
        };
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                let inputs = git::line_history_key(&repo, &at, &path, range);
                let result =
                    inputs
                        .as_ref()
                        .map_err(Clone::clone)
                        .and_then(|(rev, path, range)| {
                            git::line_history(&repo, &LinesAt::Rev(rev.clone()), path, *range)
                        });
                Ok((inputs.ok(), result))
            },
            move |tab, outcome, ctx| tab.finish_line_history(generation, token, outcome, ctx),
        );
    }

    pub(super) fn finish_line_history(
        &mut self,
        generation: u64,
        token: u64,
        outcome: LineHistoryOutcome,
        ctx: &egui::Context,
    ) {
        if !self.finish_file_load(
            FileTab::History,
            token,
            generation,
            outcome.as_ref().err().map(String::as_str),
            ctx,
        ) {
            return;
        }
        let (inputs, result) = outcome.unwrap_or_else(|error| (None, Err(error)));
        if result.is_ok()
            && let Some(view) = &self.file_view
            && view
                .line_observed
                .as_ref()
                .is_some_and(|observed| Some(observed) != inputs.as_ref())
        {
            self.invalidate_mutable_file_view(ctx);
            return;
        }
        if let Some(view) = &mut self.file_view {
            view.line_inputs = inputs;
            view.history = Some(result);
        }
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
                        if icons::small_button(ui, "x", crate::i18n::text("repo-core-0711"))
                            .clicked()
                        {
                            acts.push(Action::CloseFileView);
                        }
                        let whole = ui
                            .button(crate::i18n::text("repo-core-2489"))
                            .on_hover_text(crate::i18n::text("repo-core-2490"));
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

#[cfg(test)]
/// Capture the opening as well as its range: reopening identical lines is a new request.
pub(super) fn history_reply(
    generation: u64,
    path: String,
    at: LinesAt,
    range: LineRange,
) -> impl FnOnce(&mut RepoTab, Result<Vec<git::FileCommit>, String>, &egui::Context) {
    move |tab, result, _| {
        if let Some(view) = &mut tab.file_view
            && view.generation == generation
            && view.path == path
            && view.lines.as_ref() == Some(&(at, range))
        {
            view.history = Some(result);
        }
    }
}

/// "Line 4" or "Lines 4–9".
pub(super) fn lines_label((start, end): LineRange) -> String {
    if start == end {
        crate::i18n::message("repo-core-2491", &[("start", format!("{}", start).into())])
    } else {
        crate::i18n::message(
            "repo-core-2492",
            &[
                ("start", format!("{}", start).into()),
                ("end", format!("{}", end).into()),
            ],
        )
    }
}

/// Where the lines' numbers count from.
fn where_label(at: &LinesAt) -> String {
    match at {
        LinesAt::Rev(rev) => match rev.strip_suffix('^') {
            Some(commit) => crate::i18n::message(
                "repo-core-2493",
                &[("commit", short(commit).to_string().into())],
            ),
            None => {
                crate::i18n::message("repo-core-2494", &[("rev", short(rev).to_string().into())])
            }
        },
        LinesAt::Index => crate::i18n::text("repo-core-2495").to_owned(),
        LinesAt::WorkTree => crate::i18n::text("repo-core-2496").to_owned(),
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
