use super::*;
use crate::repo::DiffKey;

fn data() -> DiffData {
    let lines = [Kind::Hunk, Kind::Del, Kind::Add, Kind::Context, Kind::Fold]
        .into_iter()
        .map(|kind| DiffLine {
            kind,
            old: None,
            new: None,
            text: String::new(),
            raw: String::new(),
        })
        .collect();
    DiffData {
        lines,
        split: vec![
            (Some(0), Some(0)),
            (Some(1), Some(2)),
            (Some(3), Some(3)),
            (Some(4), Some(4)),
        ],
        ..Default::default()
    }
}

#[test]
fn split_display_rows_map_both_sides_headers_context_and_fold() {
    let data = data();
    let map = DisplayRows::new(&data, true);
    assert_eq!(map.len(), 4);
    assert_eq!(map.patch_lines(1), Some((Some(1), Some(2))));
    assert_eq!(map.row_of(1), Some(1));
    assert_eq!(map.row_of(2), Some(1));
    assert_eq!(map.patch_lines(3), Some((Some(4), Some(4))));
    assert_eq!(map.patch_lines(4), None);
    assert_eq!(map.row_of(5), None);
}

#[test]
fn unified_rows_are_identity_even_for_a_new_file() {
    let mut data = data();
    data.new_file = true;
    let map = DisplayRows::new(&data, data.shows_split(true));
    assert_eq!(map.len(), 5);
    for i in 0..map.len() {
        assert_eq!(map.row_of(i), Some(i));
        assert_eq!(map.patch_lines(i), Some((Some(i), Some(i))));
    }
}

#[test]
fn split_filler_never_becomes_a_patch_line() {
    let mut data = data();
    data.split = vec![(None, Some(2)), (Some(1), None)];
    let map = DisplayRows::new(&data, true);
    assert_eq!(map.patch_lines(0), Some((None, Some(2))));
    assert_eq!(map.patch_lines(1), Some((Some(1), None)));
    assert_eq!(map.row_of(0), None);
}

fn view() -> DiffView {
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "test".into(),
        path: "file".into(),
    });
    view.data = Some(Ok(data()));
    view
}

#[test]
fn explicit_action_selection_toggles_and_ranges_only_changed_patch_lines() {
    let mut view = view();
    pick_line(&mut view, 1, false);
    pick_line(&mut view, 3, false);
    assert_eq!(view.chosen.iter().copied().collect::<Vec<_>>(), [1]);
    pick_line(&mut view, 2, true);
    assert_eq!(view.chosen.iter().copied().collect::<Vec<_>>(), [1, 2]);
    pick_line(&mut view, 1, false);
    assert_eq!(view.chosen.iter().copied().collect::<Vec<_>>(), [2]);
    pick_line(&mut view, usize::MAX, false);
    assert_eq!(view.chosen.len(), 1);
}

#[test]
fn new_text_selection_clears_action_anchor_and_selection() {
    let mut view = view();
    pick_line(&mut view, 1, false);
    select(&mut view, None, false, true, None, false);
    assert!(view.chosen.is_empty());
    assert!(view.anchor.is_none());
    assert!(view.text_sel.is_none());
}

#[test]
fn wrapped_scroll_keeps_stage_unstage_and_discard_on_the_visible_hunk() -> Result<(), String> {
    // A tall wrapped first hunk puts a physical offset far beyond the short second hunk
    // in fixed-height coordinates, while the viewport is still inside the first one.
    for split in [false, true] {
        let lines = super::super::parse(
            "@@ -1,5 +1,5 @@\n first\n second\n third\n-old_first\n+new_first\n@@ -20 +20 @@\n-old_second\n+new_second\n",
        );
        let data = DiffData {
            split: super::super::comparison_pairs(&lines),
            lines,
            ..Default::default()
        };
        let rows = DisplayRows::new(&data, split);
        let mut offsets = vec![0.0, super::super::LINE_H];
        for row in 1..rows.len() {
            let height = if row == 1 {
                1900.0
            } else {
                super::super::LINE_H
            };
            offsets.push(offsets.last().copied().unwrap_or(0.0) + height);
        }
        let hunk = scroll_hunk(&data, split, Some(&offsets), 500.0..880.0).ok_or("missing hunk")?;
        assert_eq!(hunk, 0);
        assert_ne!(
            data.hunk_at(split, (500.0 / super::super::LINE_H) as usize + 3),
            Some(0)
        );
        let chosen = super::super::hunk_changes(&data.lines, hunk);
        for pick in [
            super::super::Pick::Stage,
            super::super::Pick::Unstage,
            super::super::Pick::Discard,
        ] {
            let patch = super::super::partial_patch("", &data.lines, &chosen, pick)
                .ok_or("missing patch")?;
            assert!(patch.contains("-old_first\n+new_first"));
            assert!(!patch.contains("second\n+new_second"));
        }
    }
    Ok(())
}

#[test]
fn reload_boundary_clears_geometry_and_keyboard_target_even_without_folds() {
    let mut view = view();
    view.keyboard_hunk = Some(0);
    view.wrap_offsets = Some(std::sync::Arc::new(vec![0.0, 1900.0]));
    view.reopen();
    assert!(view.keyboard_hunk.is_none());
    assert!(view.wrap_offsets.is_none());
}

include!("scroll_regressions.rs");
