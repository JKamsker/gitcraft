use super::*;
use std::sync::Arc;

#[test]
fn reserved_minimap_is_painted_outside_horizontal_graph_clip() -> Result<(), String> {
    let ctx = crate::theme::test_paint::context();
    let mut tab = RepoTab::new_deferred("/nonexistent/minimap-merge-recording".into(), &ctx);
    tab.commits = (0..100)
        .map(|i| {
            Arc::new(crate::git::Commit {
                hash: format!("{i:040x}"),
                parents: if i < 99 {
                    vec![format!("{:040x}", i + 1)]
                } else {
                    vec![]
                },
                summary: format!("supplied row {i}"),
                ..Default::default()
            })
        })
        .collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.history = tab.commits.clone();
    let model = build(&tab.graph, "", &Default::default(), &|| true)?;
    tab.history_minimap.dirty = false;
    tab.history_minimap.bins = model.bins;
    tab.history_minimap.rows = model.rows;
    egui_extras::install_image_loaders(&ctx);
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 500.0),
            )),
            ..Default::default()
        },
        |ui| {
            tab.graph_rows_view(ui, &mut vec![]);
        },
    );
    let label = crate::i18n::text("repo-core-2497");
    let text = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                Some((t.pos, t.galley.size(), s.clip_rect))
            }
            _ => None,
        })
        .ok_or("minimap title was not painted")?;
    let bounds = Rect::from_min_size(text.0, text.1);
    assert!(
        text.2.contains_rect(bounds),
        "minimap title {bounds:?} clipped by {:?}",
        text.2
    );
    assert!(
        bounds.left() > 780.0,
        "not the reserved right-hand strip: {bounds:?}"
    );
    assert_eq!(tab.commits.len(), 100);
    output.drop_without_applying_deltas();
    Ok(())
}
