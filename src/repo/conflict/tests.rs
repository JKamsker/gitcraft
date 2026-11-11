use super::*;

const FILE: &str = "a\n<<<<<<< HEAD\nours 1\n=======\ntheirs 1\n>>>>>>> feature\nb\n<<<<<<< HEAD\nx\n||||||| base\nbase\n=======\ny\n>>>>>>> feature\nc\n";

#[test]
fn an_agent_is_told_every_conflict_with_its_three_sides() {
    let msg = editor(FILE).for_agent();
    assert!(msg.contains("has 2 merge conflicts"), "{msg}");
    assert!(
        msg.contains("ours (Ours (HEAD)):\n```\nours 1\n```"),
        "{msg}"
    );
    assert!(
        msg.contains("base (what both started from):\n```\nbase\n```"),
        "{msg}"
    );
    assert!(
        msg.contains("theirs (Theirs (feature)):\n```\ny\n```"),
        "{msg}"
    );
}

#[test]
fn parses_conflicts_and_labels() {
    let (segs, ours, theirs) = parse(FILE);
    assert_eq!((ours.as_str(), theirs.as_str()), ("HEAD", "feature"));
    let conflicts: Vec<_> = segs
        .iter()
        .filter(|s| matches!(s, Segment::Conflict { .. }))
        .collect();
    assert_eq!(conflicts.len(), 2);
    assert_eq!(
        conflicts[1],
        &Segment::Conflict {
            ours: "x\n".into(),
            base: Some("base\n".into()),
            theirs: "y\n".into()
        }
    );
}

#[test]
fn renders_choices_and_keeps_undecided_markers() {
    let (segs, o, t) = parse(FILE);
    let out = render(
        &segs,
        &[Some(Choice::Theirs), Some(Choice::OursThenTheirs)],
        &[],
        (&o, &t),
    );
    assert_eq!(out, "a\ntheirs 1\nb\nx\ny\nc\n");
    let partial = render(&segs, &[Some(Choice::Ours), None], &[], (&o, &t));
    assert!(partial.starts_with("a\nours 1\nb\n<<<<<<< HEAD\nx\n"));
    assert!(partial.contains("||||||| base\nbase\n=======\ny\n>>>>>>> feature\n"));
}

#[test]
fn unterminated_conflict_stays_text() {
    // One real conflict, then a stray start marker (a fixture, a doc about git…).
    let text =
        "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nc\n<<<<<<< not a conflict\nd\n=======\ne\n";
    let (segs, o, t) = parse(text);
    let conflicts = segs
        .iter()
        .filter(|s| matches!(s, Segment::Conflict { .. }))
        .count();
    assert_eq!(conflicts, 1);
    assert_eq!(
        render(&segs, &[Some(Choice::Ours)], &[], (&o, &t)),
        "a\nx\nc\n<<<<<<< not a conflict\nd\n=======\ne\n"
    );
    // Once the real one is chosen, nothing is left to resolve.
    let mut ed = ConflictEditor {
        path: "f".into(),
        choices: vec![None],
        recorded: vec![None],
        segments: segs,
        opened: text.into(),
        result: text.into(),
        manual: false,
        ours_label: o,
        theirs_label: t,
        rebase: false,
        solving: false,
        on_disk: 1,
        generation: 0,
        saving: false,
        original: original::State::default(),
        history: Default::default(),
        reused: None,
        memo: Default::default(),
    };
    assert_eq!(ed.unresolved(), 1);
    ed.choose(0, Choice::Ours);
    assert_eq!(ed.unresolved(), 0);
}

fn editor(text: &str) -> ConflictEditor {
    let (segments, ours_label, theirs_label) = parse(text);
    let conflicts = segments
        .iter()
        .filter(|s| matches!(s, Segment::Conflict { .. }))
        .count();
    ConflictEditor {
        path: "f".into(),
        choices: vec![None; conflicts],
        recorded: vec![None; conflicts],
        segments,
        opened: text.into(),
        result: text.into(),
        manual: false,
        ours_label,
        theirs_label,
        rebase: false,
        solving: false,
        on_disk: conflicts,
        generation: 0,
        saving: false,
        original: original::State::default(),
        history: Default::default(),
        reused: None,
        memo: Default::default(),
    }
}

/// Markers left by a hand edit keep the file from being saved as resolved, also when the
/// edit removed a start marker the file had as text.
#[test]
fn hand_edits_that_leave_markers_are_unresolved() {
    let text = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nc\n<<<<<<< not a conflict\nd\n";
    let mut ed = editor(text);
    assert_eq!(ed.unresolved(), 1);
    // The stray line deleted by hand, the real conflict still there.
    ed.manual = true;
    ed.result = text.replacen("<<<<<<< not a conflict\n", "", 1);
    assert_eq!(ed.unresolved(), 1);
    assert!(
        ed.save(Path::new("/nonexistent"))
            .is_err_and(|e| e.contains("unresolved"))
    );
    // A conflict taken apart: its end marker deleted, the rest kept.
    let real = "a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> b\nc\n";
    let mut ed = editor(real);
    ed.manual = true;
    ed.result = "a\n<<<<<<< HEAD\nx\n=======\ny\nc\n".into();
    assert_eq!(ed.unresolved(), 1);
    ed.result = "a\nx\n=======\ny\nc\n".into();
    assert_eq!(ed.unresolved(), 1);
    assert!(
        ed.save(Path::new("/nonexistent"))
            .is_err_and(|e| e.contains("unresolved"))
    );
    ed.result = "a\nxy\nc\n".into();
    assert_eq!(ed.unresolved(), 0);
    // A side that has a marker-like line as text (a heading's underline) can be chosen.
    let heading = "<<<<<<< HEAD\nx\n=======\nTitles\n=======\n>>>>>>> b\n";
    let mut ed = editor(heading);
    ed.choose(0, Choice::Theirs);
    assert_eq!(ed.result, "Titles\n=======\n");
    assert_eq!(ed.unresolved(), 0);
}

/// Choosing after editing the result by hand keeps the edits, as "Back to choices" says.
#[test]
fn choices_wait_while_edited_by_hand() {
    let mut ed = editor(FILE);
    ed.choose(0, Choice::Ours);
    ed.manual = true;
    ed.result.push_str("my edit\n");
    let edited = ed.result.clone();
    ed.choose(1, Choice::Theirs);
    assert_eq!(ed.result, edited);
    assert_eq!(ed.choices, vec![Some(Choice::Ours), Some(Choice::Theirs)]);
}

#[test]
fn solved_elsewhere_leaves_the_rest_to_choose() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-solved-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let two =
        "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\nmid\n<<<<<<< HEAD\nc\n=======\nd\n>>>>>>> x\n";
    std::fs::write(dir.join("f"), two).map_err(|e| e.to_string())?;
    let mut ed = ConflictEditor::open(&dir, "f", None).ok_or("no conflicts")?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ed.on_disk, 2);
    ed.choose(0, Choice::Ours);

    // One solved: the other is offered again, the earlier choice no longer applies.
    let one = "ab\nmid\n<<<<<<< HEAD\nc\n=======\nd\n>>>>>>> x\n";
    assert_eq!(ed.take_solved(one.into()), 1);
    assert_eq!(ed.choices, vec![None]);
    assert_eq!(ed.result, one);
    assert_eq!(ed.unresolved(), 1);
    ed.choose(0, Choice::Theirs);
    assert_eq!(ed.result, "ab\nmid\nd\n");

    // All solved: nothing left to choose, ready to save.
    assert_eq!(ed.take_solved("ab\nmid\ncd\n".into()), 0);
    assert_eq!(ed.unresolved(), 0);
    Ok(())
}

/// A merge leaves the other side's link where it conflicts with a file: the editor doesn't
/// read (or later write) what it points to, which may never end (`/dev/zero`) or have
/// conflict markers of its own.
#[cfg(unix)]
#[test]
fn a_conflicted_link_is_not_followed() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-conflict-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let marked = "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n";
    std::fs::write(dir.join("elsewhere"), marked).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(dir.join("elsewhere"), dir.join("f")).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("/dev/zero", dir.join("z")).map_err(|e| e.to_string())?;
    let (link, zero) = (
        ConflictEditor::open(&dir, "f", None),
        ConflictEditor::open(&dir, "z", None),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(link.is_none() && zero.is_none());
    Ok(())
}

/// What was read or checked in the background for an editor closed since (or opened
/// again since) is not taken for the one open: its choices stay. A change seen for the one
/// open opens it again; not while it is being saved.
#[test]
fn results_for_an_earlier_editor_are_not_taken() -> Result<(), String> {
    use crate::git::{Area, StatusFile};
    use crate::repo::{DiffKey, RepoTab};
    let dir = std::env::temp_dir().join(format!("gitcraft-conflict-gen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("f"), FILE).map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let file = StatusFile {
        path: "f".to_owned(),
        old_path: None,
        code: 'U',
        area: Area::Conflicted,
    };
    tab.open_diff(DiffKey::Work(file), &ctx);
    let now = tab.conflict_generation;
    let chosen = |tab: &RepoTab| tab.conflict.as_ref().map(|ed| ed.choices.clone());
    if let Some(ed) = tab.conflict.as_mut() {
        assert_eq!(ed.generation, now);
        ed.choose(0, Choice::Ours);
    }
    let picked = Some(vec![Some(Choice::Ours), None]);

    // Read for an earlier opening: not taken.
    let earlier = ConflictEditor::open(&dir, "f", None).map(Box::new);
    tab.conflict_opened(now - 1, "f".to_owned(), earlier);
    assert_eq!(chosen(&tab), picked);
    // Changed, as seen for an earlier opening: nothing reopened.
    tab.conflict_changed(now - 1, "f".to_owned(), &ctx);
    assert_eq!(chosen(&tab), picked);
    // Nor while the one open is being saved.
    if let Some(ed) = tab.conflict.as_mut() {
        ed.saving = true;
    }
    tab.conflict_changed(now, "f".to_owned(), &ctx);
    assert_eq!(chosen(&tab), picked);
    if let Some(ed) = tab.conflict.as_mut() {
        ed.saving = false;
    }
    // Changed, as seen for the one open: opened again from the file.
    tab.conflict_changed(now, "f".to_owned(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(chosen(&tab), Some(vec![None, None]));
    assert_eq!(tab.conflict.as_ref().map(|ed| ed.generation), Some(now + 1));
    Ok(())
}

/// The save script on another machine replaces the file (keeping its mode) rather than
/// write through a link, and refuses one that is a link.
#[test]
fn the_save_script_replaces_rather_than_follows() {
    let script = save_script("~/repo/dir/it's.txt");
    assert!(
        script.starts_with("f=\"$HOME\"'/repo/dir/it'\\''s.txt'\n"),
        "{script}"
    );
    assert!(script.contains("mktemp \"$HOME\"'/repo/dir'/.gitcraft-save.XXXXXX"));
    assert!(script.contains("if [ -L \"$f\" ] || [ ! -f \"$f\" ]; then exit 4; fi"));
    assert!(script.contains("mv -f -- \"$t\" \"$f\""));
}

/// Against WSL (`GITCRAFT_TEST_WSL=<distro>`; skipped otherwise): a solved file is saved
/// with its mode, a link in its place is refused and left alone.
#[cfg(windows)]
#[test]
fn a_solved_file_is_saved_there() -> Result<(), String> {
    let Ok(distro) = std::env::var("GITCRAFT_TEST_WSL") else {
        return Ok(());
    };
    let host = crate::remote::Host::Wsl(distro);
    let dir = format!("/tmp/gitcraft-conflict-save-{}", std::process::id());
    let sh = |script: String| {
        crate::remote::exec(
            &host,
            &crate::remote::Request {
                script,
                ..crate::remote::Request::default()
            },
        )
    };
    let made = sh(format!(
        "rm -rf {dir} && mkdir -p {dir} && printf 'x\\n' > {dir}/run.sh && chmod 755 \
         {dir}/run.sh && printf 'secret\\n' > {dir}/target && ln -s target {dir}/link"
    ))?;
    assert_eq!(made.code, 0, "{}", String::from_utf8_lossy(&made.stderr));
    let repo = crate::remote::encode(&host, &dir);
    let saved = save_solved(&repo, "run.sh", "solved\n");
    let linked = save_solved(&repo, "link", "over the link\n");
    let after = sh(format!(
        "cat {dir}/run.sh; stat -c %a {dir}/run.sh; cat {dir}/target; [ -L {dir}/link ] && \
         echo link; ls -a {dir} | grep -c gitcraft-save"
    ))?;
    let _ = sh(format!("rm -rf {dir}"));
    assert_eq!(saved, Ok(()));
    assert!(linked.is_err(), "{linked:?}");
    assert_eq!(
        String::from_utf8_lossy(&after.stdout),
        "solved\n755\nsecret\nlink\n0\n"
    );
    Ok(())
}

#[test]
fn longer_runs_are_content() {
    // A Markdown heading underline and nested (longer) markers don't split the conflict.
    let text = "<<<<<<< HEAD\nTitle\n========\n<<<<<<<< inner\n=======\ny\n>>>>>>> b\n";
    let (segs, o, t) = parse(text);
    assert_eq!(
        segs,
        vec![Segment::Conflict {
            ours: "Title\n========\n<<<<<<<< inner\n".into(),
            base: None,
            theirs: "y\n".into(),
        }]
    );
    assert_eq!(render(&segs, &[Some(Choice::Theirs)], &[], (&o, &t)), "y\n");
    assert!(is_marker("<<<<<<<", '<'));
    assert!(is_marker(">>>>>>> feature", '>'));
    assert!(!is_marker("<<<<<<<x", '<'));
    assert!(!is_marker("======", '='));
}

#[test]
fn keeps_crlf_line_endings() {
    let crlf = "a\r\n<<<<<<< HEAD\r\nmine\r\n=======\r\nyours\r\n>>>>>>> b\r\nz\r\n";
    let (segs, o, t) = parse(crlf);
    assert_eq!(
        render(&segs, &[Some(Choice::Ours)], &[], (&o, &t)),
        "a\r\nmine\r\nz\r\n"
    );
}
