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
        members: None,
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
