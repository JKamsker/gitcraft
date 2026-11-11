fn poll_until(
    tab: &mut crate::repo::RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&crate::repo::RepoTab) -> bool,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Through the tab, as the UI does it: the editor's bases arrive in the background, and the
/// Keep both dialog goes from reading to its preview, then writes.
#[test]
fn through_the_tab() -> Result<(), String> {
    use crate::git::{Area, StatusFile};
    use crate::repo::{DiffKey, RepoTab};
    let dir = conflicted(
        "conflict-tab",
        &[("t.txt", b"a\nkeep\nb\n"), ("pic.gif", b"GIF89a\x00base")],
        &[("t.txt", b"a\nmine\nb\n"), ("pic.gif", b"GIF89a\x00ours")],
        &[
            ("t.txt", b"a\ntheirs\nb\n"),
            ("pic.gif", b"GIF89a\x00theirs"),
        ],
        "feature",
    )?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.work.in_progress = Some(InProgress::Merge);
    let file = |path: &str| StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'U',
        area: Area::Conflicted,
    };
    tab.open_diff(DiffKey::Work(file("t.txt")), &ctx);
    // As the editor's view does on each frame: asked once.
    tab.request_conflict_bases(&ctx);
    tab.request_conflict_bases(&ctx);
    let based = poll_until(&mut tab, &ctx, |tab| {
        tab.conflict.as_ref().is_some_and(|ed| !ed.wants_bases())
    });
    let base = tab.conflict.as_ref().and_then(|ed| base_of(ed, 0));

    tab.keep_both("pic.gif".to_owned(), &ctx);
    let previewed = poll_until(&mut tab, &ctx, |tab| {
        matches!(tab.keep_both, Some(KeepBoth::Preview(_)))
    });
    tab.keep_both_write(&ctx);
    let written = poll_until(&mut tab, &ctx, |tab| {
        matches!(tab.keep_both, Some(KeepBoth::Written(_)))
    });
    let files = (
        std::fs::read(dir.join("pic.main.gif")),
        std::fs::read(dir.join("pic.feature.gif")),
    );
    // The merge aborted while the window was open: Stage both stages and removes nothing.
    let Some(KeepBoth::Written(plan)) = tab.keep_both.take() else {
        return Err("not written".to_owned());
    };
    git::run(Some(&dir), &["merge", "--abort"])?;
    tab.keep_both_stage(plan, &ctx);
    let refused = poll_until(&mut tab, &ctx, |tab| {
        tab.status
            .as_deref()
            .is_some_and(|(m, _)| m.contains("no longer in the conflict"))
    });
    let pic_kept = std::fs::read(dir.join("pic.gif"));
    let staged = git::run(Some(&dir), &["diff", "--cached", "--name-only"])?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(based && previewed && written && refused);
    assert_eq!(pic_kept.map_err(|e| e.to_string())?, b"GIF89a\x00ours");
    assert_eq!(staged, "");
    assert_eq!(base.as_deref(), Some("keep\n"));
    assert_eq!(files.0.map_err(|e| e.to_string())?, b"GIF89a\x00ours");
    assert_eq!(files.1.map_err(|e| e.to_string())?, b"GIF89a\x00theirs");
    Ok(())
}

/// The segments of a parsed file are what the card's base is looked up in.
#[test]
fn no_conflict_no_base() {
    let (segments, _, _) = super::parse("plain\n");
    assert!(matches!(segments.as_slice(), [Segment::Common(_)]));
    let state = super::original::State::new(&segments);
    assert_eq!(state.index, IndexBases::NotNeeded);
}
