//! The dialog against real repositories: painted, clicked, its background reads, the
//! re-check on confirm, and the rename's rollback through the operation executor.

use egui_kittest::{Harness, kittest::Queryable};

use super::*;
use crate::git::fixture::Repo;
use crate::git::{Area, StatusFile};
use crate::repo::operations::run_steps;
use crate::repo::{Action, OpKind, Refresh, Undo};

mod rollback;

type State = (FileMetaForm, bool, Option<Action>);

/// A harness painting `form`; the state's flag is whether the tab is idle.
fn harness(form: FileMetaForm, idle: bool) -> Harness<'static, State> {
    crate::i18n::set_language("en-US");
    let fonts = std::cell::Cell::new(false);
    Harness::new_ui_state(
        move |ui, (form, idle, done): &mut State| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            if let Outcome::Done(action) = form.show(ui, *idle) {
                *done = Some(*action);
            }
            crate::a11y::end_frame(ui.ctx());
        },
        (form, idle, None),
    )
}

/// How long a read of a few Git commands may take on a loaded test machine.
const READ_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// Paints frames until the dialog has no read in flight, returning as soon as it lands.
fn settled(h: &mut Harness<'static, State>) -> Result<(), String> {
    let deadline = std::time::Instant::now() + READ_DEADLINE;
    while std::time::Instant::now() < deadline {
        h.run_steps(1);
        if h.state().0.pending.is_none() {
            h.run_steps(1);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err("the dialog's read never arrived".into())
}

fn click(h: &mut Harness<'static, State>, button: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::Button, button)
        .click();
    h.run_steps(1);
}

#[test]
fn executable_dialog_rereads_on_confirm_and_answers_the_index_edit() -> Result<(), String> {
    let repo = Repo::new("file-metadata-dialog-chmod")?;
    repo.commit("script", &[("run.sh", Some("echo\n"))])?;
    let form = FileMetaForm::new(repo.dir.clone(), "run.sh", Edit::Executable);
    let mut h = harness(form, true);
    settled(&mut h)?;
    // Shown, not written: the patch file only comes with the confirming read.
    assert!(matches!(h.state().0.preview(), Some(Ok(p)) if p.ops[0][2] == patch_text()));
    click(&mut h, "Apply");
    settled(&mut h)?;
    let Some(Action::UndoableOps {
        ops,
        refresh,
        undo: Undo::Custom { undo, redo },
        ..
    }) = h.state_mut().2.take()
    else {
        return Err("Apply must answer an undoable operation".into());
    };
    assert_eq!(refresh, Refresh::Status);
    assert_eq!(redo, ops);
    let [op] = ops.as_slice() else {
        return Err(format!("one step expected: {ops:?}"));
    };
    assert_eq!(&op[..2], ["apply", "--cached"]);
    assert!(std::path::Path::new(&op[2]).is_file(), "{op:?}");
    assert_eq!(undo, [["apply", "--cached", "-R", op[2].as_str()]]);
    run_steps(repo.path(), &ops, OpKind::plain(), None).0?;
    assert!(repo.git(&["ls-files", "-s"])?.starts_with("100755 "));
    Ok(())
}

#[test]
fn a_file_changed_while_the_dialog_was_open_is_shown_again_not_renamed() -> Result<(), String> {
    let repo = Repo::new("file-metadata-dialog-stale")?;
    repo.commit("base", &[("a/Foo.txt", Some("foo\n"))])?;
    let mut form = FileMetaForm::new(repo.dir.clone(), "a/Foo.txt", Edit::LetterCase);
    form.name = "foo.txt".to_owned();
    let mut h = harness(form, true);
    settled(&mut h)?;
    assert!(matches!(h.state().0.preview(), Some(Ok(_))));
    // Something takes the new name in another letter case meanwhile.
    let blob = repo.rev(":a/Foo.txt")?;
    let other = [
        "update-index",
        "--add",
        "--cacheinfo",
        "100644",
        &blob,
        "a/FOO.TXT",
    ];
    repo.git(&other)?;
    click(&mut h, "Rename");
    settled(&mut h)?;
    assert!(h.state().2.is_none(), "a stale rename must not run");
    assert!(h.state().0.changed);
    assert!(matches!(h.state().0.preview(), Some(Err(e)) if e.contains("a/FOO.TXT")));
    h.get_by_label(crate::i18n::text("file-metadata-changed"));
    Ok(())
}

#[test]
fn tracked_file_modes_are_warned_about_and_a_busy_tab_answers_nothing() -> Result<(), String> {
    let repo = Repo::new("file-metadata-dialog-filemode")?;
    repo.commit("script", &[("run.sh", Some("echo\n"))])?;
    repo.git(&["config", "core.fileMode", "true"])?;
    let form = FileMetaForm::new(repo.dir.clone(), "run.sh", Edit::Executable);
    let mut h = harness(form, false);
    settled(&mut h)?;
    h.get_by_label(crate::i18n::text("file-metadata-file-mode-warning"));
    click(&mut h, "Apply");
    assert!(h.state().0.pending.is_none() && h.state().2.is_none());
    Ok(())
}

fn patch_text() -> &'static str {
    crate::i18n::text("file-metadata-patch-placeholder")
}

#[test]
fn menus_offer_edits_only_for_tracked_files() {
    let file = |code, area| StatusFile {
        path: "a".into(),
        old_path: None,
        code,
        area,
    };
    let offered = menu::offered;
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
