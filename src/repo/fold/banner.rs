//! A folded file's diff: one line saying what it changes and why it's folded, and a button
//! that opens it.

use egui::{RichText, Ui};

use super::{FoldReason, stats_line};
use crate::repo::diff::{DiffData, Kind};
use crate::theme::Tokens;

/// The lines `data` adds and removes; `None` when it has no lines to count (a binary file,
/// one git doesn't diff), as `--numstat` says of those.
pub(in crate::repo) fn line_counts(data: &DiffData) -> (Option<u32>, Option<u32>) {
    if data.lines.iter().all(|l| l.kind == Kind::Meta) {
        return (None, None);
    }
    let count = |kind| {
        let n = data.lines.iter().filter(|l| l.kind == kind).count();
        Some(u32::try_from(n).unwrap_or(u32::MAX))
    };
    (count(Kind::Add), count(Kind::Del))
}

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
        ui.label(RichText::new(format!("{name} is folded")).color(t.text_dim));
        ui.add_space(4.0);
        let line = match data {
            Some(data) => {
                let (added, removed) = line_counts(data);
                stats_line(added, removed, reason)
            }
            None => reason.label().to_owned(),
        };
        ui.label(RichText::new(line).color(t.text_faint).monospace());
        ui.add_space(10.0);
        open = ui
            .button("Show the changes")
            .on_hover_text("Settings → Git → Diffs turns folding off")
            .clicked();
    });
    open
}
