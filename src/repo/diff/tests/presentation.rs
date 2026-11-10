/// A picture renamed between two commits shows what it was before, not "(new file)".
#[test]
fn renamed_image_shows_its_old_picture() {
    let dir = std::env::temp_dir().join(format!("gitcraft-image-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(git(args).is_ok());
    }
    let _ = std::fs::write(dir.join("a.png"), b"\x89PNG not really\0\x01");
    assert!(git(&["add", "."]).is_ok());
    assert!(git(&["commit", "-q", "-m", "a"]).is_ok());
    assert!(git(&["mv", "a.png", "b.png"]).is_ok());
    assert!(git(&["commit", "-q", "-m", "b"]).is_ok());
    let key = DiffKey::Range {
        from: "HEAD~1".to_owned(),
        to: Some("HEAD".to_owned()),
        path: "b.png".to_owned(),
    };
    let data = load(&dir, &key, DiffOptions::default(), true);
    let image = data.ok().and_then(|d| d.image);
    assert!(
        image
            .as_ref()
            .is_some_and(|i| i.old.is_some() && i.new.is_some())
    );
    assert!(image.is_some_and(|i| i.old == i.new));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changed_part_gets_its_own_section() {
    let font = theme::mono(12.0);
    let job = line_job(
        &[(Color32::RED, "let x = 2;")],
        Some((8, 9)),
        &font,
        Color32::GREEN,
    );
    let pieces: Vec<&str> = job
        .sections
        .iter()
        .map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0])
        .collect();
    assert_eq!(pieces, vec!["let x = ", "2", ";"]);
    assert_eq!(job.sections[1].format.background, Color32::GREEN);
}

#[test]
fn changed_unicode_crosses_colour_spans_without_splitting_utf8() {
    let spans = [(Color32::RED, "aé😀"), (Color32::BLUE, "日本z")];
    let font = theme::mono(12.0);
    for (range, expected) in [
        (Some((1, 4)), "é😀日"),
        (Some((0, 99)), "aé😀日本z"),
        (Some((2, 2)), ""),
        (Some((90, 99)), ""),
        (None, ""),
    ] {
        let job = line_job(&spans, range, &font, Color32::GREEN);
        assert_eq!(job.text, "aé😀日本z");
        let marked: String = job
            .sections
            .iter()
            .filter(|s| s.format.background == Color32::GREEN)
            .map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0])
            .collect();
        assert_eq!(marked, expected);
    }
    assert!(
        line_job(&[(Color32::RED, "")], None, &font, Color32::GREEN)
            .sections
            .is_empty()
    );
}

/// Unstaging `[ab].txt` leaves `a.txt` staged: write commands take file names literally.
#[test]
fn unstaging_a_file_does_not_treat_its_name_as_a_pattern() {
    let dir = std::env::temp_dir().join(format!("gitcraft-unstage-literal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| git::run(Some(&dir), args);
    let staged = || git(&["diff", "--cached", "--name-only"]);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(git(args).is_ok());
    }
    for name in ["a.txt", "[ab].txt"] {
        let _ = std::fs::write(dir.join(name), b"x\n");
    }
    // Before the first commit (`rm --cached`), then after it (`restore --staged`).
    assert!(git(&["add", "-A"]).is_ok());
    let unstage = |head| {
        let args = unstage_args(head, "[ab].txt");
        git(&args.iter().map(String::as_str).collect::<Vec<_>>())
    };
    assert!(unstage("").is_ok());
    let before_commit = staged();
    assert!(git(&["commit", "-q", "-m", "base"]).is_ok());
    for name in ["a.txt", "[ab].txt"] {
        let _ = std::fs::write(dir.join(name), b"y\n");
    }
    assert!(git(&["add", "-A"]).is_ok());
    assert!(unstage("HEAD").is_ok());
    let after_commit = staged();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(before_commit.as_deref().map(str::trim), Ok("a.txt"));
    assert_eq!(after_commit.as_deref().map(str::trim), Ok("a.txt"));
}
