//! The strip above the graph during a ghost run: what the operation would do, in words.

use egui::{RichText, Ui};

use super::Strip;
use super::op::short;
use super::plan::{Outcome, describe};
use crate::icons;
use crate::repo::RepoTab;
use crate::theme::{self, Tokens};

/// How many commits and files the strip names before "and N more".
const LISTED: usize = 5;

impl RepoTab {
    /// The strip, while a ghost run is on.
    pub(in crate::repo) fn ghost_strip(&self, ui: &mut Ui) {
        let Some(state) = self.ghost_strip_state() else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.accent.gamma_multiply(0.6)))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 3.0;
                let title = match &state {
                    Strip::Working(title) | Strip::Ready(title, _) | Strip::Failed(title, _) => {
                        title
                    }
                };
                ui.horizontal(|ui| {
                    icons::inline(ui, "eye", 14.0, t.accent);
                    ui.label(
                        RichText::new(format!("Preview: {title}"))
                            .font(theme::semibold(12.5))
                            .color(t.text),
                    );
                    ui.label(
                        RichText::new("nothing has been done")
                            .size(11.5)
                            .color(t.text_faint),
                    );
                    if matches!(state, Strip::Working(_)) {
                        ui.add(crate::spinner::Spinner::new().size(12.0));
                    }
                });
                match state {
                    Strip::Working(_) => {
                        dim(ui, &t, "Working out what it would do…");
                    }
                    Strip::Failed(_, why) => {
                        line(
                            ui,
                            t.danger,
                            &format!("Can't preview it: {}", one_line(why)),
                        );
                    }
                    Strip::Ready(_, outcome) => outcome_lines(ui, &t, outcome),
                }
            });
    }
}

/// What `outcome` does, a line each.
fn outcome_lines(ui: &mut Ui, t: &Tokens, o: &Outcome) {
    if let Some(why) = &o.refused {
        line(ui, t.danger, why);
    }
    if let Some(c) = &o.conflict {
        let what = match &c.commit {
            Some(commit) => format!(
                "Stops at step {} of {} ({})",
                c.step,
                c.of,
                describe(commit)
            ),
            None => "The merge stops".to_owned(),
        };
        line(
            ui,
            t.danger,
            &format!("{what}: conflicts in {}", listed(&c.files, |f| f.clone())),
        );
    }
    let fetched = o.created.len() - o.copies;
    let mut made = Vec::new();
    if o.copies > 0 {
        made.push(plural(o.copies, "new commit", "new commits"));
    }
    if fetched > 0 {
        made.push(format!(
            "{} from the remote",
            plural(fetched, "commit", "commits")
        ));
    }
    for m in &o.moves {
        let name = m.full.strip_prefix("refs/heads/").unwrap_or(&m.full);
        let name = name.strip_prefix("refs/remotes/").unwrap_or(name);
        made.push(match (&m.from, &m.to) {
            (_, None) => format!("{name} is deleted"),
            (Some(from), Some(to)) => format!("{name} moves {} → {}", short(from), shown(to)),
            (None, Some(to)) => format!("{name} is made at {}", shown(to)),
        });
    }
    if !made.is_empty() {
        line(ui, t.text, &made.join(" · "));
    }
    if !o.lost.is_empty() {
        lost_line(ui, t, o);
    }
    match &o.files {
        Some(files) if !files.is_empty() => line(
            ui,
            t.text_dim,
            &format!(
                "Working tree: {} change: {}",
                plural(files.len(), "file", "files"),
                listed(files, |f| format!("{} {}", f.status, f.path))
            ),
        ),
        Some(_) => dim(ui, t, "Working tree: no file changes."),
        None => {}
    }
    for note in &o.notes {
        dim(ui, t, note);
    }
    if let Some(engine) = o.engine {
        ui.label(
            RichText::new(format!("Worked out with {}", engine.describe()))
                .size(10.5)
                .color(t.text_faint),
        );
    }
}

/// The commits the operation leaves on no ref (or takes off the remote's branch), with who
/// made them.
fn lost_line(ui: &mut Ui, t: &Tokens, o: &Outcome) {
    let more = if o.lost_more { " and more" } else { "" };
    let count = plural(o.lost.len(), "commit", "commits");
    let names = listed(&o.lost, |c| format!("{} — {}", describe(c), c.author));
    let (color, text) = if o.from_remote {
        (
            t.warning,
            format!("Leaving the remote's branch: {count}{more}: {names}"),
        )
    } else if o.replaced == o.lost.len() && !o.lost_more {
        (
            t.text_dim,
            format!("Replaced by the new copies (the originals are left on no branch): {names}"),
        )
    } else if o.replaced > 0 {
        (
            t.warning,
            format!(
                "Left on no branch: {count}{more} ({} replaced by the new copies): {names}",
                o.replaced
            ),
        )
    } else {
        (
            t.warning,
            format!("Left on no branch: {count}{more}: {names}"),
        )
    };
    line(ui, color, &text);
}

/// A commit as the strip names where a ref goes.
fn shown(hash: &str) -> String {
    if hash == super::plan::GHOST_MERGE {
        "a merge commit".to_owned()
    } else {
        short(hash).to_owned()
    }
}

fn line(ui: &mut Ui, color: egui::Color32, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(color));
}

fn dim(ui: &mut Ui, t: &Tokens, text: &str) {
    line(ui, t.text_dim, text);
}

fn one_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The first [`LISTED`] of `items`, then how many more.
fn listed<T>(items: &[T], name: impl Fn(&T) -> String) -> String {
    let mut names: Vec<String> = items.iter().take(LISTED).map(name).collect();
    if items.len() > LISTED {
        names.push(format!("and {} more", items.len() - LISTED));
    }
    names.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_cut_with_how_many_more() {
        let items: Vec<String> = (1..=7).map(|n| n.to_string()).collect();
        assert_eq!(listed(&items, String::clone), "1, 2, 3, 4, 5, and 2 more");
        assert_eq!(listed(&items[..2], String::clone), "1, 2");
        assert_eq!(plural(1, "file", "files"), "1 file");
        assert_eq!(plural(3, "file", "files"), "3 files");
    }
}
