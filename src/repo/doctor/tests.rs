//! The doctor in a tab of a real repository: what it reads, drawing it, and its changes going
//! through the tab's operations.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{DoctorAct, Section};
use crate::git::doctor::tests::{git, write};
use crate::git::{self, doctor::flags::Flagged};
use crate::repo::{Action, RepoTab};

/// A repository with a few files, one assume-unchanged and one skip-worktree.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = git::doctor::tests::repo(
        &format!("tab-{name}"),
        &[
            ("a.txt", b"x\n"),
            ("sub dir/ü b.txt", b"x\n"),
            ("c.txt", b"x\n"),
        ],
    )?;
    git(&dir, &["config", "pull.rebase", "merges"])?;
    git(&dir, &["update-index", "--assume-unchanged", "--", "a.txt"])?;
    git(
        &dir,
        &["update-index", "--skip-worktree", "--", "sub dir/ü b.txt"],
    )?;
    Ok(dir)
}

/// Takes in background work until `done` holds, drawing the doctor each time.
fn wait(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    let start = Instant::now();
    while !done(tab) {
        if start.elapsed() > Duration::from_secs(20) {
            return Err("timed out waiting for the doctor".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(ctx);
        draw(tab, ctx);
    }
    Ok(())
}

/// One frame of the doctor's page; what it asks for is carried out.
fn draw(tab: &mut RepoTab, ctx: &egui::Context) {
    let mut acts = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 800.0),
        )),
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.doctor_ui(ui, &mut acts));
    })
    .drop_without_applying_deltas();
    for act in acts {
        tab.apply(act, ctx);
    }
}

fn flagged(tab: &RepoTab) -> Vec<Flagged> {
    tab.doctor
        .as_ref()
        .and_then(|d| d.flags.list.as_ref())
        .and_then(|r| r.as_ref().ok())
        .map(|data| data.manual.clone())
        .unwrap_or_default()
}

#[test]
fn the_doctor_reads_settings_and_clears_flags_through_the_tab() -> Result<(), String> {
    let dir = repo("flags")?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let result = (|| {
        tab.apply(Action::OpenDoctor(Section::Config), &ctx);
        wait(&mut tab, &ctx, |t| {
            t.doctor
                .as_ref()
                .is_some_and(|d| d.config.origins.is_some())
        })?;
        let origins = tab
            .doctor
            .as_ref()
            .and_then(|d| d.config.origins.clone())
            .ok_or("not read")??
            .origins;
        let rebase = git::doctor::config::values_of(&origins, "pull.rebase");
        assert_eq!(rebase.last().map(|o| o.shown_value()), Some("merges"));
        // The setting opened draws its details.
        if let Some(d) = tab.doctor.as_mut() {
            d.config.open = Some("pull.rebase");
        }
        draw(&mut tab, &ctx);

        tab.apply(Action::Doctor(DoctorAct::Show(Section::Flags)), &ctx);
        wait(&mut tab, &ctx, |t| {
            t.doctor.as_ref().is_some_and(|d| d.flags.list.is_some())
        })?;
        let list = flagged(&tab);
        assert_eq!(list.len(), 2, "{list:?}");
        // The change shows under Changes only once the flag is cleared.
        write(&dir.join("a.txt"), b"changed\n")?;
        // Asked first: nothing runs until the answer, and a no runs nothing.
        let ask = || {
            Action::Doctor(DoctorAct::Confirm(super::Confirm {
                title: "Clear?".into(),
                body: String::new(),
                button: "Clear".into(),
                label: "Clear".into(),
                ops: git::doctor::flags::clear_ops(&list),
            }))
        };
        tab.apply(ask(), &ctx);
        draw(&mut tab, &ctx);
        assert!(tab.busy_op.is_none());
        tab.apply(Action::Doctor(DoctorAct::Answer(false)), &ctx);
        assert!(tab.busy_op.is_none());
        assert!(tab.doctor.as_ref().is_some_and(|d| d.confirm.is_none()));
        tab.apply(ask(), &ctx);
        tab.apply(Action::Doctor(DoctorAct::Answer(true)), &ctx);
        assert!(tab.doctor.as_ref().is_some_and(|d| d.waiting));
        // Read again once the operation is done.
        wait(&mut tab, &ctx, |t| {
            t.busy_op.is_none()
                && t.doctor
                    .as_ref()
                    .is_some_and(|d| !d.waiting && d.flags.list.is_some())
        })?;
        let after = flagged(&tab);
        assert!(after.is_empty(), "{after:?}");
        assert!(git(&dir, &["status", "--porcelain"])?.contains("a.txt"));
        // Escape's action closes it.
        tab.apply(Action::CloseDoctor, &ctx);
        assert!(tab.doctor.is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn an_answer_for_an_older_read_is_dropped() -> Result<(), String> {
    let dir = repo("older")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(Action::OpenDoctor(Section::Flags), &ctx);
    // Read again before the first answer is taken in: only the latest one counts.
    tab.apply(Action::Doctor(DoctorAct::Show(Section::Config)), &ctx);
    let start = Instant::now();
    let mut timed_out = false;
    while tab
        .doctor
        .as_ref()
        .is_some_and(|d| d.config.origins.is_none())
    {
        if start.elapsed() > Duration::from_secs(20) {
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
        tab.poll(&ctx);
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!timed_out);
    assert!(tab.doctor.as_ref().is_some_and(|d| d.flags.list.is_none()));
    Ok(())
}

#[test]
fn the_doctor_waits_for_an_open_editor() -> Result<(), String> {
    let dir = repo("refused")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.rebase = Some(crate::repo::rebase::RebaseEditor::new(
        "x".into(),
        "x".into(),
        Vec::new(),
    ));
    tab.apply(Action::OpenDoctor(Section::Config), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.doctor.is_none());
    assert!(tab.status.as_ref().is_some_and(|(_, err)| *err));
    Ok(())
}

#[test]
fn a_file_is_explained_and_its_rules_edited_through_the_tab() -> Result<(), String> {
    use super::IgnoreAct;
    let dir = git::doctor::tests::repo(
        "tab-ignore",
        &[
            (".gitignore", b"*.log\n"),
            (".gitattributes", b"*.bin binary\n"),
        ],
    )?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let result = (|| {
        write(&dir.join("a b.log"), b"x\n")?;
        write(&dir.join("x.tmp"), b"x\n")?;
        // From a file's menu: the section opens on its own.
        tab.apply(
            Action::Doctor(DoctorAct::Ignore(IgnoreAct::Explain("a b.log".into()))),
            &ctx,
        );
        let explained = |t: &RepoTab| {
            t.doctor
                .as_ref()
                .and_then(|d| d.ignore.explained.as_ref())
                .and_then(|(_, r)| r.as_ref())
                .and_then(|r| r.as_ref().ok())
                .cloned()
        };
        wait(&mut tab, &ctx, |t| {
            explained(t).is_some()
                && t.doctor
                    .as_ref()
                    .is_some_and(|d| d.ignore.files.is_some() && d.ignore.ignored.is_some())
        })?;
        assert!(
            tab.doctor
                .as_ref()
                .is_some_and(|d| d.section == Section::Ignore)
        );
        let e = explained(&tab).ok_or("not explained")?;
        assert!(e.ignored());
        assert_eq!(e.rule.map(|r| r.pattern), Some("*.log".to_owned()));

        // Edit the top .gitignore: preview, then save.
        tab.apply(Action::Doctor(DoctorAct::Ignore(IgnoreAct::Edit(0))), &ctx);
        wait(&mut tab, &ctx, |t| {
            t.doctor
                .as_ref()
                .and_then(|d| d.ignore.editing.as_ref())
                .is_some_and(|e| e.original.is_some())
        })?;
        if let Some(e) = tab.doctor.as_mut().and_then(|d| d.ignore.editing.as_mut()) {
            e.text = "*.tmp\n".into();
        }
        tab.apply(Action::Doctor(DoctorAct::Ignore(IgnoreAct::Preview)), &ctx);
        wait(&mut tab, &ctx, |t| {
            t.doctor
                .as_ref()
                .and_then(|d| d.ignore.editing.as_ref())
                .is_some_and(|e| matches!(e.preview, Some(Some(_))))
        })?;
        let preview = tab
            .doctor
            .as_ref()
            .and_then(|d| d.ignore.editing.as_ref())
            .and_then(|e| e.preview.clone())
            .flatten()
            .ok_or("no preview")??;
        assert_eq!(preview.newly_ignored, ["x.tmp"]);
        assert_eq!(preview.no_longer_ignored, ["a b.log"]);
        tab.apply(Action::Doctor(DoctorAct::Ignore(IgnoreAct::Save)), &ctx);
        wait(&mut tab, &ctx, |t| {
            t.doctor
                .as_ref()
                .and_then(|d| d.ignore.editing.as_ref())
                .is_some_and(|e| e.saved.is_some())
        })?;
        let saved = std::fs::read_to_string(dir.join(".gitignore")).map_err(|e| e.to_string())?;
        assert_eq!(saved, "*.tmp\n");
        // The explanation is read again: no longer ignored.
        wait(&mut tab, &ctx, |t| {
            explained(t).is_some_and(|e| !e.ignored())
        })?;

        // Edits not saved: Esc and the close button don't drop them without asking.
        if let Some(e) = tab.doctor.as_mut().and_then(|d| d.ignore.editing.as_mut()) {
            e.text = "*.other\n".into();
        }
        assert!(tab.doctor.as_ref().is_some_and(|d| d.asking()));
        tab.apply(Action::Doctor(DoctorAct::Close), &ctx);
        draw(&mut tab, &ctx);
        assert!(tab.doctor.as_ref().is_some_and(|d| d.closing));
        tab.apply(Action::Doctor(DoctorAct::AnswerClose(false)), &ctx);
        assert!(tab.doctor.as_ref().is_some_and(|d| !d.closing));
        // Another section shown meanwhile: the ignore section's reads don't pull the page
        // back to it.
        tab.apply(Action::Doctor(DoctorAct::Show(Section::Flags)), &ctx);
        tab.apply(
            Action::Doctor(DoctorAct::Ignore(IgnoreAct::CloseEditor)),
            &ctx,
        );
        wait(&mut tab, &ctx, |t| {
            t.doctor.as_ref().is_some_and(|d| d.flags.list.is_some())
        })?;
        assert!(
            tab.doctor
                .as_ref()
                .is_some_and(|d| d.section == Section::Flags)
        );
        tab.apply(Action::Doctor(DoctorAct::Close), &ctx);
        assert!(tab.doctor.is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
