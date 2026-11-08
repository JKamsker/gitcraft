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
            (Some(true), Some(why)) => crate::i18n::message(
                "repo-workflows-dco-toggle-0",
                &[("reason", (why.text()).into())],
            ),
            (Some(false), Some(why)) => crate::i18n::message(
                "repo-workflows-dco-toggle-1",
                &[("reason", (why.text()).into())],
            ),
            (Some(true), None) => crate::i18n::message("repo-workflows-dco-toggle-2", &[]),
            (Some(false), None) => crate::i18n::message("repo-workflows-dco-toggle-3", &[]),
            (None, Some(why)) => crate::i18n::message(
                "repo-workflows-dco-toggle-4",
                &[("reason", (why.text()).into())],
            ),
            (None, None) => crate::i18n::message("repo-workflows-dco-toggle-5", &[]),
        };
        let r = ui
            .add_enabled(
                enabled,
                egui::Checkbox::new(
                    &mut on,
                    crate::i18n::text("repo-workflows-dco-toggle-6ce6ab1f"),
                ),
            )
            .on_hover_text(tip)
            .on_disabled_hover_text(crate::i18n::text("repo-workflows-dco-toggle-9ba5d60e"));
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
                RichText::new(crate::i18n::text("repo-workflows-dco-commit-note-fccbd96e"))
                    .color(t.success)
                    .small(),
            );
        } else {
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-dco-commit-note-c87e4ef3"))
                    .color(t.warning)
                    .small(),
            )
            .on_hover_text(crate::i18n::text("repo-workflows-dco-commit-note-72e49d72"));
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
        crate::i18n::text("repo-workflows-dco-commits-count-acfd7b9e").to_owned()
    } else {
        crate::i18n::message(
            "repo-workflows-dco-commits-count-4eb116dc",
            &[("n", (n).clone().into())],
        )
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
                RichText::new(crate::i18n::text("repo-workflows-dco-pr-section-9ecdacc9"))
                    .color(t.text_faint),
            );
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(
                t.warning,
                crate::i18n::message(
                    "repo-workflows-dco-pr-section-e93d65c8",
                    &[("e", (e).clone().into())],
                ),
            );
            return;
        }
        Some(Ok(commits)) => commits,
    };
    let checked = commits.iter().filter(|c| !c.merge && !c.bot).count();
    let missing: Vec<_> = super::signoff::missing(commits).collect();
    if checked == 0 {
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-dco-pr-section-15029601"))
                .color(t.text_faint),
        );
        return;
    }
    if missing.is_empty() {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-dco-all-signed",
                &[("count", (checked).into())],
            ))
            .color(t.success),
        );
        return;
    }
    ui.colored_label(
        t.warning,
        crate::i18n::message(
            "repo-workflows-dco-missing-signoffs",
            &[
                ("missing", (missing.len()).into()),
                ("total", (checked).into()),
            ],
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
        crate::i18n::text("repo-workflows-dco-fix-button-04f5829a").to_owned()
    } else {
        crate::i18n::message(
            "repo-workflows-dco-fix-button-6d181602",
            &[(
                "details-head-branch-1",
                (details.head_branch).clone().into(),
            )],
        )
    };
    let r = ui
        .add_enabled(
            here && idle,
            egui::Button::new(crate::i18n::text("repo-workflows-dco-fix-button-344ac70e")),
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
