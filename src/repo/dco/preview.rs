//! Sign-off rewrite confirmation and preview text.

use super::*;

/// What the sign-off of `plan` is called: the operation's label, and the dialog's title.
pub(super) fn fix_label(plan: &Plan) -> String {
    format!("Sign off the commits of {}", plan.branch)
}

/// The dialog that shows what signing off the branch does, and does it.
pub(super) fn fix_dialog(plan: &Plan, who: &str, email: &str) -> Dialog {
    let button = if plan.push.is_some() {
        crate::i18n::text("repo-workflows-dco-fix-dialog-1a8aeebd")
    } else {
        crate::i18n::text("repo-workflows-dco-fix-dialog-6ce6ab1f")
    };
    let mut local = plan.clone();
    local.push = None;
    Dialog::confirm(
        &fix_label(plan),
        fix_preview(plan, who, email),
        button,
        local.steps(),
        None,
    )
    .undo(Undo::HeadMove { mode: "--keep" })
    .refresh(Refresh::All)
}

/// What [`fix_dialog`] says it will do.
pub(crate) fn fix_preview(plan: &Plan, who: &str, email: &str) -> String {
    let unsigned = plan.unsigned();
    let commits = unsigned
        .iter()
        .take(8)
        .map(|c| format!("  • {} {}\n", super::super::short(&c.hash), c.summary))
        .collect::<String>();
    let others = plan.by_others(email);
    let mut names: Vec<&str> = Vec::new();
    for c in &others {
        if !names.contains(&c.author.as_str()) {
            names.push(&c.author);
        }
    }
    crate::i18n::message(
        "repo-workflows-dco-whole-preview",
        &[
            ("count", (plan.commits.len()).into()),
            ("branch", (plan.branch.as_str()).into()),
            ("base", (super::super::short(&plan.base)).into()),
            ("who", (who).into()),
            ("unsigned", (unsigned.len()).into()),
            ("commits", (commits).into()),
            ("more", (unsigned.len().saturating_sub(8)).into()),
            ("others", (others.len()).into()),
            ("authors", (names.join(", ")).into()),
            (
                "push",
                (if plan.push.is_some() { "yes" } else { "no" }).into(),
            ),
            (
                "remote",
                (plan
                    .push
                    .as_ref()
                    .map(signoff::PushTarget::label)
                    .unwrap_or_default())
                .into(),
            ),
            (
                "seen",
                (if plan.push.as_ref().is_some_and(|p| p.seen.is_some()) {
                    "yes"
                } else {
                    "no"
                })
                .into(),
            ),
        ],
    )
}

/// The action that asks to sign off the checked-out branch's commits since `base`, while it is
/// at `head`.
pub(crate) fn plan_fix(base: &str, head: &str) -> Action {
    Action::Dco(DcoAct::PlanFix {
        base: base.to_owned(),
        head: head.to_owned(),
    })
}
