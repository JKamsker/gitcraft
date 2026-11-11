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
