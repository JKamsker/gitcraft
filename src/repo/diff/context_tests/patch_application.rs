/// A repository whose `name` is a 100-line file, committed.
fn repo(tag: &str, name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-ctx-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join(name), text);
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-qm", "base"]).is_ok());
    dir
}

fn numbered(n: usize, end: &str) -> String {
    (1..=n)
        .map(|k| format!("línea {k}\twith a tab{end}\n"))
        .collect()
}

fn unstaged(name: &str) -> DiffKey {
    DiffKey::Work(git::StatusFile {
        path: name.to_owned(),
        old_path: None,
        code: 'M',
        area: git::Area::Unstaged,
    })
}

/// Applies `pick` to the lines `chosen` of `view`'s diff, as the view does.
fn apply(
    dir: &Path,
    view: &DiffView,
    chosen: &std::collections::BTreeSet<usize>,
    pick: Pick,
) -> Result<(), String> {
    let Some(Ok(data)) = &view.data else {
        return Err("no diff".to_owned());
    };
    let patch = partial_patch(&data.header, &data.lines, chosen, pick).ok_or("nothing picked")?;
    let path = git::save_patch(&patch)?.to_string_lossy().into_owned();
    let mut args = pick.args().to_vec();
    args.push(&path);
    git::run(Some(dir), &args).map(|_| ())
}

fn view_of(dir: &Path, key: &DiffKey) -> Result<DiffView, String> {
    let mut view = DiffView::new(key.clone());
    let request = view.next_request();
    view.arrived(request, load(dir, key, DiffOptions::default(), true));
    Ok(view)
}

fn data(view: &DiffView) -> Result<&DiffData, String> {
    match &view.data {
        Some(Ok(d)) => Ok(d),
        Some(Err(e)) => Err(e.clone()),
        None => Err("not read".to_owned()),
    }
}

fn check_unfolded_hunks_stage(tag: &str, end: &str) -> Result<(), String> {
    let name = "dossier à/ño file.txt";
    let base = numbered(100, end);
    let dir = repo(tag, &name.replace('/', "_"), &base);
    let name = name.replace('/', "_");
    let edited = base
        .replace(
            &format!("línea 10\twith a tab{end}\n"),
            &format!("línea diez{end}\n"),
        )
        .replace(
            &format!("línea 60\twith a tab{end}\n"),
            &format!("línea sesenta{end}\n"),
        );
    let _ = std::fs::write(dir.join(&name), &edited);
    let key = unstaged(&name);
    let mut view = view_of(&dir, &key)?;
    let gaps: Vec<(u32, u32)> = data(&view)?
        .context
        .as_ref()
        .map(|c| c.gaps.iter().map(|g| (g.first, g.last)).collect())
        .unwrap_or_default();
    assert_eq!(gaps, vec![(1, 6), (14, 56), (64, 100)]);
    // The second hunk's change is picked, then folds open around it.
    let second = data(&view)?
        .lines
        .iter()
        .position(|l| l.text == "línea sesenta")
        .ok_or("no change")?;
    view.chosen.insert(second);
    view.unfold(1, Unfold::Up(20));
    view.unfold(1, Unfold::Down(5));
    view.unfold(0, Unfold::All);
    view.unfold(2, Unfold::All);
    let d = data(&view)?;
    // The pick followed its line.
    let picked: Vec<&str> = view
        .chosen
        .iter()
        .map(|&i| d.lines[i].text.as_str())
        .collect();
    assert_eq!(picked, ["línea sesenta"]);
    let ctx = d.context.as_ref().ok_or("no context")?;
    let hidden: Vec<u32> = ctx.gaps.iter().map(Gap::hidden).collect();
    assert_eq!(hidden, vec![0, 43 - 25, 0]);
    // No fold row is left after the end, and every line is numbered as git numbers it.
    assert!(d.lines.iter().all(|l| l.kind != Kind::Fold));
    assert!(
        d.lines
            .iter()
            .any(|l| l.new == Some(100) && l.old == Some(100))
    );
    // Side by side, every line has its row, in order.
    let in_split: Vec<usize> = d
        .split
        .iter()
        .flat_map(|&(l, r)| [l, r])
        .flatten()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(in_split, (0..d.lines.len()).collect::<Vec<_>>());
    let order: Vec<usize> = d.split.iter().filter_map(|&(l, r)| l.or(r)).collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]));
    // Staging the picked line, then a whole hunk, still applies.
    let chosen = view.chosen.clone();
    apply(&dir, &view, &chosen, Pick::Stage)?;
    let first_hunk = d
        .lines
        .iter()
        .position(|l| l.kind == Kind::Hunk)
        .ok_or("no hunk")?;
    apply(
        &dir,
        &view,
        &hunk_changes(&d.lines, first_hunk),
        Pick::Stage,
    )?;
    let staged = git::run(Some(&dir), &["diff", "--cached", "--no-color"])?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(staged.contains("+línea diez"), "{staged}");
    assert!(staged.contains("+línea sesenta"), "{staged}");
    Ok(())
}

#[test]
fn hunks_and_lines_still_stage_once_folds_are_open() -> Result<(), String> {
    check_unfolded_hunks_stage("lf", "")
}

#[test]
fn hunks_still_stage_in_a_file_with_crlf_endings() -> Result<(), String> {
    check_unfolded_hunks_stage("crlf", "\r")
}

/// Opened up to its last line, a file without a newline at its end keeps git's note, and a
/// hunk still discards.
#[test]
fn the_end_of_a_file_without_a_newline_opens_with_its_note() -> Result<(), String> {
    let base = numbered(30, "");
    let base = base.trim_end_matches('\n').to_owned();
    let dir = repo("eof", "f.txt", &base);
    let _ = std::fs::write(dir.join("f.txt"), base.replace("línea 3\t", "línea tres\t"));
    let key = unstaged("f.txt");
    let mut view = view_of(&dir, &key)?;
    view.unfold(1, Unfold::All);
    let d = data(&view)?;
    let last = d.lines.last().ok_or("no lines")?;
    assert_eq!(last.kind, Kind::Meta);
    assert!(last.raw.starts_with('\\'));
    let hunk = hunk_changes(&d.lines, 0);
    apply(&dir, &view, &hunk, Pick::Discard)?;
    let now = std::fs::read_to_string(dir.join("f.txt")).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(now, base);
    Ok(())
}
