//! The right-click menu of a commit row, and of a branch pill on it.

use egui::{Response, Ui};

use super::badge::{BadgeKind, badges};
use super::compare_items;
use crate::forge::links::Target;
use crate::git;
use crate::menu_item::MenuItem;
use crate::repo::actions::{BranchRef, cherry_pick_action, interactive_rebase_action};
use crate::repo::commit_actions::revert_action;
use crate::repo::conflict_check::Op;
use crate::repo::ghost::{GhostOp, offer};
use crate::repo::operations::{expect_ref_at, expect_symref};
use crate::repo::{
    Action, Dialog, RepoTab, Undo, WIP_HASH, fast_forward_action, merge_action, ops, rebase_action,
};

/// Where a row remembers which branch pill was right-clicked, for its menu.
pub(super) fn pill_menu_id(hash: &str) -> egui::Id {
    egui::Id::new(("pill_menu", hash))
}

impl RepoTab {
    pub(super) fn copy_branch(&self, i: usize) -> Option<String> {
        let commit = self.commits.get(i)?;
        badges(&commit.refs, &self.refs)
            .into_iter()
            .find(|b| b.kind == BadgeKind::Branch)
            .map(|b| b.git_ref())
    }

    /// Right-click menu of a commit row.
    pub(super) fn commit_menu(
        &self,
        resp: &Response,
        i: usize,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(commit) = self.commits.get(i) else {
            return;
        };
        // The branch whose pill was right-clicked, if any.
        let pill: Option<String> = resp
            .ctx
            .data(|d| d.get_temp::<Option<String>>(pill_menu_id(&commit.hash)))
            .flatten();
        crate::menu_item::context(resp)
            .style(crate::theme::menu_style)
            .show(|ui| self.commit_menu_items(ui, i, pill, idle, acts));
    }

    /// The entries of a commit's menu; `pill` is the branch whose pill was right-clicked, whose
    /// actions go on top.
    pub(super) fn commit_menu_items(
        &self,
        ui: &mut Ui,
        i: usize,
        pill: Option<String>,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(commit) = self.commits.get(i) else {
            return;
        };
        let hash = commit.hash.clone();
        let short = hash.get(..8).unwrap_or(&hash).to_owned();
        let summary = commit.summary.clone();
        let is_head = hash == self.refs.head;
        // The right-clicked branch, else the commit's first branch, for copying.
        let first_branch = self.copy_branch(i);
        let branch_name = pill.clone().or_else(|| first_branch.clone());
        let target = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        if let Some(branch) = &pill
            && self.refs.current.as_deref() != Some(branch.as_str())
        {
            self.pill_items(ui, branch, &hash, idle, acts);
            ui.separator();
        }
        self.branch_here_items(ui, &hash, &short, idle, is_head, acts);
        ui.separator();
        self.compare_with_items(ui, i, &hash, &short, acts);
        ui.separator();
        self.quick_edit_items(ui, &hash, idle, acts);
        self.fold_item(ui, &hash, idle, acts);
        ui.separator();
        let upstream = self
            .refs
            .current
            .as_deref()
            .and_then(|b| self.upstream_ref(b));
        self.bisect_menu_items(ui, &hash, idle, acts);
        self.parallel_menu_items(ui, &hash, acts);
        self.surgery_items(ui, &hash, idle, acts);
        self.test_menu_items(ui, &hash, acts);
        ui.separator();
        let rewrite = Rewrite {
            tab: self,
            merge: commit.parents.len() > 1,
            hash: &hash,
            short: &short,
            summary: &summary,
            target: &target,
            head: &self.refs.head,
            upstream: upstream.as_deref(),
        };
        rewrite_items(ui, &rewrite, idle, is_head, acts);
        ui.separator();
        let merge = commit.parents.len() > 1;
        if MenuItem::new(crate::i18n::text("repo-core-2627"))
            .icon("file-diff")
            .refused(merge.then_some(crate::i18n::text("repo-core-2628")))
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-2629"))
            .clicked()
        {
            acts.push(Action::SavePatch {
                hash: hash.clone(),
                summary: summary.clone(),
            });
        }
        if is_head
            && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2630"))
                .icon("file-diff")
                .enabled(idle)
                .show(ui)
                .on_hover_text(super::APPLY_PATCH_TIP)
                .clicked()
        {
            acts.push(Action::ApplyPatch);
        }
        ui.separator();
        self.link_items(ui, &hash, pill.as_deref(), branch_name.as_deref(), acts);
        self.gerrit_menu(ui, commit, acts);
        copy_items(
            ui,
            &hash,
            &summary,
            branch_name.as_deref(),
            self.selected == Some(i),
            branch_name == first_branch,
        );
        let scope = self.commit_scope(&hash, branch_name.as_deref());
        crate::repo::custom_commands::menu_items(
            ui,
            crate::custom_commands::Placement::Commit,
            scope,
            acts,
        );
    }

    /// Copy a link to the commit, or to its branch, or open them on the forge; links go to
    /// the remote of the right-clicked branch (`pill`), else of the checked-out one.
    fn link_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        pill: Option<&str>,
        branch: Option<&str>,
        acts: &mut Vec<Action>,
    ) {
        let mut any = false;
        if let Some(menu) = self.link_menu(pill) {
            let target = Target::Commit {
                rev: hash.to_owned(),
            };
            menu.items(ui, "commit", &target, acts);
            any = true;
        }
        if let Some(name) = branch
            && let Some((menu, target)) = self.branch_link_menu(name)
        {
            menu.items(
                ui,
                &crate::i18n::message("repo-core-2631", &[("name", format!("{}", name).into())]),
                &target,
                acts,
            );
            any = true;
        }
        if any {
            ui.separator();
        }
    }

    /// Actions on the right-clicked `branch`: check it out, or bring it into the checked-out
    /// branch.
    fn pill_items(
        &self,
        ui: &mut Ui,
        branch: &str,
        hash: &str,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        // A pill names the local branch when there is one here, else `remote/branch`: a local
        // branch elsewhere that happens to be called `origin/topic` is not this pill's.
        let local = self
            .refs
            .local
            .iter()
            .any(|b| b.name == branch && b.target == hash);
        let git_ref = BranchRef {
            name: branch,
            remote: !local,
        };
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2632",
            &[("branch", format!("{}", branch).into())],
        ))
        .icon("log-in")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(if local {
                Action::Checkout(branch.to_owned())
            } else {
                Action::CheckoutRemote(branch.to_owned())
            });
        }
        let Some(current) = self.refs.current.as_deref() else {
            return;
        };
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2634",
            &[
                ("current", format!("{}", current).into()),
                ("branch", format!("{}", branch).into()),
            ],
        ))
        .icon("arrow-right")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(fast_forward_action(git_ref, current));
        }
        let merge = crate::i18n::message(
            "repo-core-2636",
            &[
                ("branch", format!("{}", branch).into()),
                ("current", format!("{}", current).into()),
            ],
        );
        if self
            .conflict_button(ui, merge, idle, Op::Merge, hash)
            .clicked()
        {
            acts.push(merge_action(git_ref, current));
        }
        let rebase = crate::i18n::message(
            "repo-core-2637",
            &[
                ("current", format!("{}", current).into()),
                ("branch", format!("{}", branch).into()),
            ],
        );
        let rebase = self.conflict_button(ui, rebase, idle, Op::Rebase, hash);
        offer(&rebase, || GhostOp::Rebase {
            onto: git_ref.full(),
            label: branch.to_owned(),
        });
        if rebase.clicked() {
            acts.push(rebase_action(git_ref, current));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2638",
            &[
                ("current", format!("{}", current).into()),
                ("branch", format!("{}", branch).into()),
            ],
        ))
        .icon("git-merge")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(interactive_rebase_action(git_ref));
        }
        crate::repo::rehearse::rehearse_item(ui, acts, git_ref);
        if branch != current {
            crate::repo::interaction::interaction_item(ui, acts, branch);
        }
        ui.separator();
        compare_items(ui, acts, git_ref, current);
    }

    /// A submenu for each local branch pointing at the commit, creating a branch or tag there,
    /// and checking it out detached.
    fn branch_here_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        short: &str,
        idle: bool,
        is_head: bool,
        acts: &mut Vec<Action>,
    ) {
        // Local branches pointing here get their own submenu.
        let here: Vec<&git::Branch> = self
            .refs
            .local
            .iter()
            .filter(|b| b.target == hash)
            .collect();
        for b in &here {
            crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-core-2639",
                &[("name", format!("{}", b.name).into())],
            ))
            .icon("git-branch-plus")
            .submenu(ui, |ui| {
                self.branch_menu_items(ui, acts, b, idle);
            });
        }
        if !here.is_empty() {
            ui.separator();
        }
        // At HEAD, the new-branch key does the same.
        let mut create = MenuItem::new(crate::i18n::text("repo-core-2641")).icon("git-branch-plus");
        if is_head {
            create = create.keys(crate::keymap::Command::NewBranch);
        }
        if create.show(ui).clicked() {
            acts.push(Action::Dialog(Dialog::create_branch(hash, short)));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2642"))
            .icon("tag")
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::tag(
                hash,
                short,
                self.default_remote(),
            )));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-0139"))
            .icon("package")
            .enabled(idle)
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-2643"))
            .clicked()
        {
            acts.push(Action::Release(super::super::release::ReleaseAct::Open(
                hash.to_owned(),
            )));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2644"))
            .icon("folder-git-2")
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-2646"))
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::worktree_at(&self.path, hash)));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2647"))
            .icon("log-in")
            .enabled(idle)
            .refused(is_head.then_some(crate::i18n::text("repo-core-0796")))
            .show(ui)
            .clicked()
        {
            acts.push(Action::Checkout(hash.to_owned()));
        }
    }

    /// Comparing row `i` with the selected row, or with the working tree.
    fn compare_with_items(
        &self,
        ui: &mut Ui,
        i: usize,
        hash: &str,
        short: &str,
        acts: &mut Vec<Action>,
    ) {
        if let Some(compare) = self
            .selected
            .filter(|&s| s != i)
            .and_then(|s| self.compare_rows(s, i))
        {
            let other = self
                .selected
                .and_then(|s| self.commits.get(s))
                .map(|c| {
                    if c.hash == WIP_HASH {
                        crate::i18n::text("repo-core-0926").to_owned()
                    } else {
                        crate::repo::short(&c.hash).to_owned()
                    }
                })
                .unwrap_or_default();
            if crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-core-2648",
                &[("other", format!("{}", other).into())],
            ))
            .icon("file-diff")
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-2649"))
            .clicked()
            {
                acts.push(compare);
            }
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2650"))
            .icon("file-diff")
            .show(ui)
            .clicked()
        {
            acts.push(Action::Compare {
                base: hash.to_owned(),
                base_label: short.to_owned(),
                target: None,
                target_label: crate::i18n::text("repo-core-2651").to_owned(),
                since_fork: false,
            });
        }
    }
}

/// The commit a menu rewrites the checked-out branch from.
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
fn rewrite_items(ui: &mut Ui, r: &Rewrite<'_>, idle: bool, is_head: bool, acts: &mut Vec<Action>) {
    let Rewrite {
        hash,
        short,
        target,
        ..
    } = *r;
    if crate::menu_item::MenuItem::new(crate::i18n::message(
        "repo-core-2652",
        &[("target", format!("{}", target).into())],
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
            &[("target", format!("{}", target).into())],
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
                            ("target", format!("{}", target).into()),
                            ("short", format!("{}", short).into()),
                            ("summary", format!("{}", summary).into()),
                            ("explain", format!("{}", explain).into()),
                        ],
                    ),
                    &crate::i18n::message(
                        "repo-core-0389",
                        &[("to-lowercase", format!("{}", label.to_lowercase()).into())],
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
                            &[("target", format!("{}", target).into())],
                        )
                    } else {
                        crate::i18n::message(
                            "repo-core-2662",
                            &[("target", format!("{}", target).into())],
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
fn reset_steps(mode: &str, hash: &str, target: &str, head: &str) -> Vec<Vec<String>> {
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

/// Copying the commit's hash, message, and `branch` when there is one.
fn copy_items(
    ui: &mut Ui,
    hash: &str,
    summary: &str,
    branch: Option<&str>,
    selected: bool,
    first_branch: bool,
) {
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2663"))
        .icon("copy")
        .keys_when(crate::keymap::Command::GraphCopyHash, selected)
        .show(ui)
        .clicked()
    {
        ui.ctx().copy_text(hash.to_owned());
    }
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2664"))
        .icon("copy")
        .keys_when(crate::keymap::Command::GraphCopyMessage, selected)
        .show(ui)
        .clicked()
    {
        ui.ctx().copy_text(summary.to_owned());
    }
    if let Some(name) = branch
        && crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2665",
            &[("name", format!("{}", name).into())],
        ))
        .icon("copy")
        .keys_when(
            crate::keymap::Command::GraphCopyBranch,
            selected && first_branch,
        )
        .show(ui)
        .clicked()
    {
        ui.ctx().copy_text(name.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use crate::git;
    use crate::repo::OpKind;
    use crate::repo::commit_actions::reset_steps;
    use crate::repo::operations::run_steps;

    /// A hard reset asked for on `main` does nothing once another branch is checked out (in the
    /// terminal, while the dialog was open): it would throw away that one's uncommitted work.
    #[test]
    fn a_reset_refuses_once_another_branch_is_checked_out() {
        let dir = std::env::temp_dir().join(format!("gitcraft-reset-moved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let base = git::head(&dir);
        assert!(git(&["commit", "-q", "--allow-empty", "-m", "two"]).is_ok());
        let seen = git::head(&dir);
        let steps = reset_steps("--hard", &base, "main", &seen);
        // Then, elsewhere: another branch, with a commit and work not committed.
        for args in [
            &["switch", "-q", "-c", "other"][..],
            &["commit", "-q", "--allow-empty", "-m", "other work"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let other = git::head(&dir);
        let _ = std::fs::write(dir.join("wip.txt"), "unsaved\n");
        assert!(git(&["add", "wip.txt"]).is_ok());
        let (refused, _) = run_steps(&dir, &steps, OpKind::plain(), None);
        let kept = (git::head(&dir), dir.join("wip.txt").exists());
        // Back on main as the dialog saw it, the reset goes through.
        let _ = git(&["reset", "-q", "--hard"]);
        let back = git(&["switch", "-q", "main"]);
        let (reset, _) = run_steps(&dir, &steps, OpKind::plain(), None);
        let now = git::head(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(refused.is_err());
        assert_eq!(kept, (other, true));
        assert!(back.is_ok() && reset.is_ok());
        assert_eq!(now, base);
    }

    /// Nor once another branch at the same commit is checked out: it would move that branch.
    /// And one asked for on a detached HEAD refuses once a branch is checked out there.
    #[test]
    fn a_reset_refuses_once_a_branch_at_the_same_commit_is_checked_out() {
        let dir = std::env::temp_dir().join(format!("gitcraft-reset-same-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let base = git::head(&dir);
        assert!(git(&["commit", "-q", "--allow-empty", "-m", "two"]).is_ok());
        let seen = git::head(&dir);
        let steps = reset_steps("--hard", &base, "main", &seen);
        let detached = reset_steps("--hard", &base, "HEAD", &seen);
        // `feature` is where `main` is, and gets checked out while the dialog is open.
        let switched = git(&["switch", "-q", "-c", "feature"]);
        let (refused, _) = run_steps(&dir, &steps, OpKind::plain(), None);
        let (refused_detached, _) = run_steps(&dir, &detached, OpKind::plain(), None);
        let feature = git::resolve(&dir, "refs/heads/feature");
        // Back on main, it goes through; detached, so does the other.
        let back = git(&["switch", "-q", "main"]);
        let (reset, _) = run_steps(&dir, &steps, OpKind::plain(), None);
        let main = git::resolve(&dir, "refs/heads/main");
        let detach = git(&["switch", "-q", "--detach", "feature"]);
        let (reset_detached, _) = run_steps(&dir, &detached, OpKind::plain(), None);
        let (head, feature_after) = (git::head(&dir), git::resolve(&dir, "refs/heads/feature"));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(switched.is_ok() && back.is_ok() && detach.is_ok());
        assert!(refused.is_err());
        assert!(refused_detached.is_err());
        assert_eq!(feature, Ok(seen.clone()));
        assert!(reset.is_ok(), "{reset:?}");
        assert_eq!(main, Ok(base.clone()));
        assert!(reset_detached.is_ok(), "{reset_detached:?}");
        assert_eq!(head, base);
        assert_eq!(feature_after, Ok(seen));
    }

    /// A repository at `dir` with `ünï cödé.txt` (a space and accents in its name) changed
    /// between `base` and `two`, and `other file.txt` left alone. The hashes of both.
    fn keep_repo(name: &str) -> (std::path::PathBuf, String, String) {
        let dir = std::env::temp_dir().join(format!("gitcraft-keep-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            // The files read back as written, whatever Windows' git would convert.
            &["config", "core.autocrlf", "false"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let _ = std::fs::write(dir.join("ünï cödé.txt"), "one\n");
        let _ = std::fs::write(dir.join("other file.txt"), "same\n");
        assert!(git(&["add", "-A"]).is_ok());
        assert!(git(&["commit", "-q", "-m", "base"]).is_ok());
        let base = git::head(&dir);
        let _ = std::fs::write(dir.join("ünï cödé.txt"), "two\n");
        assert!(git(&["commit", "-q", "-am", "two"]).is_ok());
        let two = git::head(&dir);
        (dir, base, two)
    }

    /// `reset --keep` moves the branch and keeps a change to a file the move doesn't touch.
    #[test]
    fn a_keep_reset_moves_the_branch_and_keeps_local_changes() {
        let (dir, base, two) = keep_repo("ok");
        let _ = std::fs::write(dir.join("other file.txt"), "mine, not committed\n");
        let steps = reset_steps("--keep", &base, "main", &two);
        let (result, _) = run_steps(&dir, &steps, OpKind::plain(), None);
        let head = git::head(&dir);
        let kept = std::fs::read_to_string(dir.join("other file.txt")).unwrap_or_default();
        let moved = std::fs::read_to_string(dir.join("ünï cödé.txt")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(head, base);
        assert_eq!(kept, "mine, not committed\n");
        assert_eq!(moved, "one\n");
    }

    /// When the move would touch a file with changes, it stops with git's own message and
    /// changes nothing: the branch stays, the change stays.
    #[test]
    fn a_keep_reset_refuses_to_touch_a_changed_file() {
        let (dir, base, two) = keep_repo("refused");
        let _ = std::fs::write(dir.join("ünï cödé.txt"), "mine, not committed\n");
        let steps = reset_steps("--keep", &base, "main", &two);
        let (result, _) = run_steps(&dir, &steps, OpKind::plain(), None);
        let head = git::head(&dir);
        let kept = std::fs::read_to_string(dir.join("ünï cödé.txt")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        let err = result.err().unwrap_or_default();
        assert!(
            err.contains("ünï cödé.txt") || err.contains("not uptodate"),
            "{err}"
        );
        assert_eq!(head, two);
        assert_eq!(kept, "mine, not committed\n");
    }
}

#[cfg(test)]
mod registry_menu_tests {
    use super::*;
    use crate::keymap::{Command, Stroke};
    use egui::{Key, Modifiers};
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn graph_copy_menu_hints_match_selected_commit_and_first_branch_only() {
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (bool, bool, bool)| {
                if !std::mem::replace(&mut state.0, true) {
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = crate::settings::Settings::default();
                    for (command, key) in [
                        (Command::GraphCopyHash, Key::F8),
                        (Command::GraphCopyMessage, Key::F9),
                        (Command::GraphCopyBranch, Key::F10),
                    ] {
                        settings
                            .key_bindings
                            .insert(command.id().into(), vec![Stroke::new(key, Modifiers::NONE)]);
                    }
                    settings.put_for_test(ui.ctx());
                }
                copy_items(ui, "recorded", "message", Some("main"), state.1, state.2);
            },
            (false, true, true),
        );
        h.run();
        h.get_by_label("Copy commit hash F8");
        h.get_by_label("Copy message F9");
        h.get_by_label("Copy branch name (main) F10");
        h.state_mut().2 = false;
        h.run();
        h.get_by_label("Copy branch name (main)");
        h.get_by_label("Copy commit hash F8");
        h.state_mut().1 = false;
        h.run();
        h.get_by_label("Copy commit hash");
        h.get_by_label("Copy message");
    }
}
