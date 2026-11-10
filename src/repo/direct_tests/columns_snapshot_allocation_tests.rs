use super::*;
#[test]
fn every_divider_preserves_all_other_displayed_allocations_on_release() {
    let prefs = Cols {
        refs: 330.0,
        graph: 100.0,
        hash: 70.0,
        author: 150.0,
        when: 100.0,
        date: 118.0,
        push: 0.0,
    };
    for available in [500.0, 638.0, 850.0, 1000.0, 1800.0] {
        let shown = prefs.allocated(available, DescriptionSpace::default());
        for bar in Bar::ALL {
            for dx in [-20.0, 20.0] {
                let saved = bar.drag_shown(&prefs, &shown, dx, available);
                let changed = saved.width(bar) != prefs.width(bar);
                let delta = if changed {
                    saved.width(bar) - shown.width(bar)
                } else {
                    0.0
                };
                let space = if changed {
                    let target = bar.drag_shown(&shown, &shown, dx, available);
                    assert_eq!(target.width(bar) - shown.width(bar), delta);
                    DescriptionSpace::exchanged(available, &prefs, &target, bar)
                } else {
                    DescriptionSpace::default()
                };
                let after = saved.allocated(available, space);
                for other in Bar::ALL {
                    if bar == other && changed {
                        assert_eq!(after.width(bar), saved.width(bar));
                    } else {
                        assert_eq!(
                            after.width(other),
                            shown.width(other),
                            "{available}/{bar:?}/{dx} changed {other:?}"
                        );
                    }
                    if bar != other {
                        assert_eq!(saved.width(other), prefs.width(other));
                    }
                }
            }
        }
    }
}
