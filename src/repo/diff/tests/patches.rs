/// The hunks of a partial patch, without the file header.
fn hunks(lines: &[DiffLine], chosen: &[usize], pick: Pick) -> Option<String> {
    let header = patch_header(PATCH);
    let chosen: BTreeSet<usize> = chosen.iter().copied().collect();
    partial_patch(&header, lines, &chosen, pick)
        .and_then(|p| p.strip_prefix(&header).map(str::to_owned))
}

#[test]
fn partial_patch_keeps_only_the_chosen_lines() {
    let lines = parse(PATCH);
    assert_eq!(patch_header(PATCH).lines().count(), 4);
    // Indices: 0 hunk, 1 a, 2 -b, 3 -c, 4 +B, 5 +C, 6 d, 7 hunk, 8 x, 9 +y, 10 z.
    // b → B only: c stays where it was, after B.
    assert_eq!(
        hunks(&lines, &[2, 4], Pick::Stage).as_deref(),
        Some("@@ -1,4 +1,4 @@ top\n a\n-b\n+B\n c\n d\n")
    );
    // In reverse an addition left out stays as context and a removal left out goes.
    assert_eq!(
        hunks(&lines, &[2, 4], Pick::Discard).as_deref(),
        Some("@@ -1,4 +1,4 @@ top\n a\n-b\n+B\n C\n d\n")
    );
    // The second hunk alone, after the first moved nothing.
    let second: Vec<usize> = hunk_changes(&lines, 7).into_iter().collect();
    assert_eq!(
        hunks(&lines, &second, Pick::Stage).as_deref(),
        Some("@@ -10,2 +10,3 @@\n x\n+y\n z\n")
    );
    assert_eq!(hunks(&lines, &[], Pick::Stage), None);
}

#[test]
fn later_hunks_follow_the_lines_earlier_ones_add() {
    let lines = parse(PATCH);
    // Only the additions of the first hunk (+2 lines), and the second hunk.
    let patch = hunks(&lines, &[4, 5, 9], Pick::Stage).unwrap_or_default();
    assert!(patch.starts_with("@@ -1,4 +1,6 @@ top\n a\n b\n+B\n c\n+C\n d\n"));
    assert!(patch.contains("@@ -10,2 +12,3 @@\n"));
}

#[test]
fn no_newline_note_follows_its_line() {
    let text = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+b\n";
    let lines = parse(text);
    let header = patch_header(text);
    // Only the removal of the last line: its note goes with it, the addition is left out.
    let chosen: BTreeSet<usize> = [2].into();
    let patch = partial_patch(&header, &lines, &chosen, Pick::Stage).unwrap_or_default();
    assert!(patch.ends_with("@@ -1,2 +1,1 @@\n a\n-b\n\\ No newline at end of file\n"));
}

/// A line without a final newline that a partial patch puts others after: the side that goes
/// on gets the newline, or `git apply` would join the two lines ("bc", "a0d1").
#[test]
fn no_newline_note_only_ends_its_side() {
    let hunks_of = |text: &str, chosen: &[usize], pick| {
        let header = "diff --git a/f b/f\n--- a/f\n+++ b/f\n";
        let lines = parse(&format!("{header}{text}"));
        let chosen: BTreeSet<usize> = chosen.iter().copied().collect();
        partial_patch(header, &lines, &chosen, pick)
            .and_then(|p| p.strip_prefix(header).map(str::to_owned))
    };
    let note = "\\ No newline at end of file\n";
    // "a\nb" → "a\nb\nc\n", staging only "+c": b keeps its place but gets a newline.
    let grown = format!("@@ -1,2 +1,3 @@\n a\n-b\n{note}+b\n+c\n");
    assert_eq!(
        hunks_of(&grown, &[5], Pick::Stage),
        Some(format!("@@ -1,2 +1,3 @@\n a\n-b\n{note}+b\n+c\n"))
    );
    // "x\nd0\nd1\n" → "x\na0", staging only "+a0": d1 stays after it.
    let cut = format!("@@ -1,3 +1,2 @@\n x\n-d0\n-d1\n+a0\n{note}");
    assert_eq!(
        hunks_of(&cut, &[4], Pick::Stage),
        Some("@@ -1,3 +1,4 @@\n x\n d0\n+a0\n d1\n".to_owned())
    );
    // "x\nd0" → "x\na0\na1\n", discarding only "-d0": the lines kept come after it.
    let added = format!("@@ -1,2 +1,3 @@\n x\n-d0\n{note}+a0\n+a1\n");
    assert_eq!(
        hunks_of(&added, &[2], Pick::Discard),
        Some("@@ -1,4 +1,3 @@\n x\n-d0\n a0\n a1\n".to_owned())
    );
    // "d0\nd1\n" → "a0", discarding only "-d1": a0, kept, is no longer last on the old side.
    let shrunk = format!("@@ -1,2 +1 @@\n-d0\n-d1\n+a0\n{note}");
    assert_eq!(
        hunks_of(&shrunk, &[2], Pick::Discard),
        Some(format!("@@ -1,2 +1,1 @@\n-a0\n+a0\n{note}-d1\n"))
    );
}

#[test]
fn crlf_lines_keep_their_carriage_return() {
    let lines = parse("@@ -1 +1 @@\n-a\r\n+b\r\n");
    assert_eq!(lines[1].text, "a");
    assert_eq!(lines[2].raw, "+b\r");
}

#[test]
fn partial_patches_apply_with_git() {
    partial_patches_apply(false);
}

#[test]
fn partial_patches_of_the_whole_file_apply_with_git() {
    partial_patches_apply(true);
}

/// Stages, unstages and discards a few lines in a real repository.
fn partial_patches_apply(whole_file: bool) {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-partial-{}-{whole_file}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "core.autocrlf", "false"],
    ] {
        assert!(git(args).is_ok());
    }
    let file = dir.join("f.txt");
    let base: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    let _ = std::fs::write(&file, &base);
    assert!(git(&["add", "."]).is_ok());
    assert!(git(&["commit", "-q", "-m", "base"]).is_ok());
    let edited = base
        .replace("line 2\n", "line two\n")
        .replace("line 3\n", "line three\n")
        .replace("line 18\n", "line 18\nline 18b\n");
    let _ = std::fs::write(&file, &edited);

    let work = |area| git::StatusFile {
        path: "f.txt".to_owned(),
        old_path: None,
        code: 'M',
        area,
    };
    let pick = |area, which: &dyn Fn(&DiffLine) -> bool, how: Pick| -> Result<(), String> {
        let opts = DiffOptions {
            whole_file,
            ..DiffOptions::default()
        };
        let data = load(&dir, &DiffKey::Work(work(area)), opts, true)?;
        assert!(data.partial);
        let chosen = (0..data.lines.len())
            .filter(|&i| which(&data.lines[i]))
            .collect();
        let patch =
            partial_patch(&data.header, &data.lines, &chosen, how).ok_or("nothing picked")?;
        let path = git::save_patch(&patch)?.to_string_lossy().into_owned();
        let mut args = how.args().to_vec();
        args.push(&path);
        git(&args).map(|_| ())
    };
    let index = || git(&["show", ":f.txt"]).unwrap_or_default();
    // Stage line 2 → "line two" only.
    let second = |l: &DiffLine| l.text == "line 2" || l.text == "line two";
    assert_eq!(pick(Area::Unstaged, &second, Pick::Stage), Ok(()));
    assert_eq!(index(), base.replace("line 2\n", "line two\n"));
    // Discard the line added near the end.
    let added = |l: &DiffLine| l.text == "line 18b";
    assert_eq!(pick(Area::Unstaged, &added, Pick::Discard), Ok(()));
    let on_disk = std::fs::read_to_string(&file).unwrap_or_default();
    assert_eq!(
        on_disk,
        base.replace("line 2\n", "line two\n")
            .replace("line 3\n", "line three\n")
    );
    // Unstage the addition but keep the removal staged.
    let two = |l: &DiffLine| l.text == "line two";
    assert_eq!(pick(Area::Staged, &two, Pick::Unstage), Ok(()));
    assert_eq!(index(), base.replace("line 2\n", ""));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Latin-1 file is read with its odd bytes replaced: a patch of its lines would write the
/// replacements into the index, so its lines only go whole.
#[test]
fn text_that_is_not_utf8_is_not_picked_from() {
    let dir = std::env::temp_dir().join(format!("gitcraft-partial-latin1-{}", std::process::id()));
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
    let file = dir.join("f.txt");
    let _ = std::fs::write(&file, b"a\nb\n");
    assert!(git(&["add", "."]).is_ok());
    assert!(git(&["commit", "-q", "-m", "base"]).is_ok());
    let _ = std::fs::write(&file, b"a\ncaf\xe9\nb\n");
    let key = DiffKey::Work(git::StatusFile {
        path: "f.txt".to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let data = load(&dir, &key, DiffOptions::default(), true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(data.map(|d| (d.lines.len(), d.partial)), Ok((4, false)));
}

/// A file made executable and edited: picking lines leaves the mode change where it is.
#[cfg(unix)]
#[test]
fn partial_patches_leave_a_mode_change_alone() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("gitcraft-partial-mode-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "core.fileMode", "true"],
    ] {
        assert!(git(args).is_ok());
    }
    let file = dir.join("f.txt");
    let base: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    let _ = std::fs::write(&file, &base);
    assert!(git(&["add", "."]).is_ok());
    assert!(git(&["commit", "-q", "-m", "base"]).is_ok());
    let _ = std::fs::write(&file, base.replace("line 2\n", "line two\n"));
    let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
    let mode = || {
        std::fs::metadata(&file)
            .map(|m| m.permissions().mode() & 0o111)
            .unwrap_or(0)
    };
    let key = DiffKey::Work(git::StatusFile {
        path: "f.txt".to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let apply = |pick: Pick| -> Result<(), String> {
        let data = load(&dir, &key, DiffOptions::default(), true)?;
        assert!(data.partial);
        let chosen = hunk_changes(&data.lines, 0);
        let patch =
            partial_patch(&data.header, &data.lines, &chosen, pick).ok_or("nothing picked")?;
        let path = git::save_patch(&patch)?.to_string_lossy().into_owned();
        let mut args = pick.args().to_vec();
        args.push(&path);
        git(&args).map(|_| ())
    };
    assert_eq!(apply(Pick::Stage), Ok(()));
    assert_eq!(
        git(&["ls-files", "-s", "f.txt"]).map(|s| s.starts_with("100644")),
        Ok(true)
    );
    assert!(git(&["reset", "-q"]).is_ok());
    assert_eq!(apply(Pick::Discard), Ok(()));
    assert_eq!(std::fs::read_to_string(&file).ok(), Some(base));
    assert_ne!(mode(), 0, "the file is still executable");
    let _ = std::fs::remove_dir_all(&dir);
}
