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

/// A textconv driver that fails doesn't fail the diff: the file is shown as the binary it is.
#[test]
fn a_failing_textconv_falls_back_to_the_bytes() -> Result<(), String> {
    let dir = repo("textconv-fails")?;
    std::fs::write(dir.join(".gitattributes"), "*.docx diff=broken\n")
        .map_err(|e| e.to_string())?;
    // The driver comes from the repository's attributes, not the tests' blanket `!diff`.
    std::fs::write(dir.join(".git/info/attributes"), "").map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["config", "diff.broken.textconv", "false"])?;
    let name = "Spec.docx";
    commit(&dir, name, &make_docx(&["Intro", "Old plan"]), "one")?;
    let hash = commit(&dir, name, &make_docx(&["Intro", "New plan"]), "two")?;
    let data = load(&dir, &commit_key(&hash, name), DiffOptions::default(), true)?;
    let Some(binary) = data.binary else {
        return Err("the docx isn't shown as a binary".into());
    };
    let text = binary.text.unwrap_or_default();
    if data.textconv.is_some()
        || !text
            .iter()
            .any(|(e, t)| *e == Edit::Add && t.contains("New plan"))
    {
        return Err(format!("textconv {:?}", data.textconv));
    }
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
