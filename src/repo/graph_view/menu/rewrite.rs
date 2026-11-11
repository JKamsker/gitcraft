//! Rewrite/reset menu construction and guarded reset plans.

use super::*;

pub(in crate::repo) struct Rewrite<'a> {
    /// The tab, which marks the cherry-pick with whether it would conflict.
    pub(in crate::repo) tab: &'a RepoTab,
    /// The commit is a merge (cherry-picking it needs a parent, and isn't checked).
    pub(in crate::repo) merge: bool,
    pub(in crate::repo) hash: &'a str,
    pub(in crate::repo) short: &'a str,
    pub(in crate::repo) summary: &'a str,
    /// The checked-out branch, or `HEAD` when detached.
    pub(in crate::repo) target: &'a str,
    /// HEAD's commit, as the menu saw it.
    pub(in crate::repo) head: &'a str,
    /// The remote-tracking branch `target` is pushed to (`refs/remotes/origin/main`).
    pub(in crate::repo) upstream: Option<&'a str>,
}

/// Entries that rewrite `target` (the checked-out branch) from the commit: interactive rebase,
/// cherry-pick, revert and reset.
pub(super) fn rewrite_items(
    ui: &mut Ui,
    r: &Rewrite<'_>,
    idle: bool,
    is_head: bool,
    acts: &mut Vec<Action>,
) {
    let Rewrite {
        hash,
        short,
        target,
        ..
    } = *r;
    if crate::menu_item::MenuItem::new(crate::i18n::message(
        "repo-core-2652",
        &[("target", target.to_string().into())],
    ))
    .icon("git-merge")
    .enabled(idle)
    .refused(is_head.then_some(crate::i18n::text("repo-core-0796")))
    .show(ui)
    .on_hover_text(crate::i18n::text("repo-core-2653"))
    .clicked()
    {
        acts.push(Action::InteractiveRebase {
            onto: hash.to_owned(),
            label: short.to_owned(),
        });
    }
    let cherry_pick = crate::i18n::text("repo-core-2654").to_owned();
    let picked = if r.merge {
        crate::menu_item::MenuItem::new(cherry_pick)
            .icon("command")
            .enabled(idle)
            .refused(is_head.then_some(crate::i18n::text("repo-core-0796")))
            .show(ui)
    } else {
        r.tab
            .conflict_button(ui, cherry_pick, idle && !is_head, Op::CherryPick, hash)
    };
    if picked.clicked() {
        acts.push(cherry_pick_action(hash));
    }
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2655"))
        .icon("undo-2")
        .enabled(idle)
        .show(ui)
        .clicked()
    {
        acts.push(revert_action(hash, short));
    }
    {
        crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2656",
            &[("target", target.to_string().into())],
        ))
        .icon("undo-2")
        .enabled(idle)
        .refused(is_head.then_some(crate::i18n::text("repo-core-0796")))
        .submenu(ui, |ui| {
            reset_items(ui, r, acts);
        });
    }
}

/// The reset modes, each asking before it moves `target` to the commit.
pub(in crate::repo) fn reset_items(ui: &mut Ui, r: &Rewrite<'_>, acts: &mut Vec<Action>) {
    let Rewrite {
        hash,
        short,
        summary,
        target,
        head,
        upstream,
        ..
    } = *r;
    for (mode, undo_mode, label, explain) in [
        (
            "--soft",
            "--soft",
            "Soft",
            crate::i18n::text("repo-core-0375"),
        ),
        (
            "--mixed",
            "--mixed",
            "Mixed",
            crate::i18n::text("repo-core-0378"),
        ),
        (
            "--keep",
            "--keep",
            "Keep",
            crate::i18n::text("repo-core-2657"),
        ),
        (
            "--hard",
            "--keep",
            "Hard",
            crate::i18n::text("repo-core-0385"),
        ),
    ] {
        let first = explain.split('.').next().unwrap_or(explain);
        let button = crate::menu_item::MenuItem::new(format!(
            "{} — {first}",
            crate::repo::messages::display_operation(label)
        ))
        .icon("command")
        .show(ui);
        if mode == "--hard" {
            offer(&button, || GhostOp::ResetHard {
                to: hash.to_owned(),
            });
        }
        if button.clicked() {
            acts.push(Action::Dialog(
                Dialog::confirm(
                    &format!("{label} reset"),
                    crate::i18n::message(
                        "repo-core-2660",
                        &[
                            ("target", target.to_string().into()),
                            ("short", short.to_string().into()),
                            ("summary", summary.to_string().into()),
                            ("explain", explain.to_string().into()),
                        ],
                    ),
                    &crate::i18n::message(
                        "repo-core-0389",
                        &[("to-lowercase", label.to_lowercase().to_string().into())],
                    ),
                    reset_steps(mode, hash, target, head),
                    None,
                )
                .undo(Undo::HeadMove { mode: undo_mode })
                .preview(
                    git::PreviewAsk::reset(
                        target,
                        head,
                        hash,
                        upstream.map(str::to_owned),
                        mode == "--hard",
                    ),
                    if mode == "--hard" {
                        crate::i18n::message(
                            "repo-core-2661",
                            &[("target", target.to_string().into())],
                        )
                    } else {
                        crate::i18n::message(
                            "repo-core-2662",
                            &[("target", target.to_string().into())],
                        )
                    },
                ),
            ));
        }
    }
}

/// `git reset <mode> <hash>` of the branch `target` (or of a detached `HEAD`), which the dialog
/// asking first named, at `head`: it resets whatever is checked out when it runs, so it refuses
/// once that has moved (another branch checked out, a commit made, in the terminal, while the
/// dialog was open). A hard reset would throw away that branch's uncommitted work. That it is
/// still `target` that is checked out is checked by name: another branch at the same commit
/// resolves the same.
pub(super) fn reset_steps(mode: &str, hash: &str, target: &str, head: &str) -> Vec<Vec<String>> {
    let checked_out = if target == "HEAD" {
        String::new()
    } else {
        format!("refs/heads/{target}")
    };
    let mut steps = vec![
        expect_symref("HEAD", &checked_out),
        expect_ref_at("HEAD", head),
    ];
    steps.extend(ops(&[&["reset", mode, hash]]));
    steps
}
