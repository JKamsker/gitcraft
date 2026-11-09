//! Paint production dialogs and record returned intents without dispatching them.
use super::*;
use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};

struct Recording<S> {
    form: S,
    fonts: bool,
    actions: usize,
}

fn harness<S: 'static>(
    form: S,
    mut paint: impl FnMut(&mut egui::Ui, &mut S) -> Outcome + 'static,
) -> Harness<'static, Recording<S>> {
    crate::i18n::set_language("en-US");
    let mut h = Harness::new_ui_state(
        move |ui, state: &mut Recording<S>| {
            if !state.fonts {
                state.fonts = true;
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            if matches!(paint(ui, &mut state.form), Outcome::Done(_)) {
                state.actions += 1;
            }
        },
        Recording {
            form,
            fonts: false,
            actions: 0,
        },
    );
    h.run_steps(3);
    h
}

fn press_enter<S>(h: &mut Harness<'_, Recording<S>>) {
    h.key_press(Key::Enter);
    h.run_steps(3);
}

#[test]
fn rehearsal_checkbox_does_not_start_but_command_and_primary_button_do() {
    let mut h = harness((true, false, "cargo test".into(), false), |ui, form| {
        rehearse::rehearse(
            ui,
            "inert machine",
            Some("main"),
            "topic",
            false,
            &mut form.0,
            &mut form.1,
            &mut form.2,
            &mut form.3,
        )
    });
    h.get_by_role_and_label(Role::CheckBox, "Rehearse the merge too, to compare")
        .focus();
    press_enter(&mut h);
    assert!(h.state().form.1);
    assert_eq!(h.state().actions, 0);
    h.state_mut().form.3 = true;
    h.run_steps(2);
    press_enter(&mut h);
    assert_eq!(h.state().actions, 1);
    h.get_by_role_and_label(Role::Button, "Rehearse").focus();
    press_enter(&mut h);
    assert_eq!(h.state().actions, 2);
}

#[test]
fn test_lane_checkboxes_do_not_start_but_command_and_primary_button_do() {
    let start = super::super::test_lane::StartTests {
        range: "main..topic".into(),
        command: "cargo test".into(),
        workers: 1,
        retest: false,
        forget: false,
    };
    let mut h = harness((start, false), |ui, form| {
        test_commits::test_commits(
            ui,
            "inert machine",
            false,
            &mut form.0,
            Some("old-command"),
            &mut form.1,
        )
    });
    for label in [
        "Test again commits that have a result",
        "Forget the results of the old command (`old-command`); this can't be undone",
    ] {
        h.get_by_role_and_label(Role::CheckBox, label).focus();
        press_enter(&mut h);
        assert_eq!(h.state().actions, 0);
    }
    assert!(h.state().form.0.retest && h.state().form.0.forget);
    h.state_mut().form.1 = true;
    h.run_steps(2);
    press_enter(&mut h);
    assert_eq!(h.state().actions, 1);
    h.get_by_role_and_label(Role::Button, "Test").focus();
    press_enter(&mut h);
    assert_eq!(h.state().actions, 2);
}

#[test]
fn stash_scope_checkbox_does_not_submit_but_message_and_primary_button_do() {
    use crate::git::{Area, StatusFile, WorkStatus};
    for choice in [stash::Choice::Everything, stash::Choice::Snapshot] {
        for untracked in [false, true] {
            let mut dialog = stash::StashDialog::new(choice, Vec::new());
            dialog.untracked = untracked;
            dialog.focus = false;
            let work = WorkStatus {
                unstaged: vec![
                    StatusFile {
                        path: "tracked".into(),
                        old_path: None,
                        code: 'M',
                        area: Area::Unstaged,
                    },
                    StatusFile {
                        path: "untracked".into(),
                        old_path: None,
                        code: '?',
                        area: Area::Untracked,
                    },
                ],
                ..Default::default()
            };
            let mut h = harness((dialog, work), |ui, form| {
                stash::stash(ui, &mut form.0, &form.1, &StagedSupport::Unknown)
            });
            h.get_by_role_and_label(Role::CheckBox, "Include untracked files")
                .focus();
            press_enter(&mut h);
            assert_eq!(h.state().form.0.untracked, !untracked);
            assert_eq!(h.state().actions, 0);
            h.state_mut().form.0.focus = true;
            h.run_steps(2);
            press_enter(&mut h);
            assert_eq!(h.state().actions, 1);
            let label = if choice == stash::Choice::Snapshot {
                "Save snapshot"
            } else {
                "Stash"
            };
            h.get_by_role_and_label(Role::Button, label).focus();
            press_enter(&mut h);
            assert_eq!(h.state().actions, 2);
        }
    }
}

#[test]
fn fold_checkbox_never_submits_in_either_pushed_state_or_toggle_direction() {
    for pushed in [false, true] {
        for keep_fixup in [false, true] {
            let target = crate::git::RebaseCommit {
                hash: "a".repeat(40),
                summary: "inert target".into(),
                ..Default::default()
            };
            let plan = super::super::fold::Plan {
                pushed: if pushed {
                    vec![target.hash.clone()]
                } else {
                    Vec::new()
                },
                target,
                after: Vec::new(),
                root: false,
                head: "b".repeat(40),
                branch: "main".into(),
                staged: 1,
                keep_fixup,
            };
            let mut h = harness(plan, fold::fold);
            h.get_by_role_and_label(Role::CheckBox, "Keep as fixup! commit for later")
                .focus();
            press_enter(&mut h);
            assert_eq!(h.state().form.keep_fixup, !keep_fixup);
            assert_eq!(h.state().actions, 0);
            let label = crate::i18n::text(if h.state().form.keep_fixup {
                "repo-core-1314"
            } else {
                "repo-core-1315"
            });
            h.get_by_role_and_label(Role::Button, label).focus();
            press_enter(&mut h);
            assert_eq!(h.state().actions, 1);
        }
    }
}
