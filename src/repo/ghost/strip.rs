//! The strip over the foot of the graph during a ghost run: what the operation would do, in
//! words.

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
    /// The strip, `width` wide, while a ghost run is on.
    pub(in crate::repo) fn ghost_strip(&self, ui: &mut Ui, width: f32) {
        let Some(state) = self.ghost_strip_state() else {
            return;
        };
        let above = self.ghost.rows_above();
        let t = Tokens::get(ui.ctx());
        egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.accent.gamma_multiply(0.6)))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .corner_radius(egui::CornerRadius::same(t.radius as u8))
            .shadow(egui::Shadow {
                offset: [0, 2],
                blur: 10,
                spread: 0,
                color: t.shadow,
            })
            .show(ui, |ui| {
                let inner = (width - 22.0).max(120.0);
                ui.set_width(inner);
                ui.set_max_width(inner);
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
                        alert(
                            ui,
                            &t,
                            t.danger,
                            &format!("Can't preview it. {}", sentence(one_line(why))),
                        );
                    }
                    Strip::Ready(_, outcome) => outcome_lines(ui, &t, outcome),
                }
                if above > 0 {
                    dim(
                        ui,
                        &t,
                        &format!(
                            "↑ {} above the rows in view.",
                            plural(above, "new commit is", "new commits are")
                        ),
                    );
                }
            });
    }
}

/// What `outcome` does, a line each.
fn outcome_lines(ui: &mut Ui, t: &Tokens, o: &Outcome) {
    if let Some(why) = &o.refused {
        alert(ui, t, t.danger, &sentence(why));
    }
    for warning in &o.warnings {
        alert(ui, t, t.warning, warning);
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
        let why = if c.files.is_empty() {
            "the commit becomes empty".to_owned()
        } else {
            format!("conflicts in {}", listed(&c.files, |f| f.clone()))
        };
        alert(ui, t, t.danger, &format!("{what}, {why}."));
    }
    if let Some(dropped) = &o.dropped {
        line(
            ui,
            t.text,
            &format!("Drops {} — {}", describe(dropped), dropped.author),
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
                "Files of the working tree that change ({}): {}",
                files.len(),
                listed(files, |f| format!("{} {}", f.status, f.path))
            ),
        ),
        Some(_) => dim(ui, t, "No file of the working tree changes."),
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
    let count = if o.lost_more {
        format!("more than {} commits", o.lost.len())
    } else {
        plural(o.lost.len(), "commit", "commits")
    };
    let names = listed(&o.lost, |c| format!("{} — {}", describe(c), c.author));
    let (color, text) = if o.from_remote {
        (
            t.warning,
            format!("Leaving the remote's branch ({count}): {names}"),
        )
    } else if o.replaced == o.lost.len() && !o.lost_more {
        let originals = if o.replaced == 1 {
            "the original is"
        } else {
            "the originals are"
        };
        (
            t.text_dim,
            format!("Replaced by new copies ({count}; {originals} left on no branch): {names}"),
        )
    } else if o.replaced > 0 {
        (
            t.warning,
            format!(
                "Left on no branch ({count}, {} of them replaced by new copies): {names}",
                o.replaced
            ),
        )
    } else {
        (t.warning, format!("Left on no branch ({count}): {names}"))
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

/// A line that needs attention: an alert icon in `color`, the text in the theme's text colour
/// (a red or orange text is hard to read on some themes' cards).
fn alert(ui: &mut Ui, t: &Tokens, color: egui::Color32, text: &str) {
    ui.horizontal_wrapped(|ui| {
        icons::inline(ui, "circle-alert", 13.0, color);
        ui.label(RichText::new(text).size(12.0).color(t.text));
    });
}

/// `text` as a sentence: its first letter upper case.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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
        assert_eq!(sentence("x isn't merged"), "X isn't merged");
    }
}
