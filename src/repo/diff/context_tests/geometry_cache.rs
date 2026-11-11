// Headless font measurements and the real egui temporary cache; no input/render backend.
#[test]
fn final_one_line_unfold_invalidates_cached_geometry_with_unchanged_source_and_counts()
-> Result<(), String> {
    use crate::repo::diff::{LINE_H, NUM_W, comparison_pairs, wrap};
    use std::sync::Arc;
    for split in [false, true] {
        let long = "x".repeat(3000);
        let mut lines = parse("@@ -1 +1 @@\n-old\n+new\n");
        lines.push(DiffLine {
            kind: Kind::Fold,
            old: None,
            new: None,
            text: String::new(),
            raw: String::new(),
        });
        let mut view = DiffView::new(DiffKey::Commit {
            hash: "cache".into(),
            path: "f.txt".into(),
        });
        view.data = Some(Ok(DiffData {
            split: comparison_pairs(&lines),
            spans: vec![None; lines.len()],
            changed: vec![None; lines.len()],
            lines,
            source: 42,
            context: Some(Context {
                raw: vec!["new".into(), long.clone()],
                gaps: vec![Gap {
                    row: 3,
                    first: 2,
                    last: 2,
                    old_offset: 0,
                    trailing: true,
                    function: None,
                }],
                ..Default::default()
            }),
            ..Default::default()
        }));
        let ctx = egui::Context::default();
        let mut result = Ok(());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            result = (|| -> Result<(), String> {
                let font = crate::theme::mono(12.0);
                let width = 280.0;
                let before_data = view
                    .data
                    .as_ref()
                    .ok_or("data")?
                    .as_ref()
                    .map_err(Clone::clone)?;
                let counts = (before_data.lines.len(), before_data.split.len());
                assert_eq!(counts, (4, 3));
                let before = wrap::geometry(ui, before_data, split, width, &font);
                let cached = wrap::geometry(ui, before_data, split, width, &font);
                assert!(Arc::ptr_eq(&before, &cached));
                let last = before.offsets.len() - 2;
                assert_eq!(before.height(last), LINE_H);
                view.wrap_offsets = Some(before.offsets.clone());
                view.unfold(0, Unfold::All);
                assert!(view.wrap_offsets.is_none());
                let data = view
                    .data
                    .as_ref()
                    .ok_or("data")?
                    .as_ref()
                    .map_err(Clone::clone)?;
                assert_eq!(data.source, 42);
                assert_eq!((data.lines.len(), data.split.len()), counts);
                assert_eq!(data.lines[3].kind, Kind::Context);
                assert_eq!(data.lines[3].text.len(), 3000);
                let after = wrap::geometry(ui, data, split, width, &font);
                assert!(after.height(last) > LINE_H * 10.0);
                assert!(!Arc::ptr_eq(&before, &after));
                assert!(Arc::ptr_eq(
                    &after,
                    &wrap::geometry(ui, data, split, width, &font)
                ));
                let text = wrap::galley(ui, &long, &font, after.text_width);
                assert_eq!(after.height(last), (text.size().y + 4.0).max(LINE_H));
                let rects = wrap::rectangles(&text, 0..3000);
                assert_eq!(rects.len(), text.rows.len());
                assert!(rects.len() > 10);
                for rect in rects {
                    assert_eq!(after.row(after.at(last) + rect.center().y), last);
                }
                // Turning wrapping off uses fixed rows; turning it on reuses valid geometry.
                assert_eq!(
                    wrap::logical_offset(None, last as f32 * LINE_H),
                    last as f32 * LINE_H
                );
                assert_eq!(
                    wrap::logical_offset(
                        Some(&after.offsets),
                        after.at(last) + after.height(last) / 2.0
                    ),
                    last as f32 * LINE_H
                );
                let other = wrap::geometry(ui, data, !split, width, &font);
                assert_ne!(other.text_width, after.text_width);
                let expected_width = if !split {
                    width / 2.0 - NUM_W - 24.0
                } else {
                    width - NUM_W * 2.0 - 24.0
                };
                let other_text = wrap::galley(ui, &long, &font, expected_width);
                let other_last = other.offsets.len() - 2;
                assert_eq!(
                    other.height(other_last),
                    (other_text.size().y + 4.0).max(LINE_H)
                );
                let restored = wrap::geometry(ui, data, split, width, &font);
                assert_eq!(*restored.offsets, *after.offsets);
                assert_eq!(before.height(last), LINE_H); // retained Arc is an immutable snapshot
                Ok(())
            })();
        });
        output.textures_delta.clear();
        result?;
    }
    Ok(())
}
