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
                        RichText::new(crate::i18n::message(
                            "repo-workflows-ghost-ghost-strip-c4a8b8f8",
                            &[("title", (title).clone().into())],
                        ))
                        .font(theme::semibold(12.5))
                        .color(t.text),
                    );
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-ghost-ghost-strip-a81f72de",
                        ))
                        .size(11.5)
                        .color(t.text_faint),
                    );
                    if matches!(state, Strip::Working(_)) {
                        ui.add(crate::spinner::Spinner::new().size(12.0));
                    }
                });
                match state {
                    Strip::Working(_) => {
                        dim(
                            ui,
                            &t,
                            crate::i18n::text("repo-workflows-ghost-ghost-strip-d0c20bfd"),
                        );
                    }
                    Strip::Failed(_, why) => {
                        alert(
                            ui,
                            &t,
                            t.danger,
                            &crate::i18n::message(
                                "repo-workflows-ghost-ghost-strip-f637d899",
                                &[(
                                    "sentence-one-line-why-1",
                                    (sentence(one_line(why))).clone().into(),
                                )],
                            ),
                        );
                    }
                    Strip::Ready(_, outcome) => outcome_lines(ui, &t, outcome),
                }
                if above > 0 {
                    dim(
                        ui,
                        &t,
                        &crate::i18n::message(
                            "repo-workflows-ghost-ghost-strip-b69b3d5d",
                            &[(
                                "plural-above-new-commit-is-new-c-1",
                                (plural(above, "new commit is", "new commits are"))
                                    .clone()
                                    .into(),
                            )],
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
            Some(commit) => crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-7728bdcc",
                &[
                    ("c-step-1", (c.step).clone().into()),
                    ("c-of-2", (c.of).clone().into()),
                    ("describe-commit-3", (describe(commit)).clone().into()),
                ],
            ),
            None => crate::i18n::text("repo-workflows-ghost-outcome-lines-193731dd").to_owned(),
        };
        let why = if c.files.is_empty() {
            crate::i18n::text("repo-workflows-ghost-outcome-lines-f0e05d95").to_owned()
        } else {
            crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-f93fde8f",
                &[(
                    "listed-c-files-f-f-clone-1",
                    (listed(&c.files, |f| f.clone())).clone().into(),
                )],
            )
        };
        alert(ui, t, t.danger, &format!("{what}, {why}."));
    }
    if let Some(dropped) = &o.dropped {
        line(
            ui,
            t.text,
            &crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-b8d8f166",
                &[
                    ("describe-dropped-1", (describe(dropped)).clone().into()),
                    ("dropped-author-2", (dropped.author).clone().into()),
                ],
            ),
        );
    }
    let fetched = o.created.len() - o.copies;
    let mut made = Vec::new();
    if o.copies > 0 {
        made.push(plural(o.copies, "new commit", "new commits"));
    }
    if fetched > 0 {
        made.push(crate::i18n::message(
            "repo-workflows-ghost-outcome-lines-dfa9e9b6",
            &[(
                "plural-fetched-commit-commits-1",
                (plural(fetched, "commit", "commits")).clone().into(),
            )],
        ));
    }
    for m in &o.moves {
        let name = m.full.strip_prefix("refs/heads/").unwrap_or(&m.full);
        let name = name.strip_prefix("refs/remotes/").unwrap_or(name);
        made.push(match (&m.from, &m.to) {
            (_, None) => crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-91af17db",
                &[("name", (name).clone().into())],
            ),
            (Some(from), Some(to)) => crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-ce73844c",
                &[
                    ("name", (name).clone().into()),
                    ("short-from-1", (short(from)).clone().into()),
                    ("shown-to-2", (shown(to)).clone().into()),
                ],
            ),
            (None, Some(to)) => crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-81deb64d",
                &[
                    ("name", (name).clone().into()),
                    ("shown-to-1", (shown(to)).clone().into()),
                ],
            ),
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
            &crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-4b54308f",
                &[
                    ("files-len-1", (files.len()).clone().into()),
                    (
                        "listed-files-f-format-f-status-f-2",
                        (listed(files, |f| format!("{} {}", f.status, f.path)))
                            .clone()
                            .into(),
                    ),
                ],
            ),
        ),
        Some(_) => dim(
            ui,
            t,
            crate::i18n::text("repo-workflows-ghost-outcome-lines-02e3b1f7"),
        ),
        None => {}
    }
    for note in &o.notes {
        dim(ui, t, note);
    }
    if let Some(engine) = o.engine {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-ghost-outcome-lines-820df962",
                &[("engine-describe-1", (engine.describe()).clone().into())],
            ))
            .size(10.5)
            .color(t.text_faint),
        );
    }
}

/// The commits the operation leaves on no ref (or takes off the remote's branch), with who
/// made them.
fn lost_line(ui: &mut Ui, t: &Tokens, o: &Outcome) {
    let count = if o.lost_more {
        crate::i18n::message(
            "repo-workflows-ghost-lost-line-50788b96",
            &[("o-lost-len-1", (o.lost.len()).clone().into())],
        )
    } else {
        plural(o.lost.len(), "commit", "commits")
    };
    let names = listed(&o.lost, |c| format!("{} — {}", describe(c), c.author));
    let (color, text) = if o.from_remote {
        (
            t.warning,
            crate::i18n::message(
                "repo-workflows-ghost-lost-line-02717c8c",
                &[
                    ("count", (count).clone().into()),
                    ("names", (names).clone().into()),
                ],
            ),
        )
    } else if o.replaced == o.lost.len() && !o.lost_more {
        let originals = if o.replaced == 1 {
            "the original is"
        } else {
            "the originals are"
        };
        (
            t.text_dim,
            crate::i18n::message(
                "repo-workflows-ghost-lost-line-2cd6d050",
                &[
                    ("count", (count).clone().into()),
                    ("originals", (originals).clone().into()),
                    ("names", (names).clone().into()),
                ],
            ),
        )
    } else if o.replaced > 0 {
        (
            t.warning,
            crate::i18n::message(
                "repo-workflows-ghost-lost-line-8ccbdaa0",
                &[
                    ("count", (count).clone().into()),
                    ("o-replaced-1", (o.replaced).clone().into()),
                    ("names", (names).clone().into()),
                ],
            ),
        )
    } else {
        (
            t.warning,
            crate::i18n::message(
                "repo-workflows-ghost-lost-line-811bd7ca",
                &[
                    ("count", (count).clone().into()),
                    ("names", (names).clone().into()),
                ],
            ),
        )
    };
    line(ui, color, &text);
}

/// A commit as the strip names where a ref goes.
fn shown(hash: &str) -> String {
    if hash == super::plan::GHOST_MERGE {
        crate::i18n::text("repo-workflows-ghost-shown-4ef6715c").to_owned()
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
        names.push(crate::i18n::message(
            "repo-workflows-ghost-listed-683768ab",
            &[("items-len-listed-1", (items.len() - LISTED).clone().into())],
        ));
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
