// Patch-output regressions for physical keyboard targeting, independent of rendering.
#[test]
fn tall_context_keeps_all_three_operations_on_its_enclosing_hunk() -> Result<(), String> {
    use super::super::{LINE_H, Pick, comparison_pairs, hunk_changes, parse, partial_patch};
    for split in [false, true] {
        for height in [400.0, 900.0, 1900.0, 8000.0] {
            let lines = parse(
                "@@ -1,2 +1,2 @@\n long_context\n-old_first\n+new_first\n@@ -20 +20 @@\n-old_second\n+new_second\n",
            );
            let data = DiffData {
                split: comparison_pairs(&lines),
                lines,
                ..Default::default()
            };
            let rows = DisplayRows::new(&data, split);
            let mut offsets = vec![0.0];
            for row in 0..rows.len() {
                offsets.push(
                    offsets.last().copied().unwrap_or(0.0) + if row == 1 { height } else { LINE_H },
                );
            }
            let second = offsets[rows.row_of(4).ok_or("missing header")?];
            // Includes the reviewer's exact 500..880 viewport and 1957/1938 headers.
            let viewport = if height == 1900.0 {
                500.0..880.0
            } else {
                height / 3.0..height * 0.8
            };
            assert!(second > viewport.end);
            let hunk = scroll_hunk(&data, split, Some(&offsets), viewport).ok_or("missing hunk")?;
            for pick in [Pick::Stage, Pick::Unstage, Pick::Discard] {
                let patch = partial_patch("", &data.lines, &hunk_changes(&data.lines, hunk), pick)
                    .ok_or("missing patch")?;
                assert_eq!(
                    patch,
                    "@@ -1,2 +1,2 @@\n long_context\n-old_first\n+new_first\n"
                );
            }
            // A nearby header wins inside the physical lookahead, in either layout.
            assert_eq!(
                scroll_hunk(&data, split, Some(&offsets), second - 40.0..second + 100.0),
                Some(4)
            );
            // A header at the bottom edge is outside the viewport, even if within lookahead.
            assert_eq!(
                scroll_hunk(&data, split, Some(&offsets), second - 20.0..second),
                Some(0)
            );
            assert_eq!(
                scroll_hunk(&data, split, Some(&offsets), second..second),
                None
            );
        }
        let data = DiffData {
            lines: parse("@@ -1 +1 @@\n-a\n+b\n@@ -20 +20 @@\n-c\n+d\n"),
            ..Default::default()
        };
        let data = DiffData {
            split: comparison_pairs(&data.lines),
            ..data
        };
        let header_y = DisplayRows::new(&data, split).row_of(3).ok_or("header")? as f32 * LINE_H;
        assert_eq!(
            scroll_hunk(&data, split, None, header_y - 30.0..header_y + 100.0),
            Some(3)
        );
        assert_eq!(
            scroll_hunk(&data, split, None, header_y - 20.0..header_y),
            Some(0)
        );
    }
    Ok(())
}
