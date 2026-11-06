//! What the sign-off shows: the commit box's toggle, the note on a commit without one, and the
//! pull request's section.

use egui::{RichText, Ui};

use super::{Dco, DcoAct, PrSignoffs, plan_fix, signed_off_by};
use crate::git::CommitDetail;
use crate::pull_request::PrDetails;
use crate::repo::{Action, short};
use crate::theme::Tokens;

impl Dco {
    /// The commit box's _Sign off_ toggle.
    pub(crate) fn toggle(&self, ui: &mut Ui, enabled: bool, acts: &mut Vec<Action>) {
        let mut on = self.signs_off();
        let tip = match (self.rules.choice, self.reason()) {
            (Some(_), Some(why)) => format!(
                "Adds Signed-off-by with your name and email (DCO). On by itself here, as {}",
                why.text()
            ),
            (Some(_), None) => {
                "Adds Signed-off-by with your name and email (DCO); kept for this repository"
                    .to_owned()
            }
            (None, Some(why)) => format!(
                "Adds Signed-off-by with your name and email (DCO). On by itself: {}",
                why.text()
            ),
            (None, None) => "Adds Signed-off-by with your name and email (DCO)".to_owned(),
        };
        let r = ui
            .add_enabled(enabled, egui::Checkbox::new(&mut on, "Sign off"))
            .on_hover_text(tip);
        if r.changed() {
            acts.push(Action::Dco(DcoAct::SetSignOff(on)));
        }
    }

    /// Under a commit's author: whether it has its author's sign-off, where the repository asks
    /// for one.
    pub(crate) fn commit_note(&self, ui: &mut Ui, detail: &CommitDetail) {
        if !self.required() || detail.parents.len() > 1 {
            return;
        }
        let t = Tokens::get(ui.ctx());
        ui.add_space(6.0);
        if signed_off_by(&detail.message, &detail.email) {
            ui.label(
                RichText::new("✔ Signed off by its author (DCO)")
                    .color(t.success)
                    .small(),
            );
        } else {
            ui.label(
                RichText::new("No Signed-off-by from its author: the DCO check refuses it")
                    .color(t.warning)
                    .small(),
            );
        }
    }
}

/// Whether the pull request's page has a sign-off section: where the repository asks for
/// sign-offs, or its DCO check fails.
pub(crate) fn pr_shows(dco: &Dco, details: &PrDetails) -> bool {
    dco.required() || dco.failing_check == Some(details.number)
}

/// The pull request's sign-off section (see [`pr_shows`]): shown where the repository asks for sign-offs or its DCO
/// check fails, with the commits that have none and the button that signs them off.
/// `current`: the branch checked out here.
pub(crate) fn pr_section(
    ui: &mut Ui,
    dco: &Dco,
    details: &PrDetails,
    current: Option<&str>,
    idle: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let Some(signoffs) = dco.for_pr(details.number, &details.head_oid) else {
        ui.label(
            RichText::new("Reading the commits' sign-offs once the changes are here…")
                .color(t.text_faint),
        );
        return;
    };
    let commits = match &signoffs.commits {
        Ok(commits) => commits,
        Err(e) => {
            ui.colored_label(
                t.warning,
                format!("Couldn't read the commits' sign-offs: {e}"),
            );
            return;
        }
    };
    let missing: Vec<_> = super::signoff::missing(commits).collect();
    if missing.is_empty() {
        let n = commits.iter().filter(|c| !c.merge).count();
        ui.label(
            RichText::new(format!(
                "✔ {} signed off by {} author.",
                if n == 1 {
                    "The commit is".to_owned()
                } else {
                    format!("All {n} commits are")
                },
                if n == 1 { "its" } else { "their" }
            ))
            .color(t.success),
        );
        return;
    }
    ui.colored_label(
        t.warning,
        format!(
            "{} of {} commits have no Signed-off-by from their author:",
            missing.len(),
            commits.iter().filter(|c| !c.merge).count()
        ),
    );
    for c in &missing {
        ui.label(
            RichText::new(format!("{} {} · {}", short(&c.hash), c.summary, c.author))
                .color(t.text_dim)
                .small(),
        );
    }
    ui.add_space(6.0);
    fix_button(ui, signoffs, details, current, idle, acts);
}

/// _Sign off my commits on this branch_, there when the pull request's branch is checked out.
fn fix_button(
    ui: &mut Ui,
    signoffs: &PrSignoffs,
    details: &PrDetails,
    current: Option<&str>,
    idle: bool,
    acts: &mut Vec<Action>,
) {
    let here = current == Some(details.head_branch.as_str());
    let tip = if here {
        "Rewrites the branch's commits with your Signed-off-by (rebase --signoff) and \
         force-pushes them with a lease, after showing what it does. Ctrl+Z undoes it."
            .to_owned()
    } else {
        format!(
            "Check out {} first (or open it in a worktree): the commits signed off are those \
             of the branch checked out",
            details.head_branch
        )
    };
    let r = ui
        .add_enabled(
            here && idle,
            egui::Button::new("Sign off my commits on this branch"),
        )
        .on_hover_text(&tip)
        .on_disabled_hover_text(tip);
    if r.clicked() {
        acts.push(plan_fix(&signoffs.base));
    }
}

/// Whether the commit `oid` of the pull request is one without its author's sign-off.
pub(crate) fn unsigned(signoffs: Option<&PrSignoffs>, oid: &str) -> bool {
    signoffs
        .and_then(|s| s.commits.as_ref().ok())
        .is_some_and(|list| list.iter().any(|c| c.hash == oid && !c.merge && !c.signed))
}
