//! Working-tree and comparison context menu entries.

use super::*;

pub(super) fn wip_menu(resp: &Response, idle: bool, acts: &mut Vec<Action>) {
    crate::menu_item::context(resp)
        .style(crate::theme::menu_style)
        .show(|ui| {
            if MenuItem::new(crate::i18n::text("repo-core-0055"))
                .icon("check")
                .enabled(idle)
                .keys(Command::StageAll)
                .show(ui)
                .clicked()
            {
                acts.push(Action::Ops {
                    label: "Stage all".into(),
                    ops: ops(&[&["add", "-A"]]),
                    refresh: Refresh::Status,
                });
            }
            if MenuItem::new(crate::i18n::text("repo-core-0054"))
                .icon("archive")
                .enabled(idle)
                .keys(Command::Stash)
                .show(ui)
                .clicked()
            {
                acts.push(Action::Dialog(Dialog::stash()));
            }
            ui.separator();
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2630"))
                .icon("file-diff")
                .enabled(idle)
                .show(ui)
                .on_hover_text(crate::i18n::text(APPLY_PATCH_TIP))
                .clicked()
            {
                acts.push(Action::ApplyPatch);
            }
        });
}

/// What "Apply patch…" does.
pub(super) const APPLY_PATCH_TIP: &str = "repo-core-4143";

/// Menu entries comparing `branch` with `current`: what it would bring in (since they
/// diverged, as a pull request shows it), and every difference.
pub(in crate::repo) fn compare_items(
    ui: &mut Ui,
    acts: &mut Vec<Action>,
    branch: BranchRef<'_>,
    current: &str,
) {
    let name = branch.name;
    let current_ref = BranchRef::local(current);
    if crate::menu_item::MenuItem::new(crate::i18n::message(
        "repo-core-4144",
        &[
            ("name", name.to_string().into()),
            ("current", current.to_string().into()),
        ],
    ))
    .icon("command")
    .show(ui)
    .on_hover_text(crate::i18n::text("repo-core-3388"))
    .clicked()
    {
        acts.push(compare_branches_action(current_ref, branch, true));
    }
    if crate::menu_item::MenuItem::new(crate::i18n::message(
        "repo-core-4145",
        &[
            ("current", current.to_string().into()),
            ("name", name.to_string().into()),
        ],
    ))
    .icon("file-diff")
    .show(ui)
    .on_hover_text(crate::i18n::text("repo-core-4146"))
    .clicked()
    {
        acts.push(compare_branches_action(current_ref, branch, false));
    }
}
