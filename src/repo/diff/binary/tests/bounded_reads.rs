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
