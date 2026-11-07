//! Remembered resolutions on real repositories: the switch, a resolution reused and shown,
//! forgetting it, through the tab as the conflict editor uses them.

use std::path::{Path, PathBuf};

use super::super::Segment;
use super::commands;
use super::*;
use crate::git::{Area, StatusFile};
use crate::repo::{DiffKey, OpKind, Refresh, ops};

/// A fresh repository named after `name`, with one commit.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-rerere-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        git::run(Some(&dir), args)?;
    }
    Ok(dir)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
}

fn write(dir: &Path, path: &str, text: &str) -> Result<(), String> {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, text).map_err(|e| e.to_string())
}

/// `path` changed one way on `main` and another on `side`, with rerere on: `main` checked out.
fn diverged(dir: &Path, path: &str) -> Result<(), String> {
    commands::switch(dir, true)?;
    write(dir, path, "top\nold\nbottom\n")?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", "add"])?;
    git(dir, &["checkout", "-qb", "side"])?;
    write(dir, path, "top\ntheirs\nbottom\n")?;
    git(dir, &["commit", "-qam", "side"])?;
    git(dir, &["checkout", "-q", "main"])?;
    write(dir, path, "top\nours\nbottom\n")?;
    git(dir, &["commit", "-qam", "main"])?;
    Ok(())
}

/// Merges `side` (conflicting), resolves `path` to `resolved` and commits, so rerere records
/// it; then goes back to before the merge.
fn record(dir: &Path, path: &str, resolved: &str) -> Result<(), String> {
    let merged = git(dir, &["merge", "side"]);
    assert!(merged.is_err_and(|e| e.contains("CONFLICT")));
    write(dir, path, resolved)?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", "merged"])?;
    git(dir, &["reset", "-q", "--hard", "HEAD~1"])?;
    Ok(())
}

#[test]
fn the_switch_turns_rerere_on_and_off_here() -> Result<(), String> {
    let dir = repo("switch")?;
    let before = setup(&dir);
    commands::switch(&dir, true)?;
    let on = setup(&dir);
    let auto = git(&dir, &["config", "--local", "rerere.autoUpdate"]);
    commands::switch(&dir, false)?;
    let off = setup(&dir);
    let auto_after = git(&dir, &["config", "--local", "rerere.autoUpdate"]);
    // Unset, with an rr-cache folder: on, as git has it; staging by itself, said so.
    git(&dir, &["config", "--local", "--unset", "rerere.enabled"])?;
    git(&dir, &["config", "--local", "rerere.autoUpdate", "true"])?;
    std::fs::create_dir_all(dir.join(".git").join("rr-cache")).map_err(|e| e.to_string())?;
    let by_cache = setup(&dir);
    // A value git can't read: said, not taken for off.
    git(&dir, &["config", "--local", "rerere.enabled", "maybe"])?;
    let broken = setup(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let state = |on, stages| Ok(Setup { on, stages });
    assert_eq!(before, state(false, false));
    assert_eq!(on, state(true, false));
    assert_eq!(auto.map(|s| s.trim().to_owned()), Ok("false".to_owned()));
    assert_eq!(off, state(false, false));
    assert!(auto_after.is_err(), "{auto_after:?}");
    assert_eq!(by_cache, state(true, true));
    assert!(broken.is_err_and(|e| e.contains("rerere.enabled")));
    Ok(())
}

/// Turned on in the middle of a conflict, that conflict is remembered too.
#[test]
fn turning_it_on_during_a_conflict_remembers_that_one() -> Result<(), String> {
    let dir = repo("midway")?;
    let path = "f.txt";
    diverged(&dir, path)?;
    commands::switch(&dir, false)?;
    let _ = git(&dir, &["merge", "side"]);
    commands::switch(&dir, true)?;
    write(&dir, path, "top\nlate\nbottom\n")?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "merged"])?;
    git(&dir, &["reset", "-q", "--hard", "HEAD~1"])?;
    let again = git(&dir, &["merge", "side"]).err().unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(reused_in(&again).contains(path), "{again}");
    Ok(())
}

#[test]
fn reused_files_are_counted() {
    let out = "Resolved 'a b/ü.txt' using previous resolution.\nStaged 'c' using previous \
               resolution.";
    assert_eq!(reused_count(out), 2);
    assert_eq!(reused_count(""), 0);
}

/// A resolution recorded once is reused the next time: the file is left unmerged and
/// unstaged, and the editor shows each conflict with the text reused for it.
#[test]
fn a_reused_resolution_opens_with_what_was_reused() -> Result<(), String> {
    let dir = repo("reuse")?;
    let path = "dír ü/my file.txt";
    diverged(&dir, path)?;
    record(&dir, path, "top\nours and theirs\nbottom\n")?;
    let again = git(&dir, &["merge", "side"]).err().unwrap_or_default();
    let status = git(&dir, &["status", "--porcelain=v1", "-z"])?;
    let editor = open_reused(&dir, path, Some(InProgress::Merge), true);
    let plain = ConflictEditor::open(&dir, path, Some(InProgress::Merge));
    let _ = std::fs::remove_dir_all(&dir);

    assert!(reused_in(&again).contains(path), "{again}");
    assert!(status.starts_with("UU "), "{status}");
    // Its markers are gone: the plain editor doesn't open it.
    assert!(plain.is_none());
    let ed = editor?.ok_or("not opened")?;
    assert_eq!(ed.reused, Some(Reused::Rerere));
    assert_eq!(ed.choices, vec![Some(Choice::Recorded)]);
    assert_eq!(ed.recorded(0), Some("ours and theirs\n"));
    assert!(!ed.manual);
    assert_eq!(ed.result, "top\nours and theirs\nbottom\n");
    assert_eq!(ed.solved(), Ok("top\nours and theirs\nbottom\n".to_owned()));
    // Each side as the conflict had it, the base too.
    match ed
        .segments
        .iter()
        .find(|s| matches!(s, Segment::Conflict { .. }))
    {
        Some(Segment::Conflict { ours, base, theirs }) => {
            assert_eq!(ours, "ours\n");
            assert_eq!(base.as_deref(), Some("old\n"));
            assert_eq!(theirs, "theirs\n");
        }
        _ => return Err("no conflict".to_owned()),
    }
    Ok(())
}

/// Choosing a side instead of what was reused, and back.
#[test]
fn a_reused_text_can_be_swapped_for_a_side_and_back() -> Result<(), String> {
    let pre = "a\n<<<<<<< HEAD\nx\n||||||| base\no\n=======\ny\n>>>>>>> b\nz\n";
    let mut ed =
        ConflictEditor::reused("f", pre, "a\nxy\nz\n".into(), None, false).ok_or("not opened")?;
    assert_eq!(ed.reused, Some(Reused::NoMarkers));
    ed.choose(0, Choice::Theirs);
    assert_eq!(ed.result, "a\ny\nz\n");
    ed.choose(0, Choice::Recorded);
    assert_eq!(ed.result, "a\nxy\nz\n");
    // Unchosen: the conflict's markers are back, and it is unresolved.
    ed.choose(0, Choice::Recorded);
    assert!(ed.result.contains("<<<<<<< HEAD"));
    assert!(ed.solved().is_err());
    Ok(())
}

/// The text around a conflict edited as well: shown as it is, edited by hand.
#[test]
fn a_file_changed_around_its_conflicts_opens_as_it_is() -> Result<(), String> {
    let pre = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nz\n";
    let ed =
        ConflictEditor::reused("f", pre, "A\nxy\nZ\n".into(), None, true).ok_or("not opened")?;
    assert!(ed.manual);
    assert_eq!(ed.choices, vec![None]);
    assert_eq!(ed.result, "A\nxy\nZ\n");
    assert_eq!(ed.solved(), Ok("A\nxy\nZ\n".to_owned()));
    assert!(ConflictEditor::reused("f", "no conflict\n", String::new(), None, true).is_none());
    Ok(())
}

/// Forgetting drops the recorded resolution and brings the markers back: the next time, the
/// conflict is not resolved by itself.
#[test]
fn forgetting_brings_the_conflict_back_for_good() -> Result<(), String> {
    let dir = repo("forget")?;
    let path = "f.txt";
    diverged(&dir, path)?;
    record(&dir, path, "top\nmine\nbottom\n")?;
    let _ = git(&dir, &["merge", "side"]);
    for step in forget_ops(path, true) {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        git(&dir, &args)?;
    }
    let after = std::fs::read_to_string(dir.join(path)).map_err(|e| e.to_string())?;
    let editor = ConflictEditor::open(&dir, path, Some(InProgress::Merge));
    git(&dir, &["merge", "--abort"])?;
    let next = git(&dir, &["merge", "side"]).err().unwrap_or_default();
    let next_file = std::fs::read_to_string(dir.join(path)).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(after.contains("<<<<<<< "), "{after}");
    assert!(editor.is_some());
    assert!(reused_in(&next).is_empty(), "{next}");
    assert!(next_file.contains("<<<<<<< "), "{next_file}");
    Ok(())
}

/// Added on both sides: rebuilt with the empty file as its base, the reused text shown.
#[test]
fn a_file_added_on_both_sides_is_rebuilt() -> Result<(), String> {
    let dir = repo("addadd")?;
    commands::switch(&dir, true)?;
    git(&dir, &["checkout", "-qb", "side"])?;
    write(&dir, "n", "theirs\n")?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "side"])?;
    git(&dir, &["checkout", "-q", "main"])?;
    write(&dir, "n", "ours\n")?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "main"])?;
    record(&dir, "n", "both\n")?;
    let again = git(&dir, &["merge", "side"]).err().unwrap_or_default();
    let editor = open_reused(&dir, "n", Some(InProgress::Merge), true);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(reused_in(&again).contains("n"), "{again}");
    let ed = editor?.ok_or("not opened")?;
    assert_eq!(ed.recorded(0), Some("both\n"));
    Ok(())
}

/// A conflicted file without markers that git doesn't merge with the usual markers (another
/// marker size, `-merge`, a filter) or that is binary isn't rebuilt: no editor that would take
/// marker-like lines for resolved text.
#[test]
fn files_merged_otherwise_are_not_rebuilt() -> Result<(), String> {
    for (name, attributes, ours) in [
        (
            "marker-size",
            "f conflict-marker-size=10\n",
            "top\nours\nbottom\n",
        ),
        ("no-merge", "f -merge\n", "top\nours\nbottom\n"),
        (
            "filtered",
            "f filter=gitcraft-none\n",
            "top\nours\nbottom\n",
        ),
        ("binary", "", "top\n\0ours\nbottom\n"),
    ] {
        let dir = repo(name)?;
        write(&dir, ".gitattributes", attributes)?;
        write(&dir, "f", "top\nold\nbottom\n")?;
        git(&dir, &["add", "-A"])?;
        git(&dir, &["commit", "-qm", "add"])?;
        git(&dir, &["checkout", "-qb", "side"])?;
        write(&dir, "f", &ours.replace("ours", "theirs"))?;
        git(&dir, &["commit", "-qam", "side"])?;
        git(&dir, &["checkout", "-q", "main"])?;
        write(&dir, "f", ours)?;
        git(&dir, &["commit", "-qam", "main"])?;
        let _ = git(&dir, &["merge", "side"]);
        // Without markers of the usual size, whatever git left.
        write(
            &dir,
            "f",
            "top\n<<<<<<<<<< HEAD\nours\n==========\nx\n>>>>>>>>>> side\nbottom\n",
        )?;
        let opened = open_reused(&dir, "f", Some(InProgress::Merge), false);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            matches!(opened, Ok(None)),
            "{name}: {:?}",
            opened.map(|e| e.map(|e| e.result))
        );
    }
    Ok(())
}

/// Checked out with CRLF line ends while the index keeps LF: the reused text is still found
/// conflict by conflict, and choosing a side keeps CRLF.
#[test]
fn crlf_checkouts_are_split_too() -> Result<(), String> {
    let dir = repo("crlf")?;
    write(&dir, ".gitattributes", "*.txt text eol=crlf\n")?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "attributes"])?;
    let path = "f.txt";
    diverged(&dir, path)?;
    record(&dir, path, "top\r\nkept\r\nbottom\r\n")?;
    let _ = git(&dir, &["merge", "side"]);
    let editor = open_reused(&dir, path, Some(InProgress::Merge), true);
    let _ = std::fs::remove_dir_all(&dir);
    let mut ed = editor?.ok_or("not opened")?;
    assert!(!ed.manual);
    assert_eq!(ed.recorded(0), Some("kept\r\n"));
    ed.choose(0, Choice::Theirs);
    assert_eq!(ed.result, "top\r\ntheirs\r\nbottom\r\n");
    Ok(())
}

/// A file that can't be rebuilt isn't tried again on every refresh, until the next operation.
#[test]
fn an_unopenable_file_is_not_tried_again_until_the_next_operation() -> Result<(), String> {
    let dir = repo("unopenable")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    let generation = tab.conflict_generation;
    tab.reused_opened(generation, "f".to_owned(), Ok(None));
    assert!(tab.rerere.unopenable.contains("f"));
    tab.reused_opened(generation, "g".to_owned(), Err("too old".to_owned()));
    assert_eq!(tab.status, Some(("too old".to_owned(), true)));
    tab.note_reused(None);
    assert!(tab.rerere.unopenable.is_empty());
    Ok(())
}

/// Polls `tab` until `done` holds (or a few seconds pass).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Through the tab: the merge run by gitcraft says which files rerere resolved, opening one
/// shows the editor with what was reused, and "forget" brings its markers back.
#[test]
fn the_tab_opens_a_reused_resolution_and_forgets_it() -> Result<(), String> {
    let dir = repo("tab")?;
    let path = "a b.txt";
    diverged(&dir, path)?;
    record(&dir, path, "top\nkept\nbottom\n")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.run_ops(
        "Merge side",
        ops(&[&["merge", "side"]]),
        Refresh::Status,
        OpKind::plain(),
        &ctx,
    );
    wait(&mut tab, &ctx, |tab| tab.busy_op.is_none());
    let noted = tab.rerere.reused.contains(path);
    tab.work.in_progress = Some(InProgress::Merge);
    let file = StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'U',
        area: Area::Conflicted,
    };
    tab.open_diff(DiffKey::Work(file), &ctx);
    wait(&mut tab, &ctx, |tab| tab.conflict.is_some());
    let opened = tab
        .conflict
        .as_ref()
        .map(|ed| (ed.reused, ed.recorded(0).map(str::to_owned)));
    // What the banner's confirmation runs.
    tab.run_ops(
        "Forget",
        forget_ops(path, true),
        Refresh::Status,
        OpKind::plain(),
        &ctx,
    );
    wait(&mut tab, &ctx, |tab| tab.busy_op.is_none());
    let after = std::fs::read_to_string(dir.join(path)).map_err(|e| e.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(noted);
    assert_eq!(
        opened,
        Some((Some(Reused::Rerere), Some("kept\n".to_owned())))
    );
    assert!(after?.contains("<<<<<<< "));
    Ok(())
}

/// Drawn: the banner says what was reused and its button asks to forget it; each conflict shows
/// the reused text, and the switch turns rerere off.
#[test]
fn the_editor_shows_what_was_reused() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    let pre = "a\n<<<<<<< HEAD\nx\n||||||| base\no\n=======\ny\n>>>>>>> b\nz\n";
    let ed = ConflictEditor::reused("dír/f.txt", pre, "a\nxy\nz\n".into(), None, true)
        .ok_or("not opened")?;
    let dir = repo("drawn")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    tab.conflict = Some(ed);
    tab.rerere.setup = Some(Setup {
        on: true,
        stages: false,
    });
    let state = (tab, Vec::<crate::repo::Action>::new(), false);
    let mut harness = egui_kittest::Harness::new_ui_state(
        |ui, (tab, acts, fonts): &mut (RepoTab, Vec<crate::repo::Action>, bool)| {
            // gitcraft's fonts first: they apply from the next frame.
            if !*fonts {
                crate::theme::install_fonts(ui.ctx());
                *fonts = true;
                return;
            }
            tab.conflict_view(ui, acts);
        },
        state,
    );
    harness.run();
    harness.get_by_label_contains("Resolved as you did before");
    harness.get_by_label_contains("Reused: the text now in place");
    harness.get_by_label("Forget this resolution").click();
    harness.run();
    harness.get_by_label("Remember resolutions").click();
    harness.run();
    let acts = &harness.state().1;
    assert!(acts.iter().any(|a| matches!(a,
        crate::repo::Action::Dialog(crate::repo::Dialog::Confirm { body, .. })
            if body.contains("dír/f.txt"))));
    assert!(
        acts.iter()
            .any(|a| matches!(a, crate::repo::Action::Rerere(RerereAct::SetEnabled(false))))
    );
    Ok(())
}
