use super::*;
use crate::repo::DiffKey;
use crate::repo::diff::{Pick, comparison_pairs, hunk_changes, parse, partial_patch, wrap};

#[test]
fn unchanged_scroll_reflow_refreshes_production_target_and_all_patches() -> Result<(), String> {
    for split in [false, true] {
        for wrapped in [false, true] {
            for change in ["width", "font", "layout", "wrap"] {
                let text = format!(
                    "@@ -1,2 +1,2 @@\n {}\n-old_first\n+new_first\n@@ -20,101 +20,101 @@\n-old_second\n+new_second\n{}",
                    "context ".repeat(400),
                    " tail\n".repeat(100)
                );
                let lines = parse(&text);
                let data = DiffData {
                    split: comparison_pairs(&lines),
                    lines,
                    ..Default::default()
                };
                let mut view = DiffView::new(DiffKey::Commit {
                    hash: "test".into(),
                    path: "f".into(),
                });
                let ctx = egui::Context::default();
                let mut result = Ok(());
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    result = (|| -> Result<(), String> {
                        let width = if change == "font" { 1200.0 } else { 280.0 };
                        let font = egui::FontId::monospace(if change == "font" { 40.0 } else { 12.0 });
                        let before = wrapped.then(|| wrap::geometry(ui, &data, split, width, &font));
                        let after_split = if change == "layout" { !split } else { split };
                        let after_wrap = if change == "wrap" { !wrapped } else { wrapped };
                        let after_width = if change == "width" { 20000.0 } else { width };
                        let after_font = egui::FontId::monospace(if change == "font" { 6.0 } else { 12.0 });
                        let after = after_wrap.then(|| wrap::geometry(ui, &data, after_split, after_width, &after_font));
                        let header = |split, g: Option<&std::sync::Arc<wrap::Geometry>>| {
                            let row = DisplayRows::new(&data, split).row_of(4).ok_or("missing fixture header row")?;
                            Ok::<f32, String>(g.map_or(row as f32 * super::super::LINE_H, |g| g.at(row)))
                        };
                        let old_y = header(split, before.as_ref())?;
                        let new_y = header(after_split, after.as_ref())?;
                        // Keep the physical offset fixed across both owner transitions.
                        let offset = if wrapped && change == "width" { 500.0 } else if wrapped && old_y > new_y + 120.0 { (old_y + new_y) / 2.0 } else { new_y + 5.0 };
                        if wrapped && old_y > new_y + 120.0 {
                            assert_eq!(scroll_hunk(&data, split, before.as_ref().map(|g| g.offsets.as_slice()), offset..offset+380.0), Some(0));
                        }
                        let id = egui::Id::new("layout-test");
                        let layout = |split, wrapped, width: f32, font: &egui::FontId, g: Option<&std::sync::Arc<wrap::Geometry>>| KeyboardLayout {
                            key: egui::Id::new((split, wrapped, width.to_bits(), font.clone())),
                            offsets: g.map(|g| g.offsets.clone()), viewport_height: 380.0,
                        };
                        let old = layout(split, wrapped, width, &font, before.as_ref());
                        assert!(old.changed(&ctx, id));
                        assert!(!old.changed(&ctx, id));
                        // An explicit first-hunk target is legal until the layout changes.
                        view.keyboard_hunk = keyboard_target(&view, &data, split, before.as_ref().map(|g| g.offsets.as_slice()), offset..offset+380.0, Some(1), false);
                        assert_eq!(view.keyboard_hunk, Some(0));
                        let new = layout(after_split, after_wrap, after_width, &after_font, after.as_ref());
                        let changed = new.changed(&ctx, id);
                        assert!(changed);
                        let target = keyboard_target(&view, &data, after_split, after.as_ref().map(|g| g.offsets.as_slice()), offset..offset+380.0, None, changed).ok_or("target")?;
                        let expected = scroll_hunk(&data, after_split, after.as_ref().map(|g| g.offsets.as_slice()), offset..offset+380.0).ok_or("visible")?;
                        assert_eq!(target, expected);
                        if new_y <= offset { assert_eq!(target, 4); }
                        assert_target_patches(&data, target)?;
                        // Reflow does not override fresh pointer intent or explicit selections.
                        view.chosen.insert(2);
                        view.anchor = Some(2);
                        assert_eq!(keyboard_target(&view, &data, after_split, None, offset..offset+380.0, Some(1), true), Some(0));
                        assert_eq!(view.chosen.iter().copied().collect::<Vec<_>>(), [2]);
                        assert_eq!(view.anchor, Some(2));
                        Ok(())
                    })();
                });
                output.textures_delta.clear();
                result?;
            }
        }
    }
    Ok(())
}

fn assert_target_patches(data: &DiffData, target: usize) -> Result<(), String> {
    for pick in [Pick::Stage, Pick::Unstage, Pick::Discard] {
        let patch = partial_patch("", &data.lines, &hunk_changes(&data.lines, target), pick)
            .ok_or("patch")?;
        let expected_patch = if target == 4 {
            format!(
                "@@ -20,101 +20,101 @@\n-old_second\n+new_second\n{}",
                " tail\n".repeat(100)
            )
        } else {
            format!(
                "@@ -1,2 +1,2 @@\n {}\n-old_first\n+new_first\n",
                "context ".repeat(400)
            )
        };
        assert_eq!(patch, expected_patch);
        assert_eq!(patch.contains("-old_second\n+new_second"), target == 4);
        assert_eq!(patch.contains("-old_first\n+new_first"), target == 0);
    }
    Ok(())
}
