//! Checking out something else would leave the commits made on a detached HEAD on no branch:
//! keep them on a new branch first, or leave them.

use egui::{RichText, Ui};

use crate::repo::detached::{DetachedAct, KeepDialog};
use crate::repo::{Action, short};
use crate::theme::Tokens;

use super::{Outcome, cancel_button, dash_spaces, field, title, usable};

/// How many of the commits are listed by name.
const SHOWN: usize = 5;

pub(super) fn keep_detached(ui: &mut Ui, enter: bool, d: &mut KeepDialog) -> Outcome {
    let t = Tokens::get(ui.ctx());
    let n = d.commits.len().min(crate::git::LOOSE_LISTED);
    let count = if d.more() {
        format!("More than {n} commits")
    } else if n == 1 {
        "1 commit".to_owned()
    } else {
        format!("{n} commits")
    };
    let target = d.target().to_owned();
    title(
        ui,
        "Keep the commits made here?",
        &format!(
            "{count} on this detached HEAD belong to no branch. Checking out {target} leaves \
             them behind, where only the reflog finds them, for a while."
        ),
        &t,
    );
    for (hash, summary) in d.commits.iter().take(SHOWN) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(short(hash)).monospace().color(t.text_faint));
            ui.add(egui::Label::new(summary.as_str()).truncate());
        });
    }
    if n > SHOWN {
        let rest = if d.more() {
            "and more".to_owned()
        } else {
            format!("and {} more", n - SHOWN)
        };
        ui.label(RichText::new(rest).color(t.text_faint).small());
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new("Branch to keep them on")
            .color(t.text_dim)
            .small(),
    );
    field(ui, &mut d.name, "branch name", &mut d.focus);
    dash_spaces(&mut d.name);
    ui.add_space(12.0);
    let valid = usable(&d.name);
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        let keep = crate::repo::widgets::primary_button(ui, "Keep and check out", valid, 90.0);
        if keep.clicked() || (enter && valid) {
            outcome = Outcome::Done(Action::Detached(DetachedAct::Keep {
                name: d.name.trim().to_owned(),
                head: d.head.clone(),
                leave: d.leave.clone(),
            }));
        }
        if ui
            .add(egui::Button::new("Leave them").min_size(egui::vec2(80.0, 30.0)))
            .on_hover_text(format!("Check {target} out; the commits stay on no branch"))
            .clicked()
        {
            outcome = Outcome::Done(Action::Detached(DetachedAct::Leave(d.leave.clone())));
        }
        if cancel_button(ui) {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}
