/// The lines opened are open again when the diff is read again.
#[test]
fn opened_lines_stay_open_when_the_diff_is_read_again() -> Result<(), String> {
    let dir = repo("reload", "f.txt", &numbered(80, ""));
    let base = numbered(80, "");
    let _ = std::fs::write(
        dir.join("f.txt"),
        base.replace("línea 40\t", "línea cuarenta\t"),
    );
    let key = unstaged("f.txt");
    let mut view = view_of(&dir, &key)?;
    view.unfold(0, Unfold::Up(10));
    view.unfold(1, Unfold::Down(4));
    let shown = |view: &DiffView| -> Result<usize, String> { Ok(data(view)?.lines.len()) };
    let before = shown(&view)?;
    let request = view.next_request();
    view.arrived(request, load(&dir, &key, DiffOptions::default(), true));
    let after = shown(&view)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(before, after);
    Ok(())
}

/// A new file, a whole-file diff and a file that isn't UTF-8 have no folds.
#[test]
fn no_folds_where_there_is_nothing_to_open() -> Result<(), String> {
    let dir = repo("none", "f.txt", &numbered(40, ""));
    let _ = std::fs::write(
        dir.join("f.txt"),
        numbered(40, "").replace("línea 20\t", "x\t"),
    );
    let whole = load(
        &dir,
        &unstaged("f.txt"),
        DiffOptions {
            whole_file: true,
            ..Default::default()
        },
        true,
    )?;
    let _ = std::fs::write(dir.join("new.txt"), "a\nb\n");
    let untracked = load(
        &dir,
        &DiffKey::Work(git::StatusFile {
            path: "new.txt".to_owned(),
            old_path: None,
            code: '?',
            area: git::Area::Untracked,
        }),
        DiffOptions::default(),
        true,
    )?;
    let mut latin = numbered(40, "").into_bytes();
    latin.extend_from_slice(b"caf\xe9\n");
    let _ = std::fs::write(dir.join("f.txt"), &latin);
    let not_utf8 = load(&dir, &unstaged("f.txt"), DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(whole.context.is_none());
    assert!(untracked.context.is_none());
    assert!(not_utf8.context.is_none());
    Ok(())
}

#[test]
fn a_commented_line_in_a_fold_opens_from_the_nearer_hunk() {
    let ctx = Context {
        gaps: vec![
            Gap {
                row: 0,
                first: 1,
                last: 40,
                old_offset: 0,
                trailing: false,
                function: None,
            },
            Gap {
                row: 9,
                first: 60,
                last: 100,
                old_offset: -2,
                trailing: true,
                function: None,
            },
        ],
        ..Context::default()
    };
    // Near the first hunk (below line 40): up from it.
    assert_eq!(unfold_to(&ctx, 38, false, 3), Some((0, Unfold::Up(6))));
    // The first gap has no hunk above it.
    assert_eq!(unfold_to(&ctx, 2, false, 3), Some((0, Unfold::Up(42))));
    // Old side numbers go through the gap's offset.
    assert_eq!(unfold_to(&ctx, 60, true, 0), Some((1, Unfold::Down(3))));
    assert_eq!(unfold_to(&ctx, 50, false, 3), None);
}

/// _Show the enclosing function_ opens the lines from the function git named for the hunk.
#[test]
fn the_enclosing_function_opens_from_its_first_line() -> Result<(), String> {
    let mut src = String::new();
    for f in 0..6 {
        src.push_str(&format!("fn function_{f}(a: u32) -> u32 {{\n"));
        for k in 0..8 {
            src.push_str(&format!("    let v{k} = a + {k};\n"));
        }
        src.push_str("    a\n}\n\n");
    }
    let dir = repo("function", "a.rs", &src);
    let edited = src.replacen(
        "    a\n}\n\nfn function_2",
        "    a * 2\n}\n\nfn function_2",
        1,
    );
    let _ = std::fs::write(dir.join("a.rs"), edited);
    let key = unstaged("a.rs");
    let mut view = view_of(&dir, &key)?;
    let _ = std::fs::remove_dir_all(&dir);
    view.unfold(0, Unfold::Function);
    let d = data(&view)?;
    let first = d.lines.get(1).map(|l| l.text.as_str());
    assert_eq!(first, Some("fn function_1(a: u32) -> u32 {"));
    assert_eq!(
        d.lines[0].raw,
        "@@ -13,13 +13,13 @@ fn function_1(a: u32) -> u32 {"
    );
    Ok(())
}

/// With `core.autocrlf` the file on disk isn't what git diffed: no folds rather than context
/// that would make wrong patches.
#[test]
fn no_folds_when_the_file_on_disk_is_not_what_git_diffed() -> Result<(), String> {
    let dir = repo("autocrlf", "f.txt", &numbered(40, ""));
    git::run(Some(&dir), &["config", "core.autocrlf", "true"])?;
    let crlf = numbered(40, "\r").replace("línea 20\t", "veinte\t");
    let _ = std::fs::write(dir.join("f.txt"), crlf);
    let data = load(&dir, &unstaged("f.txt"), DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(data.lines.iter().any(|l| l.kind == Kind::Add));
    assert!(data.context.is_none());
    Ok(())
}
