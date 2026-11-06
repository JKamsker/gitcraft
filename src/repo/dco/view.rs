//! What the sign-off shows: the commit box's toggle, the note on a commit without one, and the
//! pull request's section.

use egui::{RichText, Ui};

use super::{Dco, DcoAct, PrSignoffs, plan_fix, signed_off_by};
use crate::git::CommitDetail;
use crate::pull_request::PrDetails;
use crate::repo::{Action, short};
use crate::theme::Tokens;

const WHAT: &str = "Adds Signed-off-by with your name and email (DCO)";

impl Dco {
    /// The commit box's _Sign off_ toggle.
    pub(crate) fn toggle(&self, ui: &mut Ui, enabled: bool, acts: &mut Vec<Action>) {
        let mut on = self.signs_off();
        let tip = match (self.rules.choice, self.reason()) {
            (Some(true), Some(why)) => format!("{WHAT}. You turned it on here; {} too", why.text()),
            (Some(false), Some(why)) => {
                format!("{WHAT}. You turned it off here, though {}", why.text())
            }
            (Some(true), None) => format!("{WHAT}. You turned it on for this repository"),
            (Some(false), None) => format!("{WHAT}. You turned it off for this repository"),
            (None, Some(why)) => format!("{WHAT}. On by itself: {}", why.text()),
            (None, None) => format!(
                "{WHAT}. Turns on by itself in a repository that asks for it (a DCO file, a \
                 failing DCO check)"
            ),
        };
        let r = ui
            .add_enabled(enabled, egui::Checkbox::new(&mut on, "Sign off"))
            .on_hover_text(tip)
            .on_disabled_hover_text("The commits being replayed keep their own messages");
        if r.changed() {
            acts.push(Action::Dco(DcoAct::SetSignOff(on)));
        }
    }

    /// Under a commit's author: whether it has its author's sign-off, where the repository asks
    /// for one.
    pub(crate) fn commit_note(&self, ui: &mut Ui, detail: &CommitDetail) {
        if !self.required() || detail.parents.len() > 1 || detail.author.ends_with("[bot]") {
            return;
        }
        let t = Tokens::get(ui.ctx());
        ui.add_space(6.0);
        if signed_off_by(&detail.message, &[(&detail.author, &detail.email)]) {
            ui.label(
                RichText::new("✔ Signed off by its author (DCO)")
                    .color(t.success)
                    .small(),
            );
        } else {
            ui.label(
                RichText::new("No Signed-off-by from its author (DCO)")
                    .color(t.warning)
                    .small(),
            )
            .on_hover_text(
                "A DCO check refuses a commit without a sign-off of its author or committer",
            );
        }
    }
}

/// Whether the pull request's page has a sign-off section: where the repository asks for
/// sign-offs, or its DCO check fails.
pub(crate) fn pr_shows(dco: &Dco, details: &PrDetails) -> bool {
    dco.required() || dco.failing_check == Some(details.number)
}

fn commits_count(n: usize) -> String {
    if n == 1 {
        "1 commit".to_owned()
    } else {
        format!("{n} commits")
    }
}

/// The pull request's sign-off section (see [`pr_shows`]), with the commits the DCO check
/// refuses and the button that signs them off. `current`: the branch checked out here.
pub(crate) fn pr_section(
    ui: &mut Ui,
    dco: &Dco,
    details: &PrDetails,
    current: Option<&str>,
    idle: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let signoffs = dco.for_pr(details.number, &details.head_oid);
    let commits = match signoffs.and_then(|s| s.commits.as_ref()) {
        None => {
            ui.label(
                RichText::new("Waiting for the pull request's commits to load…")
                    .color(t.text_faint),
            );
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(
                t.warning,
                format!("Couldn't read the commits' sign-offs: {e}"),
            );
            return;
        }
        Some(Ok(commits)) => commits,
    };
    let checked = commits.iter().filter(|c| !c.merge && !c.bot).count();
    let missing: Vec<_> = super::signoff::missing(commits).collect();
    if checked == 0 {
        ui.label(RichText::new("No commits for the DCO check to look at.").color(t.text_faint));
        return;
    }
    if missing.is_empty() {
        let all = if checked == 1 {
            "The commit is".to_owned()
        } else {
            format!("All {checked} commits are")
        };
        ui.label(RichText::new(format!("✔ {all} signed off.")).color(t.success));
        return;
    }
    ui.colored_label(
        t.warning,
        format!(
            "{} of {} {} no sign-off the DCO check takes (its author's or committer's):",
            missing.len(),
            commits_count(checked),
            if missing.len() == 1 { "has" } else { "have" },
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
    if let Some(signoffs) = signoffs {
        fix_button(ui, signoffs, details, current, idle, acts);
    }
}

/// _Sign off the branch's commits_, there when the pull request's branch is checked out.
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
        "Adds your Signed-off-by to each of the branch's commits (rebase --signoff) and \
         force-pushes them with a lease, after showing what it will do. Ctrl+Z undoes it."
            .to_owned()
    } else {
        format!(
            "Check out {} first (or open it in a worktree): the commits signed off are those \
             of the branch checked out, at the pull request's latest commit",
            details.head_branch
        )
    };
    let r = ui
        .add_enabled(
            here && idle,
            egui::Button::new("Sign off the branch's commits…"),
        )
        .on_hover_text(&tip)
        .on_disabled_hover_text(tip);
    if r.clicked() {
        acts.push(plan_fix(&signoffs.base, &signoffs.head));
    }
}

/// Whether the commit `oid` of the pull request is one the DCO check refuses.
pub(crate) fn unsigned(signoffs: Option<&PrSignoffs>, oid: &str) -> bool {
    signoffs
        .and_then(|s| s.commits.as_ref())
        .and_then(|c| c.as_ref().ok())
        .is_some_and(|list| list.iter().any(|c| c.hash == oid && c.refused()))
}
