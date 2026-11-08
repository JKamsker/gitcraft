//! Checking out something else would leave the commits made on a detached HEAD on no branch:
//! keep them on a new branch first, or leave them.

use egui::{RichText, Ui};

use crate::repo::detached::{DetachedAct, KeepDialog};
use crate::repo::{Action, short};
use crate::theme::Tokens;

use super::{Outcome, cancel_button, dash_spaces, field, title, usable};

/// How many of the commits are listed by name.
const SHOWN: usize = 5;

/// `local`: the local branches, whose names a new one can't take.
pub(super) fn keep_detached(
    ui: &mut Ui,
    enter: bool,
    d: &mut KeepDialog,
    local: &[crate::git::Branch],
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    let target = d.target();
    let n = d.commits.len().min(crate::git::LOOSE_LISTED);
    title(ui, crate::i18n::text("repo-core-1349"), &explanation(d), &t);
    for (hash, summary) in d.commits.iter().take(SHOWN) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(short(hash)).monospace().color(t.text_faint));
            ui.add(egui::Label::new(summary.as_str()).truncate());
        });
    }
    if n > SHOWN {
        let rest = if d.more() {
            crate::i18n::text("repo-core-1350").to_owned()
        } else {
            crate::i18n::message(
                "repo-core-1099",
                &[("hidden", format!("{}", n - SHOWN).into())],
            )
        };
        ui.label(RichText::new(rest).color(t.text_faint).small());
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(crate::i18n::text("repo-core-1351"))
            .color(t.text_dim)
            .small(),
    );
    field(
        ui,
        &mut d.name,
        crate::i18n::text("repo-core-1352"),
        &mut d.focus,
    );
    dash_spaces(&mut d.name);
    ui.add_space(12.0);
    let taken = local.iter().any(|b| b.name == d.name.trim());
    if taken {
        ui.label(
            RichText::new(crate::i18n::text("repo-core-1353"))
                .color(t.warning)
                .small(),
        );
    }
    let valid = usable(&d.name) && !taken;
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        let keep = crate::repo::widgets::primary_button(
            ui,
            crate::i18n::text("repo-core-1354"),
            valid,
            90.0,
        )
        .on_hover_text(crate::i18n::message(
            "repo-core-1355",
            &[
                ("trim", d.name.trim().to_string().into()),
                ("target", target.to_string().into()),
            ],
        ));
        if keep.clicked() || (enter && valid) {
            outcome = Outcome::Done(Box::new(Action::Detached(DetachedAct::Keep {
                name: d.name.trim().to_owned(),
                head: d.head.clone(),
                leave: d.leave.clone(),
            })));
        }
        if ui
            .add(
                egui::Button::new(crate::i18n::text("repo-core-1356"))
                    .min_size(egui::vec2(80.0, 30.0)),
            )
            .on_hover_text(crate::i18n::message(
                "repo-core-1357",
                &[("target", target.to_string().into())],
            ))
            .clicked()
        {
            outcome = Outcome::Done(Box::new(Action::Detached(DetachedAct::Leave(
                d.leave.clone(),
            ))));
        }
        if cancel_button(ui) {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// What checking out would do to the commits, in a sentence or two.
fn explanation(d: &KeepDialog) -> String {
    let n = d.commits.len().min(crate::git::LOOSE_LISTED);
    crate::i18n::message(
        "repo-core-1363",
        &[
            ("count", n.into()),
            ("more", if d.more() { "yes" } else { "no" }.into()),
            ("target", d.target().into()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::detached::Leave;

    fn dialog(commits: usize, target: &str) -> KeepDialog {
        KeepDialog {
            commits: (0..commits)
                .map(|i| (format!("{i:040x}"), format!("c{i}")))
                .collect(),
            head: String::new(),
            leave: Leave::Checkout(target.to_owned()),
            name: String::new(),
            focus: false,
        }
    }

    #[test]
    fn the_explanation_counts_the_commits_and_names_what_is_checked_out_readably() {
        assert_eq!(
            explanation(&dialog(1, "main")),
            "The commit made here is on no branch. Checking out main leaves it where nothing \
             shows it; git only keeps it in its reflog, for a while."
        );
        let hash = "ab".repeat(20);
        assert!(explanation(&dialog(3, &hash)).starts_with("The 3 commits made here are"));
        assert!(explanation(&dialog(3, &hash)).contains("Checking out abababab leaves them"));
        assert!(explanation(&dialog(2, "refs/tags/v1.0 ✓")).contains("out v1.0 ✓ leaves"));
        let many = explanation(&dialog(crate::git::LOOSE_LISTED + 1, "main"));
        assert!(many.starts_with(&format!("More than {} commits", crate::git::LOOSE_LISTED)));
    }
}
