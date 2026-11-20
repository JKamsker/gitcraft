//! The painted dialog and its click-to-action path, from a snapshot made by hand: no
//! repository, worker or Git command.

use std::collections::BTreeSet;

use egui_kittest::{Harness, kittest::Queryable};

use super::*;

fn snapshot(path: &str, mode: &str) -> Snapshot {
    Snapshot {
        path: path.to_owned(),
        entry: Some((mode.to_owned(), "b".repeat(40))),
        conflicted: false,
        missing: false,
        siblings: BTreeSet::from(["Run.sh".to_owned(), "other".to_owned()]),
    }
}

/// Paints `form` (idle or busy), clicks `button` and returns what the dialog answered.
fn click(form: FileMetaForm, idle: bool, button: &str) -> Option<Action> {
    crate::i18n::set_language("en-US");
    let fonts = std::cell::Cell::new(false);
    let mut form = form;
    let mut h = Harness::new_ui_state(
        move |ui, done: &mut Option<Action>| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            if let Outcome::Done(action) = form.show(ui, idle) {
                *done = Some(*action);
            }
            crate::a11y::end_frame(ui.ctx());
        },
        None,
    );
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::Button, button)
        .click();
    h.run();
    h.state_mut().take()
}

fn ready(path: &str, mode: &str, edit: Edit) -> FileMetaForm {
    let mut form = FileMetaForm::new(path, edit);
    let patch = (edit == Edit::Executable).then(|| Ok("/tmp/mode.patch".to_owned()));
    form.state = State::Ready(snapshot(path, mode), patch);
    form
}

#[test]
fn executable_dialog_answers_the_undoable_index_edit() -> Result<(), String> {
    let action = click(
        ready("bin/Run.sh", "100644", Edit::Executable),
        true,
        "Apply",
    );
    let Some(Action::UndoableOps {
        ops,
        refresh,
        undo: Undo::Custom { undo, redo },
        ..
    }) = action
    else {
        return Err("Apply must answer an undoable operation".into());
    };
    assert_eq!(ops, [["apply", "--cached", "/tmp/mode.patch"]]);
    assert_eq!(redo, ops);
    assert_eq!(undo, [["apply", "--cached", "-R", "/tmp/mode.patch"]]);
    assert_eq!(refresh, Refresh::Status);
    Ok(())
}

#[test]
fn busy_tab_and_refused_edits_answer_nothing() {
    assert!(click(ready("Run.sh", "100644", Edit::Executable), false, "Apply").is_none());
    assert!(click(ready("Run.sh", "120000", Edit::Executable), true, "Apply").is_none());
    // The name as it is: nothing to rename yet.
    assert!(click(ready("Run.sh", "100644", Edit::LetterCase), true, "Rename").is_none());
}

#[test]
fn case_dialog_answers_the_two_step_rename() -> Result<(), String> {
    let mut form = ready("Run.sh", "100644", Edit::LetterCase);
    form.name = "run.sh".to_owned();
    let Some(Action::UndoableOps { ops, .. }) = click(form, true, "Rename") else {
        return Err("Rename must answer an undoable operation".into());
    };
    let temp = ".Run.sh.gitcraft-case-rename";
    assert_eq!(
        ops,
        [["mv", "--", "Run.sh", temp], ["mv", "--", temp, "run.sh"]]
    );
    Ok(())
}

#[test]
fn menus_offer_edits_only_for_tracked_files() {
    let file = |code, area| StatusFile {
        path: "a".into(),
        old_path: None,
        code,
        area,
    };
    let both = [Edit::Executable, Edit::LetterCase];
    assert_eq!(offered(&file('M', Area::Staged)), both);
    assert_eq!(offered(&file('A', Area::Staged)), both);
    assert_eq!(offered(&file('M', Area::Unstaged)), [Edit::LetterCase]);
    for (code, area) in [
        ('D', Area::Staged),
        ('D', Area::Unstaged),
        ('A', Area::Unstaged),
        ('?', Area::Untracked),
        ('U', Area::Conflicted),
    ] {
        assert!(offered(&file(code, area)).is_empty());
    }
}
