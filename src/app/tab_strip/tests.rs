use super::*;

#[test]
fn a_row_is_its_tabs_and_the_gaps_between() {
    assert!((row_width(&[100.0, 50.0]) - (150.0 + GAP)).abs() < 0.01);
    assert!(row_width(&[]).abs() < 0.01);
}

#[test]
fn a_tab_spans_from_the_end_of_those_before_it() {
    assert_eq!(
        tab_span(&[100.0, 50.0, 80.0], 2),
        Some((150.0 + 2.0 * GAP, 230.0 + 2.0 * GAP))
    );
    assert_eq!(tab_span(&[100.0], 3), None);
}

#[test]
fn a_tab_is_scrolled_into_view_as_little_as_needed() {
    // In view: stays.
    assert!((scroll_to_show(100.0, 300.0, 150.0, 250.0) - 100.0).abs() < 0.01);
    // Past the right end: its right edge at the end of the view.
    assert!((scroll_to_show(0.0, 300.0, 350.0, 450.0) - 150.0).abs() < 0.01);
    // Before the start: its left edge at the start.
    assert!((scroll_to_show(200.0, 300.0, 50.0, 150.0) - 50.0).abs() < 0.01);
    // Wider than the view: its start.
    assert!((scroll_to_show(0.0, 100.0, 50.0, 400.0) - 50.0).abs() < 0.01);
}

#[test]
fn the_list_has_the_tabs_not_wholly_in_view() {
    let widths = [100.0; 5];
    // A view of 250 from 120: the second wholly, the third wholly, the rest not.
    let hidden = out_of_view(&widths, 100.0 + GAP, 2.0 * 100.0 + GAP);
    assert_eq!(hidden, [0, 3, 4]);
    assert!(out_of_view(&widths, 0.0, row_width(&widths)).is_empty());
}
