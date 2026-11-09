//! Dialogs that create or rename a branch, or create a tag.

use egui::Ui;

use crate::repo::operations::expect_ref_at;
use crate::repo::{Action, Refresh, Undo};
use crate::theme::Tokens;

use super::{Outcome, buttons, dash_spaces, field, title, usable};

/// A new branch at `at`, checked out or not. `previous` is what its undo checks out again.
#[allow(clippy::too_many_arguments)]
pub(super) fn create_branch(
    ui: &mut Ui,
    previous: &str,
    at: &str,
    at_label: &str,
    name: &mut String,
    checkout: &mut bool,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-0140"),
        &crate::i18n::message(
            "repo-core-1213",
            &[("at-label", at_label.to_string().into())],
        ),
        &t,
    );
    let enter = crate::a11y::singleline_enter(&field(ui, name, "feature/my-branch", focus));
    dash_spaces(name);
    ui.add_space(6.0);
    ui.checkbox(checkout, crate::i18n::text("repo-core-1215"));
    ui.add_space(12.0);
    let valid = usable(name);
    buttons(
        ui,
        crate::i18n::text("repo-core-1216"),
        valid,
        enter,
        || {
            let name = name.trim().to_owned();
            let args: Vec<String> = if *checkout {
                vec!["switch".into(), "-c".into(), name.clone(), at.to_owned()]
            } else {
                vec!["branch".into(), name.clone(), at.to_owned()]
            };
            Action::UndoableOps {
                label: format!("Create branch {name}"),
                ops: vec![args.clone()],
                refresh: Refresh::All,
                undo: Undo::Custom {
                    undo: create_branch_undo(&name, at, *checkout, previous),
                    redo: vec![args],
                },
            }
        },
    )
}

/// How creating the branch `name` at the commit `at` (and checking it out, from `previous`) is
/// undone: back to `previous`, and the branch deleted while it is still at `at`. Commits made on
/// it since (in the terminal: undoing one made here comes first) are on no other branch, and
/// `branch -D` would throw them away.
pub(in crate::repo) fn create_branch_undo(
    name: &str,
    at: &str,
    checkout: bool,
    previous: &str,
) -> Vec<Vec<String>> {
    // Checked first, so that a refused undo hasn't switched the checkout already.
    let mut undo = vec![expect_ref_at(&format!("refs/heads/{name}"), at)];
    if checkout && !previous.is_empty() {
        // `--`: `previous` is a branch or a commit, never a file of that name to check out.
        undo.push(vec![
            "checkout".to_owned(),
            previous.to_owned(),
            "--".to_owned(),
        ]);
    }
    // `branch -D` rather than deleting the ref: its upstream and other settings go too (a
    // branch pushed meanwhile would otherwise track that again once made anew under the name),
    // and git refuses while a worktree has it checked out. The check above keeps its commits.
    undo.push(vec![
        "branch".to_owned(),
        "-D".to_owned(),
        "--".to_owned(),
        name.to_owned(),
    ]);
    undo
}

/// A tag at `at`, annotated when it has a message, and pushed to `remote` when asked.
#[allow(clippy::too_many_arguments)]
pub(super) fn tag(
    ui: &mut Ui,
    at: &str,
    at_label: &str,
    name: &mut String,
    message: &mut String,
    remote: &Option<String>,
    push: &mut bool,
    release: &mut bool,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-0141"),
        &crate::i18n::message(
            "repo-core-1220",
            &[("at-label", at_label.to_string().into())],
        ),
        &t,
    );
    let enter = crate::a11y::singleline_enter(&field(ui, name, "v1.2.0", focus));
    dash_spaces(name);
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::multiline(message)
            .hint_text(crate::i18n::text("repo-core-1222"))
            .desired_width(f32::INFINITY)
            .desired_rows(2)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    if let Some(remote) = remote {
        ui.add_space(4.0);
        ui.checkbox(
            push,
            crate::i18n::message("repo-core-1223", &[("remote", remote.to_string().into())]),
        );
    }
    ui.add_space(8.0);
    ui.checkbox(release, crate::i18n::text("repo-core-1224"));
    let valid = usable(name);
    buttons(
        ui,
        crate::i18n::text("repo-core-1216"),
        valid,
        enter,
        || {
            let name = name.trim().to_owned();
            let mut create = vec!["tag".to_owned()];
            if !message.trim().is_empty() {
                create.extend(["-a".to_owned(), "-m".to_owned(), message.trim().to_owned()]);
            }
            create.extend([name.clone(), at.to_owned()]);
            let mut steps = vec![create.clone()];
            if *push && let Some(remote) = remote {
                steps.push(vec![
                    "push".to_owned(),
                    remote.clone(),
                    format!("refs/tags/{name}"),
                ]);
            }
            Action::UndoableOps {
                label: if *release {
                    format!("Create release tag {name}")
                } else {
                    format!("Create tag {name}")
                },
                ops: steps,
                refresh: Refresh::All,
                undo: Undo::Custom {
                    undo: vec![vec!["tag".to_owned(), "-d".to_owned(), name.clone()]],
                    redo: vec![create],
                },
            }
        },
    )
}

/// `children`: the layers of stacks based on the branch, which follow it to its new name.
pub(super) fn rename_branch(
    ui: &mut Ui,
    enter: bool,
    old: &str,
    new: &mut String,
    focus: &mut bool,
    children: &[String],
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-1229"),
        &crate::i18n::message("repo-core-1230", &[("old", old.to_string().into())]),
        &t,
    );
    field(ui, new, "new-name", focus);
    dash_spaces(new);
    ui.add_space(8.0);
    let valid = usable(new) && new.trim() != old;
    buttons(
        ui,
        crate::i18n::text("repo-core-1232"),
        valid,
        enter,
        || {
            let new = new.trim().to_owned();
            let old = old.to_owned();
            let rename = |from: &str, to: &str| {
                let mut steps = vec![vec!["branch".into(), "-m".into(), from.into(), to.into()]];
                steps.extend(crate::git::stack::reparent_ops(children, to));
                steps
            };
            Action::UndoableOps {
                label: format!("Rename {old} to {new}"),
                ops: rename(&old, &new),
                refresh: Refresh::All,
                undo: Undo::Custom {
                    undo: rename(&new, &old),
                    redo: rename(&old, &new),
                },
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;
    use egui_kittest::{Harness, kittest::Queryable};

    struct InertForm {
        tag: bool,
        name: String,
        message: String,
        checkout: bool,
        push: bool,
        release: bool,
        focus: bool,
        fonts: bool,
        actions: usize,
        operations: Vec<Vec<Vec<String>>>,
    }

    fn inert_form(tag_form: bool) -> Harness<'static, InertForm> {
        crate::i18n::set_language("en-US");
        let mut h = Harness::new_ui_state(
            |ui, form: &mut InertForm| {
                if !form.fonts {
                    form.fonts = true;
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let outcome = if form.tag {
                    tag(
                        ui,
                        "inert-commit",
                        "main",
                        &mut form.name,
                        &mut form.message,
                        &Some("origin".into()),
                        &mut form.push,
                        &mut form.release,
                        &mut form.focus,
                    )
                } else {
                    create_branch(
                        ui,
                        "main",
                        "inert-commit",
                        "main",
                        &mut form.name,
                        &mut form.checkout,
                        &mut form.focus,
                    )
                };
                // Construct and record intents without dispatching any Git operation.
                if let Outcome::Done(action) = outcome {
                    form.actions += 1;
                    if let Action::UndoableOps { ops, .. } = *action {
                        form.operations.push(ops);
                    }
                }
            },
            InertForm {
                tag: tag_form,
                name: "topic".into(),
                message: String::new(),
                checkout: true,
                push: false,
                release: false,
                focus: false,
                fonts: false,
                actions: 0,
                operations: Vec::new(),
            },
        );
        h.run_steps(3);
        h
    }

    #[test]
    fn branch_checkbox_enter_toggles_without_creating_and_name_enter_still_creates() {
        let mut h = inert_form(false);
        h.get_by_label("Check it out").focus();
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(!h.state().checkout);
        assert_eq!(h.state().actions, 0);
        h.state_mut().focus = true;
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().actions, 1);
    }

    #[test]
    fn tag_checkbox_enter_toggles_without_creating_and_name_enter_still_creates() {
        let mut h = inert_form(true);
        h.state_mut().message = "summary\nbody".into();
        h.run_steps(2);
        for label in ["Push it to origin", "This is a release tag"] {
            h.get_by_label(label).focus();
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().actions, 0, "{label}");
        }
        assert!(h.state().push && h.state().release);
        h.state_mut().focus = true;
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().actions, 1);
    }

    #[test]
    fn tag_name_and_create_button_accept_multiline_annotation() {
        for primary_button in [false, true] {
            let mut h = inert_form(true);
            h.state_mut().message = "summary\nbody".into();
            h.run_steps(2);
            if primary_button {
                h.get_by_label("Create").focus();
            } else {
                h.get_by_role(egui::accesskit::Role::TextInput).focus();
            }
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().actions, 1);
            assert_eq!(
                h.state().operations,
                vec![vec![vec![
                    "tag",
                    "-a",
                    "-m",
                    "summary\nbody",
                    "topic",
                    "inert-commit",
                ]]],
            );
        }
    }

    #[test]
    fn tag_annotation_enter_inserts_newline_without_submitting() {
        for message in ["summary", "summary\nbody"] {
            let mut h = inert_form(true);
            h.state_mut().message = message.into();
            h.run_steps(2);
            h.get_by_role(egui::accesskit::Role::MultilineTextInput)
                .focus();
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().actions, 0);
            assert!(h.state().operations.is_empty());
            assert_eq!(
                h.state().message.matches('\n').count(),
                message.matches('\n').count() + 1,
            );
            assert!(
                h.get_by_role(egui::accesskit::Role::MultilineTextInput)
                    .is_focused()
            );
        }
    }

    #[test]
    fn explicit_create_button_enter_creates() {
        let mut h = inert_form(false);
        h.get_by_label("Create").focus();
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().actions, 1);
    }

    /// Undoing a branch's creation deletes it while it is where it was made, and not once
    /// commits were made on it since.
    #[test]
    fn undoing_a_branch_keeps_commits_made_on_it() {
        let dir = std::env::temp_dir().join(format!("gitcraft-branch-undo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
        ] {
            assert!(git(args).is_ok());
        }
        let at = git::head(&dir);
        let undo_all = |undo: &[Vec<String>]| {
            crate::repo::operations::run_steps(&dir, undo, crate::repo::OpKind::plain(), None)
                .0
                .is_ok()
        };
        let exists = || git(&["rev-parse", "--verify", "-q", "refs/heads/feat"]).is_ok();
        // Untouched since: undone.
        assert!(git(&["switch", "-q", "-c", "feat", &at]).is_ok());
        let undone = undo_all(&create_branch_undo("feat", &at, true, "main"));
        let gone = !exists();
        // A commit on it since (in the terminal): it stays, with the commit.
        assert!(git(&["switch", "-q", "-c", "feat", &at]).is_ok());
        assert!(git(&["commit", "-q", "--allow-empty", "-m", "work"]).is_ok());
        let refused = !undo_all(&create_branch_undo("feat", &at, true, "main"));
        let kept = exists();
        // Refused before anything ran: still on the branch.
        let on_feat = git(&["symbolic-ref", "--short", "HEAD"]).map(|h| h.trim().to_owned());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(undone && gone);
        assert!(refused && kept);
        assert_eq!(on_feat.as_deref(), Ok("feat"));
    }
}
