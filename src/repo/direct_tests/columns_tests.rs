use super::*;
fn prefs() -> Cols {
    Cols {
        refs: 330.0,
        graph: 100.0,
        hash: 70.0,
        author: 150.0,
        when: 100.0,
        date: 118.0,
        push: 0.0,
    }
}
#[test]
fn compression_is_reversible_and_independent_of_drag_history() {
    for width in [100.0, 400.0, 800.0, 1800.0] {
        let expected = prefs().squeezed(width, None);
        for bar in Bar::ALL {
            assert_eq!(expected, prefs().squeezed(width, Some(bar)));
        }
    }
    assert_eq!(prefs().squeezed(1800.0, None), prefs());
}
#[test]
fn a_different_divider_in_a_narrow_window_never_saves_compression() {
    let initial = prefs();
    for bar in Bar::ALL {
        let next = bar.drag(&initial, Some(Bar::Date), 30.0, 600.0);
        for other in Bar::ALL {
            if other != bar {
                assert_eq!(next.width(other), initial.width(other));
            }
        }
        assert_eq!(next.squeezed(1800.0, None).width(bar), next.width(bar));
    }
}
#[test]
fn geometry_tiles_the_table_at_every_width_and_scale() {
    for scale in [1.0, 1.25, 1.5] {
        for width in [380.0, 900.0, 1800.0] {
            let available = width / scale;
            let cols = prefs().squeezed(available, None);
            let rect = Rect::from_min_size(
                pos2(-80.0, 20.0),
                egui::vec2(cols.content_width(available), 30.0),
            );
            let cells = [
                cols.cell(rect, Bar::Refs),
                cols.cell(rect, Bar::Graph),
                cols.description(rect),
                cols.cell(rect, Bar::Hash),
                cols.cell(rect, Bar::Author),
                cols.cell(rect, Bar::When),
                cols.cell(rect, Bar::Date),
            ];
            assert!(cells[2].width() >= MIN_DESCRIPTION_W);
            for pair in cells.windows(2) {
                assert_eq!(pair[0].right(), pair[1].left());
            }
            assert_eq!(cells[6].right(), rect.right());
        }
    }
}
#[test]
fn a_blocked_drag_keeps_the_compressed_columns_preferred_width() {
    let initial = prefs();
    for bar in Bar::ALL {
        for dx in [-100.0, 100.0] {
            assert_eq!(bar.drag(&initial, None, dx, 100.0), initial);
        }
    }
}

#[test]
fn limits_hidden_columns_and_corrupt_settings_are_bounded() {
    let mut p = prefs();
    p.author = 0.0;
    p.graph = f32::NAN;
    p.refs = f32::INFINITY;
    let shown = p.squeezed(300.0, None);
    assert_eq!(shown.author, 0.0);
    assert!(shown.graph.is_finite());
    assert!(shown.refs.is_finite());
    let next = Bar::When.drag(&prefs(), None, 10000.0, 1800.0);
    assert_eq!(next.when, Bar::When.min());
    assert_eq!(next.date, prefs().date);
}
