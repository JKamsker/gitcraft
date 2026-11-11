use super::*;
fn patch(path: &str, body: &str) -> Patch {
    Patch {
        path: path.into(),
        code: 'M',
        text: format!("--- a/{path}\n+++ b/{path}\n@@ -1,5 +1,5 @@\n{body}"),
    }
}
#[test]
fn finds_an_edited_reindented_cross_file_block_with_shared_markers() -> Result<(), String> {
    let a = patch(
        "old/module.rs",
        "-fn important_validation_function(value: i32) {\n-    validate_input(value);\n-    let amount = 1;\n-    record_the_validation_result(value);\n-}\n",
    );
    let b = patch(
        "new/module.rs",
        "+    fn important_validation_function(value: i32) {\n+        validate_input(value);\n+        let amount = 2;\n+        record_the_validation_result(value);\n+    }\n",
    );
    let data = assemble(
        "test".into(),
        vec![a, b],
        vec![],
        2,
        &["a".repeat(40), "b".repeat(40)],
        &|| true,
    )?;
    assert_eq!(data.locations.len(), 1);
    let b = &data.data.moves.blocks[0];
    assert!(!b.exact);
    assert_eq!(data.locations[0][0].key.path(), "old/module.rs");
    assert_eq!(data.locations[0][1].key.path(), "new/module.rs");
    assert_eq!(data.data.moves.at(b.del.start), Some((0, true)));
    assert_eq!(data.data.moves.at(b.add.start), Some((0, true)));
    assert_eq!(
        moved_view::Opened::new(&data.data, 0)
            .ok_or("Missing moved block")?
            .edited_lines(),
        1
    );
    Ok(())
}
#[test]
fn near_limit_edited_move_is_prepared_once_and_repaints_do_no_comparison() -> Result<(), String> {
    let prefix = "long source token ".repeat(100);
    let old = (0..1000)
        .map(|i| format!("-{prefix} original_line_{i}\n"))
        .collect::<String>();
    let new = (0..1000)
        .map(|i| {
            format!(
                "+{prefix} original_line_{}\n",
                if i == 500 { 1001 } else { i }
            )
        })
        .collect::<String>();
    let mut a = patch("a.rs", &old);
    let mut b = patch("b.rs", &new);
    a.text = a.text.replace("@@ -1,5 +1,5 @@", "@@ -1,1000 +0,0 @@");
    b.text = b.text.replace("@@ -1,5 +1,5 @@", "@@ -0,0 +1,1000 @@");
    let start = std::time::Instant::now();
    let data = assemble("near limit".into(), vec![a, b], vec![], 0, &[], &|| true)?;
    eprintln!(
        "near-limit cross-file rows prepared in {:?}",
        start.elapsed()
    );
    assert!(!data.locations.is_empty());
    assert!(data.opened.iter().flatten().any(|o| o.edited_lines() > 0));
    let temp = crate::git::file_comparison::tests::Repo::new()?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(temp.0.clone(), &ctx);
    tab.open_file_comparison(Page::CodeMoves, &ctx);
    let state = tab.file_comparison.as_mut().ok_or("state")?;
    state.code_move = Some(0);
    let count = moved_view::PREPARATIONS.with(|n| n.get());
    for frame in 0..5 {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0., frame as f32 * 10.),
                    modifiers: egui::Modifiers::default(),
                }],
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    view(
                        ui,
                        state,
                        &data,
                        &crate::theme::Tokens::get(ui.ctx()),
                        &mut Vec::new(),
                    )
                });
            },
        );
        out.shapes.clear();
        out.textures_delta.clear();
    }
    assert_eq!(moved_view::PREPARATIONS.with(|n| n.get()), count);
    // Accepted immutable rows live with their request; a stale worker cannot replace them.
    let current = state.begin(true);
    state.accept(current, Ok(Data::CodeMoves(Box::new(data))));
    let epoch = state.data_request;
    state.begin(true);
    state.accept(epoch, Err("stale selected comparison".into()));
    assert!(state.data.is_none() && state.code_move.is_none());
    assert!(
        assemble(
            "cancel".into(),
            vec![patch("a", &old), patch("b", &new)],
            vec![],
            0,
            &[],
            &|| false
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn same_file_moves_are_left_to_the_existing_diff() -> Result<(), String> {
    let a = patch(
        "one.rs",
        "-fn important_validation_function() {}\n context\n+fn important_validation_function() {}\n",
    );
    assert!(
        assemble("".into(), vec![a], vec![], 0, &[], &|| true)?
            .locations
            .is_empty()
    );
    Ok(())
}
#[test]
fn paths_preserve_literal_metacharacters_and_refuse_conflicts() -> Result<(), String> {
    assert_eq!(paths(b"M\0a\n:(glob)*\0")?.0[0].0, "a\n:(glob)*");
    assert!(paths(b"U\0a\0").is_err());
    Ok(())
}
#[test]
fn reads_staged_commit_range_and_working_snapshots() -> Result<(), String> {
    use crate::git::file_comparison::tests::Repo;
    let temp = Repo::new()?;
    let p = &temp.0;
    let content = "fn important_validation_function(value: i32) {\n    validate_input(value);\n    record_the_validation_result(value);\n}\n";
    temp.write("a.rs", content)?;
    temp.commit()?;
    temp.write("a.rs", "")?;
    temp.write("b.rs", content)?;
    temp.git(&["add", "."])?;
    assert_eq!(read(p, 0, "", "", &|| true)?.locations.len(), 1);
    temp.commit()?;
    assert_eq!(read(p, 1, "", "HEAD", &|| true)?.locations.len(), 1);
    assert_eq!(read(p, 2, "HEAD~1", "HEAD", &|| true)?.locations.len(), 1);
    temp.write("a.rs", content)?;
    temp.write("b.rs", "")?;
    assert_eq!(read(p, 3, "", "", &|| true)?.locations.len(), 1);
    assert!(read(p, 3, "", "", &|| false).is_err());
    Ok(())
}
