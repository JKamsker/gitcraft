use super::*;

fn exchange(
    prefs: Cols,
    space: DescriptionSpace,
    available: f32,
    bar: Bar,
    change: f32,
) -> (Cols, DescriptionSpace) {
    let shown = prefs.allocated(available, space);
    let sign = if matches!(bar, Bar::Refs | Bar::Graph) {
        1.0
    } else {
        -1.0
    };
    let target = bar.drag_shown(&shown, &shown, sign * change, available);
    if target.width(bar) == shown.width(bar) {
        return (prefs, space);
    }
    let saved = bar.drag_shown(&prefs, &shown, sign * change, available);
    let next = DescriptionSpace::exchanged(available, &prefs, &target, bar);
    let after = saved.allocated(available, next);
    for other in Bar::ALL {
        assert_eq!(
            after.width(other),
            target.width(other),
            "{available}/{bar:?}/{change}/{other:?}"
        );
        if other != bar {
            assert_eq!(saved.width(other), prefs.width(other));
        }
    }
    (saved, next)
}

#[test]
fn all_ordered_bar_pairs_exchange_both_directions_and_restore_deterministically()
-> Result<(), String> {
    let prefs = Cols {
        refs: 280.0,
        graph: 100.0,
        hash: 70.0,
        author: 280.0,
        when: 100.0,
        date: 100.0,
        push: 0.0,
    };
    let mut grows = [0; 6];
    for available in [500.0, 600.0, 760.0, 760.25, 850.0, 1000.0, 1800.0, 6000.0] {
        for donor in Bar::ALL {
            let (first, space) =
                exchange(prefs, DescriptionSpace::default(), available, donor, -20.0);
            for (index, bar) in Bar::ALL.into_iter().enumerate() {
                for change in [
                    -10000.0, -21.0, -20.0, -19.0, -1.0, 0.0, 1.0, 19.0, 20.0, 21.0, 10000.0,
                ] {
                    let before = first.allocated(available, space);
                    let (saved, next) = exchange(first, space, available, bar, change);
                    if saved.allocated(available, next).width(bar) > before.width(bar) {
                        grows[index] += 1;
                    }
                    let encoded = ron::to_string(&next).map_err(|e| e.to_string())?;
                    let restored: DescriptionSpace =
                        ron::from_str(&encoded).map_err(|e| e.to_string())?;
                    assert_eq!(next, restored);
                    let mut previous = saved.allocated(380.0, next);
                    let mut previous_description = 140.0;
                    for viewport in [
                        380.0, 500.0, 580.0, 600.0, 640.0, 760.0, 850.0, 1000.0, 1800.0, 6000.0,
                    ] {
                        let shown = saved.allocated(viewport, next);
                        assert_eq!(shown, saved.allocated(viewport, restored));
                        let rect = Rect::from_min_size(
                            pos2(0.0, 0.0),
                            egui::vec2(shown.content_width(viewport), 26.0),
                        );
                        for other in Bar::ALL {
                            assert!(shown.width(other) >= other.min());
                            assert!(shown.width(other) <= other.bounded(saved.width(other)));
                            assert!(shown.width(other) >= previous.width(other));
                        }
                        let description = shown.description(rect).width();
                        assert!(description >= MIN_DESCRIPTION_W);
                        assert!(description >= previous_description);
                        previous_description = description;
                        previous = shown;
                    }
                    // Arbitrary visits do not modify any of the saved inputs.
                    assert_eq!(
                        saved.allocated(available, next),
                        saved.allocated(available, restored)
                    );
                }
            }
        }
    }
    assert!(grows.into_iter().all(|count| count > 0));
    Ok(())
}

#[test]
fn legacy_and_invalid_compression_snapshots_keep_bounded_allocation() -> Result<(), String> {
    let legacy: DescriptionSpace =
        ron::from_str("(viewport:760.0,width:260.0)").map_err(|e| e.to_string())?;
    assert_eq!(legacy.compression, None);
    let prefs = Cols {
        refs: 5000.0,
        graph: 100.0,
        hash: 70.0,
        author: 280.0,
        when: 100.0,
        date: 0.0,
        push: 0.0,
    };
    for bad in [f32::NAN, f32::INFINITY, -1.0] {
        let invalid = DescriptionSpace {
            compression: Some([bad; 6]),
            ..legacy
        };
        for available in [100.0, 600.0, 760.0, 6000.0] {
            assert_eq!(
                prefs.allocated(available, invalid),
                prefs.allocated(available, legacy)
            );
        }
    }
    let shown = prefs.allocated(3000.0, DescriptionSpace::default());
    let target = Bar::Author.drag_shown(&shown, &shown, 20.0, 3000.0);
    let space = DescriptionSpace::exchanged(3000.0, &prefs, &target, Bar::Author);
    // Oversized raw preferences and hidden columns must not manufacture compression.
    assert_eq!(space.compression, Some([0.0; 6]));
    let mut saved = prefs;
    saved.author = target.author;
    assert_eq!(saved.allocated(3000.0, space), target);
    Ok(())
}
