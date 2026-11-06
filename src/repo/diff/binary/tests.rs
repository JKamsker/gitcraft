//! Binary files through the diff the view loads, on real repositories.

use std::path::{Path, PathBuf};

use super::extract::tests::{make_docx, make_pdf};
use super::view::size_change;
use super::zip::{self, tests::make_zip};
use super::{member_changes, textconv_for};
use crate::git::{self, Area, DiffOptions, StatusFile};
use crate::repo::DiffKey;
use crate::repo::diff::line_diff::Edit;
use crate::repo::diff::load;

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-binary-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    Ok(dir)
}

fn commit(dir: &Path, name: &str, bytes: &[u8], message: &str) -> Result<String, String> {
    std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
    git::run(Some(dir), &["add", "-A"])?;
    git::run(Some(dir), &["commit", "-qm", message])?;
    Ok(git::head(dir))
}

fn commit_key(hash: &str, path: &str) -> DiffKey {
    DiffKey::Commit {
        hash: hash.to_owned(),
        path: path.to_owned(),
    }
}

fn texts(lines: &[(Edit, String)]) -> Vec<String> {
    lines
        .iter()
        .map(|(e, l)| {
            let mark = match e {
                Edit::Same => ' ',
                Edit::Del => '-',
                Edit::Add => '+',
            };
            format!("{mark}{l}")
        })
        .collect()
}

#[test]
fn sizes_said_plainly() {
    assert_eq!(size_change(1000, 1000), "same size");
    assert_eq!(size_change(1024, 3072), "+2.0 KB");
    assert_eq!(size_change(300, 0), "−300 B");
}

#[test]
fn members_added_removed_and_changed() {
    let a = make_zip(&[
        ("same.txt", b"x", true),
        ("gone.txt", b"y", false),
        ("edited.txt", b"one", true),
    ]);
    let b = make_zip(&[
        ("same.txt", b"x", true),
        ("edited.txt", b"two!", true),
        ("new/ü.txt", b"z", false),
    ]);
    let (a, b) = (
        zip::members(&a).unwrap_or_default(),
        zip::members(&b).unwrap_or_default(),
    );
    let changes: Vec<(String, Option<u64>, Option<u64>)> = member_changes(&a, &b)
        .into_iter()
        .map(|c| (c.name, c.old, c.new))
        .collect();
    assert_eq!(
        changes,
        [
            ("edited.txt".to_owned(), Some(3), Some(4)),
            ("gone.txt".to_owned(), Some(1), None),
            ("new/ü.txt".to_owned(), None, Some(1)),
        ]
    );
}

/// A Word document changed in a commit: its text diffed by paragraph, its kind and members.
#[test]
fn a_word_document_in_a_commit() -> Result<(), String> {
    let dir = repo("docx")?;
    let name = "Spec ü.docx";
    commit(&dir, name, &make_docx(&["Intro", "Old plan", "End"]), "one")?;
    let hash = commit(&dir, name, &make_docx(&["Intro", "New plan", "End"]), "two")?;
    let data = load(&dir, &commit_key(&hash, name), DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    let pair = data.binary.ok_or("not shown as a binary")?;
    assert_eq!(
        texts(pair.text.as_deref().unwrap_or_default()),
        [" Intro", "-Old plan", "+New plan", " End"]
    );
    assert_eq!(
        pair.new.as_ref().and_then(|s| s.kind.as_deref()),
        Some("Word document")
    );
    let changed: Vec<&str> = pair
        .members
        .iter()
        .flatten()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(changed, ["word/document.xml"]);
    assert!(pair.error.is_none());
    Ok(())
}

/// A PDF added (no side before), staged and in the working tree; a raw binary changed.
#[test]
fn a_new_pdf_and_raw_bytes() -> Result<(), String> {
    let dir = repo("pdf")?;
    commit(&dir, "blob.bin", &[0, 1, 2, 3, 255], "blob")?;
    std::fs::write(dir.join("blob.bin"), [0, 1, 9, 3, 255, 7]).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("report.pdf"), make_pdf(&["Total: 42"], true))
        .map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["add", "report.pdf"])?;
    let staged = DiffKey::Work(StatusFile {
        path: "report.pdf".into(),
        old_path: None,
        code: 'A',
        area: Area::Staged,
    });
    let data = load(&dir, &staged, DiffOptions::default(), true)?;
    let pdf = data.binary.ok_or("pdf not shown as a binary")?;
    assert!(pdf.old.is_none());
    let facts = pdf
        .new
        .as_ref()
        .map(|s| s.facts.clone())
        .unwrap_or_default();
    assert!(
        facts.contains(&("Pages".to_owned(), "1".to_owned())),
        "{facts:?}"
    );
    assert_eq!(
        texts(pdf.text.as_deref().unwrap_or_default()),
        ["+Total: 42"]
    );

    let unstaged = DiffKey::Work(StatusFile {
        path: "blob.bin".into(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let data = load(&dir, &unstaged, DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    let blob = data.binary.ok_or("blob not shown as a binary")?;
    assert_eq!(
        blob.old.as_ref().map(|s| s.head.clone()),
        Some(vec![0, 1, 2, 3, 255])
    );
    assert_eq!(blob.new.as_ref().and_then(|s| s.size), Some(6));
    assert_eq!(
        blob.new.as_ref().map(|s| s.head.clone()),
        Some(vec![0, 1, 9, 3, 255, 7])
    );
    assert!(blob.text.is_none() && blob.members.is_none());
    assert!(!data.partial);
    Ok(())
}

/// A file with a textconv driver: git's text diff is kept, said to be the program's, and not
/// offered a line at a time.
#[cfg(unix)]
#[test]
fn a_textconv_diff_is_staged_whole() -> Result<(), String> {
    let dir = repo("textconv")?;
    std::fs::write(dir.join(".gitattributes"), "*.upper diff=upper\n")
        .map_err(|e| e.to_string())?;
    git::run(
        Some(&dir),
        &["config", "diff.upper.textconv", "tr a-z A-Z <"],
    )?;
    commit(&dir, "a b.upper", b"one\ntwo\n", "one")?;
    std::fs::write(dir.join("a b.upper"), "one\nthree\n").map_err(|e| e.to_string())?;
    assert_eq!(
        textconv_for(&dir, "a b.upper").as_deref(),
        Some("tr a-z A-Z <")
    );
    assert_eq!(textconv_for(&dir, "plain.txt"), None);
    let key = DiffKey::Work(StatusFile {
        path: "a b.upper".into(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let data = load(&dir, &key, DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(data.textconv.as_deref(), Some("tr a-z A-Z <"));
    assert!(!data.partial);
    assert!(data.lines.iter().any(|l| l.text == "THREE"));
    assert!(data.binary.is_none());
    // Staged, it is unstaged whole.
    use super::view::textconv_rest;
    assert!(textconv_rest(Some(Area::Staged)).contains("Unstage it as a whole file"));
    assert!(textconv_rest(Some(Area::Unstaged)).contains("Stage or discard"));
    assert!(!textconv_rest(None).contains("whole file"));
    Ok(())
}

/// A symbolic link is diffed with git's default driver, whatever its `diff` attribute says:
/// its diff isn't said to be the textconv's.
#[cfg(unix)]
#[test]
fn a_link_has_no_textconv() -> Result<(), String> {
    let dir = repo("textconv-link")?;
    std::fs::write(dir.join(".gitattributes"), "*.upper diff=upper\n")
        .map_err(|e| e.to_string())?;
    git::run(
        Some(&dir),
        &["config", "diff.upper.textconv", "tr a-z A-Z <"],
    )?;
    std::os::unix::fs::symlink("one", dir.join("l.upper")).map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "link"])?;
    std::fs::remove_file(dir.join("l.upper")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("two", dir.join("l.upper")).map_err(|e| e.to_string())?;
    let key = DiffKey::Work(StatusFile {
        path: "l.upper".into(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let data = load(&dir, &key, DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    if data.textconv.is_some() || !data.lines.iter().any(|l| l.text == "two") {
        return Err(format!("textconv {:?}", data.textconv));
    }
    let patch = "diff --git a/l b/l\nindex 1..2 120000\n--- a/l\n+++ b/l\n@@ -1 +1 @@\n";
    if !crate::repo::diff::is_symlink_patch(patch)
        || crate::repo::diff::is_symlink_patch(&patch.replace("120000", "100644"))
    {
        return Err("symlink patch not told".into());
    }
    Ok(())
}

/// A side whose object can't be read (gone from the repository) is a failure, not a side
/// without the file: its members aren't all shown added.
#[test]
fn an_unreadable_side_is_not_absent() -> Result<(), String> {
    let dir = repo("gone")?;
    let old = make_zip(&[("a.txt", b"one", false), ("b.txt", b"two", false)]);
    commit(&dir, "x.zip", &old, "one")?;
    let new = make_zip(&[("a.txt", b"one", false), ("b.txt", b"three", false)]);
    let hash = commit(&dir, "x.zip", &new, "two")?;
    let blob = git::run(Some(&dir), &["rev-parse", &format!("{hash}^:x.zip")])?;
    let blob = blob.trim();
    let object = dir.join(".git/objects").join(&blob[..2]).join(&blob[2..]);
    std::fs::remove_file(&object).map_err(|e| e.to_string())?;
    let pair = super::load_pair(&dir, &commit_key(&hash, "x.zip"), "x.zip", None);
    let _ = std::fs::remove_dir_all(&dir);
    let old = pair.old.as_ref().ok_or("the old side taken for absent")?;
    if !old.failed || pair.error.is_none() || pair.members.is_some() {
        return Err(format!("failed {} error {:?}", old.failed, pair.error));
    }
    if pair.members_note.as_deref()
        != Some("The old version couldn't be read, so its members aren't compared.")
    {
        return Err(format!("{:?}", pair.members_note));
    }
    if super::view::size_text(old) != "couldn't be read" {
        return Err(super::view::size_text(old));
    }
    Ok(())
}

fn archive_side(archive: super::Archive) -> super::Side {
    super::Side {
        archive,
        ..side_with(None)
    }
}

/// Members aren't compared with a side whose list can't be read or is damaged; a name given
/// twice is said, and the first compared.
#[test]
fn members_compared_only_when_known() -> Result<(), String> {
    let list = |files: &[(&str, &[u8], bool)]| zip::directory(&make_zip(files));
    let full = list(&[("a", b"x", false)]).ok_or("no directory")?;
    let p = super::pair(
        Some(archive_side(super::Archive::Unreadable)),
        Some(archive_side(super::Archive::Listed(full.clone()))),
        None,
    );
    if p.members.is_some() || p.members_note.is_none() {
        return Err("compared with an unreadable list".into());
    }
    let mut cut = full.clone();
    cut.complete = false;
    let p = super::pair(
        Some(archive_side(super::Archive::Listed(full.clone()))),
        Some(archive_side(super::Archive::Listed(cut))),
        None,
    );
    if p.members_note.as_deref()
        != Some("The new version has a damaged member list, so its members aren't compared.")
    {
        return Err(format!("{:?}", p.members_note));
    }
    // A new archive: all its members added. An archive turned into another kind of file:
    // all removed.
    let p = super::pair(
        None,
        Some(archive_side(super::Archive::Listed(full.clone()))),
        None,
    );
    if p.members.map(|m| m.len()) != Some(1) {
        return Err("a new archive's members not added".into());
    }
    let p = super::pair(
        Some(archive_side(super::Archive::Listed(full))),
        Some(archive_side(super::Archive::No)),
        None,
    );
    if p.members.map(|m| m.len()) != Some(1) {
        return Err("members of an archive no more not removed".into());
    }
    let twice = list(&[("a", b"one", false), ("a", b"two", false)]).ok_or("no directory")?;
    let once = list(&[("a", b"one", false)]).ok_or("no directory")?;
    let p = super::pair(
        Some(archive_side(super::Archive::Listed(once))),
        Some(archive_side(super::Archive::Listed(twice))),
        None,
    );
    if p.members.as_ref().map(Vec::len) != Some(0) {
        return Err(format!("not the first compared: {:?}", p.members));
    }
    if !p
        .members_note
        .as_deref()
        .is_some_and(|n| n.contains("more than one member named a"))
    {
        return Err(format!("{:?}", p.members_note));
    }
    Ok(())
}

/// A side whose text is incomplete is compared with a note saying so; texts too unlike to
/// match line by line are said to be shown roughly.
#[test]
fn incomplete_and_approximate_text_said() -> Result<(), String> {
    let mut old = side_with(Some("a\nb"));
    old.text_gaps = vec!["slide 2 couldn't be read".to_owned()];
    let p = super::pair(Some(old), Some(side_with(Some("a\nc"))), None);
    if p.text.is_none()
        || p.text_note.as_deref()
            != Some(
                "The text of the old version is incomplete (slide 2 couldn't be read), so this comparison may show changes that aren't there.",
            )
    {
        return Err(format!("{:?}", p.text_note));
    }
    let line = |e: Edit, l: &str| (e, l.to_owned());
    let fallback = [
        line(Edit::Same, "s"),
        line(Edit::Del, "x"),
        line(Edit::Del, "y"),
        line(Edit::Add, "y"),
        line(Edit::Same, "t"),
    ];
    if !super::approximate(&fallback) {
        return Err("a fallback not told".into());
    }
    let real = [
        line(Edit::Del, "x"),
        line(Edit::Add, "z"),
        line(Edit::Same, "y"),
    ];
    if super::approximate(&real) || super::approximate(&real[..2]) {
        return Err("a real diff taken for a fallback".into());
    }
    let p = super::pair(
        Some(side_with(Some("a\nb"))),
        Some(side_with(Some("a\nc"))),
        None,
    );
    if p.text_note.is_some() {
        return Err(format!("{:?}", p.text_note));
    }
    Ok(())
}

/// A file of the working tree is read no further than the limit.
#[test]
fn local_reads_are_bounded() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-binary-local-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("f.bin");
    std::fs::write(&file, [7u8; 100]).map_err(|e| e.to_string())?;
    let small = super::read_local(&file, 99).map_err(|e| e.to_string())?;
    let exact = super::read_local(&file, 100).map_err(|e| e.to_string())?;
    let missing = super::read_local(&dir.join("none"), 100);
    let _ = std::fs::remove_dir_all(&dir);
    if small.is_some() || exact.map(|b| b.len()) != Some(100) {
        return Err("not bounded".into());
    }
    if !missing.is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Err("a missing file not told".into());
    }
    Ok(())
}

/// Text files are no binaries; an image keeps its own view.
#[test]
fn text_and_images_are_not_binaries() -> Result<(), String> {
    let dir = repo("text")?;
    commit(&dir, "a.txt", b"a\n", "a")?;
    let hash = commit(&dir, "a.txt", b"b\n", "b")?;
    let data = load(
        &dir,
        &commit_key(&hash, "a.txt"),
        DiffOptions::default(),
        true,
    )?;
    assert!(data.binary.is_none() && data.textconv.is_none());
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0";
    let hash = commit(&dir, "i.png", png, "png")?;
    let data = load(
        &dir,
        &commit_key(&hash, "i.png"),
        DiffOptions::default(),
        true,
    )?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(data.binary.is_none());
    assert!(data.image.is_some());
    Ok(())
}

fn side_with(text: Option<&str>) -> super::Side {
    super::Side {
        size: Some(10),
        kind: Some("PDF document".into()),
        facts: Vec::new(),
        head: vec![1, 2, 3],
        text: text.map(str::to_owned),
        text_gaps: Vec::new(),
        archive: super::Archive::No,
        failed: false,
    }
}

/// A side whose text can't be read isn't taken for one without text: nothing is diffed and a
/// note says why.
#[test]
fn unreadable_text_is_not_all_removed() {
    let p = super::pair(Some(side_with(Some("a\nb"))), Some(side_with(None)), None);
    assert!(p.text.is_none());
    assert_eq!(
        p.text_note.as_deref(),
        Some("The text of the new version can't be read, so it isn't compared.")
    );
    let p = super::pair(None, Some(side_with(Some("a"))), None);
    assert_eq!(p.text_changes, 1);
    assert!(p.text_note.is_none());
}

#[test]
fn sizes_of_sides_not_read() {
    use super::view::size_text;
    let mut s = side_with(None);
    assert_eq!(size_text(&s), "10 B");
    s.kind = None;
    s.size = Some(70 * 1024 * 1024);
    assert_eq!(size_text(&s), "70.0 MB, not read (over 64.0 MB)");
    s.size = None;
    assert_eq!(size_text(&s), "over 64.0 MB, not read");
}

/// The view drawn in a headless frame, every view of it: nothing may panic or lock up.
#[test]
fn the_view_draws() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let docx = super::pair(
        Some(side_with(Some("one\ntwo"))),
        Some(side_with(Some("one\nthree"))),
        None,
    );
    let new_only = super::pair(None, Some(side_with(None)), None);
    for pair in [&docx, &new_only] {
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| super::binary_view(ui, pair, "x.bin"));
            });
            out.textures_delta.clear();
        }
    }
}
