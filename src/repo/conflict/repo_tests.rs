//! The Original section and Keep both on real conflicted repositories, through the code paths
//! the conflict editor and the dialog use.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::KeepBoth;
use super::keep_both;
use super::original::{Change, IndexBases, Original};
use super::{ConflictEditor, Segment};
use crate::git::{self, InProgress};

/// A new repository with one commit on `main` of `files` (name, contents).
fn repo(name: &str, files: &[(&str, &[u8])]) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
        &["config", "merge.conflictStyle", "merge"],
    ] {
        git::run(Some(&dir), args)?;
    }
    commit(&dir, files, "base")?;
    Ok(dir)
}

/// Writes `files` (an empty content deletes the file) and commits them all.
fn commit(dir: &Path, files: &[(&str, &[u8])], message: &str) -> Result<(), String> {
    for (name, bytes) in files {
        let file = dir.join(name);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if bytes.is_empty() {
            let _ = std::fs::remove_file(&file);
        } else {
            std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        }
    }
    git::run(Some(dir), &["add", "-A"])?;
    git::run(Some(dir), &["commit", "-qm", message, "--allow-empty"])?;
    Ok(())
}

/// `ours` committed on main, `theirs` on `branch` (from the first commit), then `branch`
/// merged into main: left in conflict.
fn conflicted(
    name: &str,
    base: &[(&str, &[u8])],
    ours: &[(&str, &[u8])],
    theirs: &[(&str, &[u8])],
    branch: &str,
) -> Result<PathBuf, String> {
    let dir = repo(name, base)?;
    git::run(Some(&dir), &["checkout", "-q", "-b", branch])?;
    commit(&dir, theirs, "theirs")?;
    git::run(Some(&dir), &["checkout", "-q", "main"])?;
    commit(&dir, ours, "ours")?;
    git::run_ok_codes(Some(&dir), &["merge", "-q", "--no-edit", branch], &[0, 1])?;
    Ok(dir)
}

fn editor_with_bases(dir: &Path, path: &str) -> Result<ConflictEditor, String> {
    let mut ed = ConflictEditor::open(dir, path, Some(InProgress::Merge)).ok_or("no conflicts")?;
    ed.read_bases(dir);
    Ok(ed)
}

fn base_of(ed: &ConflictEditor, k: usize) -> Option<String> {
    match ed.original.of(&ed.segments, k) {
        Original::Text(base) => Some(base.to_owned()),
        _ => None,
    }
}

/// One side deleted the validation, the other changed it: the base from the index shows both,
/// for a name with spaces and letters outside ASCII.
#[test]
fn the_base_of_a_conflict_comes_from_the_index() -> Result<(), String> {
    let name = "src/my fïle ✓.txt";
    let base = b"start\nvalidate(input)\nsave(input)\nend\n" as &[u8];
    let dir = repo("original-index", &[])?;
    commit(&dir, &[(name, base)], "base")?;
    git::run(Some(&dir), &["checkout", "-q", "-b", "feature/x"])?;
    commit(
        &dir,
        &[(name, b"start\nvalidate(input, strict)\nsave(input)\nend\n")],
        "theirs",
    )?;
    git::run(Some(&dir), &["checkout", "-q", "main"])?;
    commit(&dir, &[(name, b"start\nsave(input)\nend\n")], "ours")?;
    git::run_ok_codes(
        Some(&dir),
        &["merge", "-q", "--no-edit", "feature/x"],
        &[0, 1],
    )?;

    let mut ed = editor_with_bases(&dir, name)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!ed.wants_bases());
    assert_eq!(base_of(&ed, 0).as_deref(), Some("validate(input)\n"));
    ed.original.toggle(0);
    ed.original.prepare(&ed.segments);
    let (ours, theirs) = ed.original.diffs(0).ok_or("no changes worked out")?;
    assert_eq!(super::original::summary(ours), "deleted all 1 line");
    assert_eq!(
        theirs.clone(),
        vec![
            (Change::Removed, "validate(input)".to_owned()),
            (Change::Added, "validate(input, strict)".to_owned()),
        ]
    );
    Ok(())
}

/// With diff3 markers the file has the bases itself: the index isn't asked.
#[test]
fn a_diff3_file_has_its_own_bases() -> Result<(), String> {
    let dir = repo("original-diff3", &[("f.txt", b"a\nb\nc\n")])?;
    git::run(Some(&dir), &["config", "merge.conflictStyle", "diff3"])?;
    git::run(Some(&dir), &["checkout", "-q", "-b", "other"])?;
    commit(&dir, &[("f.txt", b"a\nB2\nc\n")], "theirs")?;
    git::run(Some(&dir), &["checkout", "-q", "main"])?;
    commit(&dir, &[("f.txt", b"a\nB1\nc\n")], "ours")?;
    git::run_ok_codes(Some(&dir), &["merge", "-q", "--no-edit", "other"], &[0, 1])?;
    let ed = ConflictEditor::open(&dir, "f.txt", Some(InProgress::Merge)).ok_or("no conflict")?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!ed.wants_bases());
    assert_eq!(ed.original.index, IndexBases::NotNeeded);
    assert_eq!(base_of(&ed, 0).as_deref(), Some("b\n"));
    Ok(())
}

/// Both sides added the file: the card says there is no original.
#[test]
fn add_add_has_no_original() -> Result<(), String> {
    let dir = repo("original-add-add", &[("keep", b"k\n")])?;
    git::run(Some(&dir), &["checkout", "-q", "-b", "other"])?;
    commit(&dir, &[("new file.txt", b"theirs\n")], "theirs")?;
    git::run(Some(&dir), &["checkout", "-q", "main"])?;
    commit(&dir, &[("new file.txt", b"ours\n")], "ours")?;
    git::run_ok_codes(Some(&dir), &["merge", "-q", "--no-edit", "other"], &[0, 1])?;
    let ed = editor_with_bases(&dir, "new file.txt")?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ed.original.of(&ed.segments, 0), Original::Added);
    Ok(())
}

/// Several conflicts, one edited by hand since: the others keep their bases; after Mergiraf
/// (or anything) solved one, the rest are matched again.
#[test]
fn bases_follow_the_conflicts_left() -> Result<(), String> {
    let base = b"one\nx\nx\nx\nx\ntwo\nx\nx\nx\nx\nthree\n" as &[u8];
    let dir = conflicted(
        "original-several",
        &[("f", base)],
        &[("f", b"ONE\nx\nx\nx\nx\nTWO\nx\nx\nx\nx\nthree\n")],
        &[("f", b"1\nx\nx\nx\nx\n2\nx\nx\nx\nx\nthree\n")],
        "other",
    )?;
    let mut ed = editor_with_bases(&dir, "f")?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ed.choices.len(), 2);
    assert_eq!(base_of(&ed, 0).as_deref(), Some("one\n"));
    assert_eq!(base_of(&ed, 1).as_deref(), Some("two\n"));
    // The first one solved: the second is the only one left, with its base.
    let rest = "1\nx\nx\nx\nx\n<<<<<<< HEAD\nTWO\n=======\n2\n>>>>>>> other\nx\nx\nx\nx\nthree\n";
    assert_eq!(ed.take_solved(rest.to_owned()), 1);
    assert_eq!(base_of(&ed, 0).as_deref(), Some("two\n"));
    // A conflict git didn't leave (edited by hand): no base, and why.
    let edited = rest.replace("TWO", "TWO edited");
    ed.take_solved(edited);
    assert!(matches!(
        ed.original.of(&ed.segments, 0),
        Original::Unknown(why) if why.contains("edited")
    ));
    Ok(())
}

/// A binary conflict: both versions written beside it, named after their branches, with
/// their origin and size; nothing staged until asked, then the conflict is resolved.
#[test]
fn keep_both_writes_each_version_beside_it() -> Result<(), String> {
    let ours = b"\x89PNG\r\n\x1a\n\x00ours".as_slice();
    let theirs = b"\x89PNG\r\n\x1a\n\x00theirs, longer".as_slice();
    let dir = conflicted(
        "keep-both",
        &[("assets/logo é.png", b"\x89PNG\r\n\x1a\n\x00base")],
        &[("assets/logo é.png", ours)],
        &[("assets/logo é.png", theirs)],
        "feature/x",
    )?;
    let path = "assets/logo é.png";
    let plan = keep_both::plan(&dir, path, Some(InProgress::Merge))?;
    assert_eq!(plan.ours.target, "assets/logo é.main.png");
    assert_eq!(plan.theirs.target, "assets/logo é.feature-x.png");
    assert_eq!(plan.ours.origin, "Ours (main)");
    assert_eq!(plan.theirs.origin, "Theirs (feature/x)");
    assert_eq!(
        (plan.ours.size, plan.theirs.size),
        (ours.len() as u64, theirs.len() as u64)
    );
    keep_both::write(&dir, &plan)?;
    let read = |rel: &str| std::fs::read(dir.join(rel)).map_err(|e| e.to_string());
    assert_eq!(read(&plan.ours.target)?, ours);
    assert_eq!(read(&plan.theirs.target)?, theirs);
    // Not staged: the file is still in conflict, the two new ones untracked.
    let status = git::run(Some(&dir), &["status", "--porcelain", "-z", "--no-renames"])?;
    assert!(status.contains("UU assets/logo é.png"), "{status}");
    assert!(status.contains("?? assets/logo é.main.png"), "{status}");
    for step in keep_both::stage_steps(&plan) {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        git::run(Some(&dir), &args)?;
    }
    let status = git::run(Some(&dir), &["status", "--porcelain", "-z", "--no-renames"])?;
    let _ = std::fs::remove_dir_all(&dir);
    let mut entries: Vec<&str> = status.split('\0').filter(|e| !e.is_empty()).collect();
    entries.sort_unstable();
    assert_eq!(
        entries,
        vec![
            "A  assets/logo é.feature-x.png",
            "A  assets/logo é.main.png",
            "D  assets/logo é.png",
        ]
    );
    Ok(())
}

/// Names that are taken (a file on disk, a tracked file) are passed over, and a file that
/// appears after the preview is never written over: nothing is left half done.
#[test]
fn keep_both_never_overwrites() -> Result<(), String> {
    let dir = conflicted(
        "keep-both-taken",
        &[
            ("logo.png", b"\x00base"),
            ("logo.feature-x.png", b"tracked"),
        ],
        &[("logo.png", b"\x00ours")],
        &[("logo.png", b"\x00theirs")],
        "feature-x",
    )?;
    std::fs::write(dir.join("logo.main.png"), "mine, untracked").map_err(|e| e.to_string())?;
    let plan = keep_both::plan(&dir, "logo.png", Some(InProgress::Merge))?;
    assert_eq!(plan.ours.target, "logo.main-2.png");
    assert_eq!(plan.ours.taken, vec!["logo.main.png".to_owned()]);
    assert_eq!(plan.theirs.target, "logo.feature-x-2.png");
    // Something appears at the second name after the preview.
    std::fs::write(dir.join("logo.feature-x-2.png"), "new").map_err(|e| e.to_string())?;
    let written = keep_both::write(&dir, &plan);
    let first_left = dir.join("logo.main-2.png").exists();
    let second = std::fs::read_to_string(dir.join("logo.feature-x-2.png"));
    let untracked = std::fs::read_to_string(dir.join("logo.main.png"));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        written.as_ref().is_err_and(
            |e| e.contains("nothing was overwritten") && e.contains("logo.feature-x-2.png")
        ),
        "{written:?}"
    );
    assert!(!first_left, "the first file is taken back");
    assert_eq!(second.map_err(|e| e.to_string())?, "new");
    assert_eq!(untracked.map_err(|e| e.to_string())?, "mine, untracked");
    Ok(())
}

/// Modify/delete: only one version, so nothing to keep both of; the reason is given. The
/// card of such a file (no markers) is not a conflict editor.
#[test]
fn keep_both_refuses_modify_delete() -> Result<(), String> {
    let dir = conflicted(
        "keep-both-deleted",
        &[("img.bin", b"\x00base"), ("other", b"o")],
        &[("img.bin", b"\x00ours")],
        &[("img.bin", b"")],
        "gone",
    )?;
    let plan = keep_both::plan(&dir, "img.bin", Some(InProgress::Merge));
    let stages = git::conflict_stages(&dir, "img.bin")?;
    let editor = ConflictEditor::open(&dir, "img.bin", Some(InProgress::Merge));
    let resolved = keep_both::plan(&dir, "other", Some(InProgress::Merge));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(stages.base.is_some() && stages.ours.is_some() && stages.theirs.is_none());
    assert!(
        plan.as_ref()
            .is_err_and(|e| e.contains("Theirs deleted img.bin")),
        "{plan:?}"
    );
    assert!(editor.is_none());
    assert!(resolved.is_err_and(|e| e.contains("no longer in conflict")));
    Ok(())
}

/// During a rebase the sides are the branch rebased onto and your own commit.
#[test]
fn keep_both_during_a_rebase() -> Result<(), String> {
    let dir = conflicted(
        "keep-both-rebase",
        &[("a.dat", b"\x00base")],
        &[("a.dat", b"\x00upstream")],
        &[("a.dat", b"\x00mine")],
        "topic",
    )?;
    git::run(Some(&dir), &["merge", "--abort"])?;
    git::run(Some(&dir), &["checkout", "-q", "topic"])?;
    git::run_ok_codes(Some(&dir), &["rebase", "-q", "main"], &[0, 1])?;
    let plan = keep_both::plan(&dir, "a.dat", Some(InProgress::Rebase));
    let _ = std::fs::remove_dir_all(&dir);
    let plan = plan?;
    assert_eq!(plan.ours.target, "a.main.dat");
    assert_eq!(plan.theirs.target, "a.topic.dat");
    assert_eq!(plan.ours.origin, "Upstream (main)");
    assert_eq!(plan.theirs.origin, "Your commit (topic)");
    Ok(())
}

/// Polls `tab` until `done` holds (or ten seconds have passed).
fn poll_until(
    tab: &mut crate::repo::RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&crate::repo::RepoTab) -> bool,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Through the tab, as the UI does it: the editor's bases arrive in the background, and the
/// Keep both dialog goes from reading to its preview, then writes.
#[test]
fn through_the_tab() -> Result<(), String> {
    use crate::git::{Area, StatusFile};
    use crate::repo::{DiffKey, RepoTab};
    let dir = conflicted(
        "conflict-tab",
        &[("t.txt", b"a\nkeep\nb\n"), ("pic.gif", b"GIF89a\x00base")],
        &[("t.txt", b"a\nmine\nb\n"), ("pic.gif", b"GIF89a\x00ours")],
        &[
            ("t.txt", b"a\ntheirs\nb\n"),
            ("pic.gif", b"GIF89a\x00theirs"),
        ],
        "feature",
    )?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.work.in_progress = Some(InProgress::Merge);
    let file = |path: &str| StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'U',
        area: Area::Conflicted,
    };
    tab.open_diff(DiffKey::Work(file("t.txt")), &ctx);
    let based = poll_until(&mut tab, &ctx, |tab| {
        tab.conflict.as_ref().is_some_and(|ed| !ed.wants_bases())
    });
    let base = tab.conflict.as_ref().and_then(|ed| base_of(ed, 0));

    tab.keep_both("pic.gif".to_owned(), &ctx);
    let previewed = poll_until(&mut tab, &ctx, |tab| {
        matches!(tab.keep_both, Some(KeepBoth::Preview(_)))
    });
    tab.keep_both_write(&ctx);
    let written = poll_until(&mut tab, &ctx, |tab| {
        matches!(tab.keep_both, Some(KeepBoth::Written(_)))
    });
    let files = (
        std::fs::read(dir.join("pic.main.gif")),
        std::fs::read(dir.join("pic.feature.gif")),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(based && previewed && written);
    assert_eq!(base.as_deref(), Some("keep\n"));
    assert_eq!(files.0.map_err(|e| e.to_string())?, b"GIF89a\x00ours");
    assert_eq!(files.1.map_err(|e| e.to_string())?, b"GIF89a\x00theirs");
    Ok(())
}

/// The segments of a parsed file are what the card's base is looked up in.
#[test]
fn no_conflict_no_base() {
    let (segments, _, _) = super::parse("plain\n");
    assert!(matches!(segments.as_slice(), [Segment::Common(_)]));
    let state = super::original::State::new(&segments);
    assert_eq!(state.index, IndexBases::NotNeeded);
}
