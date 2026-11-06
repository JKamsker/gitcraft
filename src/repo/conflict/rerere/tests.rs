//! Remembered resolutions on real repositories: the switch, a resolution reused and shown,
//! forgetting it, through the tab as the conflict editor uses them.

use std::path::{Path, PathBuf};

use super::super::Segment;
use super::*;
use crate::git::{Area, StatusFile};
use crate::repo::{DiffKey, OpKind, Refresh};

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
    for step in switch_ops(true) {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        git(dir, &args)?;
    }
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
    let on_before = enabled(&dir);
    for step in switch_ops(true) {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        git(&dir, &args)?;
    }
    let on = enabled(&dir);
    let auto = git(&dir, &["config", "--local", "rerere.autoUpdate"]);
    for step in switch_ops(false) {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        git(&dir, &args)?;
    }
    let off = enabled(&dir);
    // Unset, with an rr-cache folder: on, as git has it.
    git(&dir, &["config", "--local", "--unset", "rerere.enabled"])?;
    std::fs::create_dir_all(dir.join(".git").join("rr-cache")).map_err(|e| e.to_string())?;
    let by_cache = enabled(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(on_before, Ok(false));
    assert_eq!(on, Ok(true));
    assert_eq!(auto.map(|s| s.trim().to_owned()), Ok("false".to_owned()));
    assert_eq!(off, Ok(false));
    assert_eq!(by_cache, Ok(true));
    Ok(())
}

#[test]
fn reused_files_are_read_from_gits_output() {
    let out = "Auto-merging a b/ü.txt\nCONFLICT (content): Merge conflict in a b/ü.txt\n\
               Resolved 'a b/ü.txt' using previous resolution.\nStaged 'c' using previous \
               resolution.\nRecorded preimage for 'd'\nAutomatic merge failed";
    let found: Vec<String> = reused_in(out).into_iter().collect();
    assert_eq!(found, ["a b/ü.txt", "c"]);
    assert_eq!(reused_count(out), 2);
    assert_eq!(reused_count("CONFLICT (content): Merge conflict in x"), 0);
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

/// Added on both sides: no base to rebuild the conflict from, nothing opened.
#[test]
fn a_file_added_on_both_sides_is_not_rebuilt() -> Result<(), String> {
    let dir = repo("addadd")?;
    git(&dir, &["checkout", "-qb", "side"])?;
    write(&dir, "n", "theirs\n")?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "side"])?;
    git(&dir, &["checkout", "-q", "main"])?;
    write(&dir, "n", "ours\n")?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "main"])?;
    let _ = git(&dir, &["merge", "side"]);
    write(&dir, "n", "both\n")?;
    let rebuilt = preimage(&dir, "n", Some(InProgress::Merge));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(rebuilt, Ok(None));
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
    tab.rerere.enabled = Some(true);
    tab.rerere_act(RerereAct::Forget(path.to_owned()), &ctx);
    wait(&mut tab, &ctx, |tab| tab.busy_op.is_none());
    let after = std::fs::read_to_string(dir.join(path)).map_err(|e| e.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(noted);
    assert_eq!(
        opened,
        Some((Some(Reused::Rerere), Some("kept\n".to_owned())))
    );
    assert!(!tab.rerere.reused.contains(path));
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
    tab.rerere.enabled = Some(true);
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
    harness.get_by_label_contains("git reused how you resolved");
    harness.get_by_label("Reused resolution");
    harness
        .get_by_label("Forget it and bring the conflicts back")
        .click();
    harness.run();
    harness.get_by_label("Remember resolutions").click();
    harness.run();
    let acts = &harness.state().1;
    assert!(acts.iter().any(|a| matches!(a,
        crate::repo::Action::Rerere(RerereAct::Forget(p)) if p == "dír/f.txt")));
    assert!(
        acts.iter()
            .any(|a| matches!(a, crate::repo::Action::Rerere(RerereAct::SetEnabled(false))))
    );
    Ok(())
}
