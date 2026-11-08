//! A folded file's diff: one line saying what it changes and why it's folded, and a button
//! that opens it.

use egui::{RichText, Ui};

use super::{FoldReason, stats_line};
use crate::repo::diff::DiffData;
use crate::theme::Tokens;

/// Shows the folded diff of the file at `path` in place of its lines (`data`: once read, for
/// its counts). True when it was opened.
pub(in crate::repo) fn folded_diff(
    ui: &mut Ui,
    path: &str,
    data: Option<&DiffData>,
    reason: FoldReason,
) -> bool {
    let t = Tokens::get(ui.ctx());
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    let mut open = false;
    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-core-2539",
                &[("name", format!("{}", name).into())],
            ))
            .color(t.text_dim),
        );
        ui.add_space(4.0);
        let line = match data {
            Some(data) => {
                let (added, removed) = data.line_counts;
                stats_line(added, removed, reason)
            }
            None => reason.label().to_owned(),
        };
        ui.label(RichText::new(line).color(t.text_faint).monospace());
        ui.add_space(10.0);
        open = ui
            .button(crate::i18n::text("repo-core-2540"))
            .on_hover_text(crate::i18n::text("repo-core-2541"))
            .clicked();
    });
    open
}
