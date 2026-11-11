use super::*;
fn work_file(path: &str, area: Area) -> git::StatusFile {
    git::StatusFile {
        path: path.into(),
        old_path: None,
        code: 'M',
        area,
    }
}
#[test]
fn r2_pure_fresh_membership_staging_and_close_invalidate_old_ownership() {
    let old = working_keys(git::WorkStatus {
        unstaged: vec![work_file("a", Area::Unstaged)],
        ..Default::default()
    });
    let fresh = working_keys(git::WorkStatus {
        staged: vec![work_file("a", Area::Staged)],
        unstaged: vec![work_file("b", Area::Untracked)],
        ..Default::default()
    });
    assert_eq!(old.len(), 1);
    assert_eq!(fresh.len(), 2);
    assert!(
        fresh
            .iter()
            .any(|k| matches!(k, DiffKey::Work(f) if f.path == "a" && f.area == Area::Staged))
    );
    let mut state = ReviewState {
        open: true,
        ..Default::default()
    };
    let token = state.cancel.0.clone();
    state.invalidate();
    assert!(token.is_cancelled());
    assert!(!state.owns(0));
    assert!(state.owns(1));
    state.open = false;
    assert!(!state.owns(1));
}
#[test]
fn r2_pure_stale_hunk_refusal_and_late_callback_preserve_new_document() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/r2-pure-review".into(), &ctx);
    tab.review_document.open = true;
    tab.finish_review_hunk(0, diff::Pick::Stage, Err("source moved".into()), &ctx);
    assert!(matches!(&tab.review_document.data, Some(Err(e)) if e == "Hunk refused: source moved"));
    tab.review_document.invalidate();
    tab.review_document.data = Some(Ok(Vec::new()));
    tab.finish_review_hunk(0, diff::Pick::Stage, Ok("never-run".into()), &ctx);
    assert!(matches!(&tab.review_document.data, Some(Ok(_))));
    assert!(tab.busy_op.is_none());
}
#[test]
fn r2_pure_row_budget_and_viewport_bound_large_documents() {
    assert!(check_budget(250_001, 0).is_err());
    assert!(check_budget(0, 32 * 1024 * 1024 + 1).is_err());
    assert!(check_budget(250_000, 32 * 1024 * 1024).is_ok());
    assert_eq!(
        visible_rows(50_000, 0.0, 22000.0, 22440.0, 22.0),
        1000..1021
    );
    assert_eq!(visible_rows(3, 500.0, 0.0, 300.0, 22.0), 0..1);
}
#[test]
fn r2_pure_metadata_only_sections_describe_changes_and_open_comparison() {
    for header in [
        "old mode 100644\nnew mode 100755",
        "similarity index 100%\nrename from old.txt\nrename to new.txt",
        "similarity index 100%\ncopy from old.txt\ncopy to new.txt",
        "new file mode 100644",
        "deleted file mode 100644",
        "Binary files a/picture and b/picture differ",
    ] {
        let ctx = egui::Context::default();
        let key = DiffKey::Work(git::StatusFile {
            old_path: Some("old.txt".into()),
            ..work_file("new.txt", Area::Staged)
        });
        let mut data = diff::DiffData::default();
        data.header = header.into();
        let mut acts = Vec::new();
        let mut rect = egui::Rect::NOTHING;
        let (output, rect) = {
            let mut render = |events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        rect = file_intro(ui, &key, &data, &mut acts).rect;
                    },
                );
                output.textures_delta.clear();
                output
            };
            let output = render(Vec::new());
            (output, rect)
        };
        let text = output
            .shapes
            .iter()
            .filter_map(|s| {
                if let egui::epaint::Shape::Text(t) = &s.shape {
                    Some(t.galley.text())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(header), "{text}");
        assert!(text.contains("old.txt → new.txt"));
        assert!(text.contains("No textual hunks"));
        let at = rect.center();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 600.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    },
                ],
                ..Default::default()
            },
            |ui| {
                file_intro(ui, &key, &data, &mut acts);
            },
        );
        output.textures_delta.clear();
        assert!(
            acts.iter()
                .any(|a| matches!(a, Action::OpenDiff(k) if k == &key))
        );
    }
}
#[test]
fn a_document_hunk_stages_one_file_without_staging_its_neighbor() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("continuous-hunk")?;
    r.commit("a.rs", "old\n", "a")?;
    r.commit("b.rs", "neighbor\n", "b")?;
    std::fs::write(r.path().join("a.rs"), "new\n").map_err(|e| e.to_string())?;
    std::fs::write(r.path().join("b.rs"), "changed neighbor\n").map_err(|e| e.to_string())?;
    let status = git::status(r.path())?;
    let key = DiffKey::Work(
        status
            .unstaged
            .iter()
            .find(|f| f.path == "a.rs")
            .ok_or("a absent")?
            .clone(),
    );
    let data = diff::load(
        r.path(),
        &key,
        DiffOptions {
            no_textconv: true,
            ..Default::default()
        },
        false,
    )?;
    let hunk = data
        .lines
        .iter()
        .position(|l| l.kind == diff::Kind::Hunk)
        .ok_or("no hunk")?;
    let patch = diff::partial_patch(
        &data.header,
        &data.lines,
        &diff::hunk_changes(&data.lines, hunk),
        diff::Pick::Stage,
    )
    .ok_or("no patch")?;
    git::run_input(Some(r.path()), &["apply", "--cached", "-"], &patch)?;
    assert_eq!(r.git(&["show", ":a.rs"])?, "new\n");
    assert_eq!(r.git(&["show", ":b.rs"])?, "neighbor\n");
    Ok(())
}
#[test]
fn hunk_actions_respect_source_and_patch_capabilities() {
    let staged = DiffKey::Work(git::StatusFile {
        path: "a".into(),
        old_path: None,
        code: 'M',
        area: Area::Staged,
    });
    assert_eq!(picks(&staged, true, false), [diff::Pick::Unstage]);
    assert!(picks(&staged, false, false).is_empty());
    assert_eq!(
        picks(
            &DiffKey::Commit {
                hash: "h".into(),
                path: "a".into()
            },
            false,
            true
        ),
        [diff::Pick::Revert]
    );
    assert!(
        picks(
            &DiffKey::Range {
                from: "b".into(),
                to: Some("h".into()),
                path: "a".into()
            },
            true,
            true
        )
        .is_empty()
    );
}
