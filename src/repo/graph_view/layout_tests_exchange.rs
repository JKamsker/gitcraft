#[test]
fn all_bars_sequential_pointer_exchanges_preserve_siblings_and_origin_after_auto_fit()
-> Result<(), String> {
    use crate::repo::hang_diagnostics::{run, stage, trace};
    let _trace =
        trace("all_bars_sequential_pointer_exchanges_preserve_siblings_and_origin_after_auto_fit");
    for width in [600.0, 760.0, 1000.0, 1800.0] {
        for auto in [false, true] {
            for scale in [1.0, 1.25, 1.5] {
                for offset in [-4.0, 4.0] {
                    let _case = stage(format_args!(
                        "matrix width={width} auto={auto} scale={scale} offset={offset}"
                    ));
                    let settings = Settings {
                        show_hash: true,
                        show_date: true,
                        auto_refs_width: auto,
                        graph_width: if auto { 0.0 } else { 100.0 },
                        ui_scale: scale,
                        ..exchange_settings()
                    };
                    let mut h = run(format_args!("construct and paint harness"), || {
                        exchange_harness(&settings, width, auto)
                    });
                    assert!((painted(&h)?.header.width() - width).abs() < 0.1);
                    if auto {
                        assert_ne!(
                            painted(&h)?.prefs.refs,
                            settings.refs_width,
                            "fixture must exercise readable_viewport's actual auto width"
                        );
                        assert_eq!(Settings::get(&h.ctx).refs_width, settings.refs_width);
                    }
                    // Refs goes last so the other five gestures retain production auto refs.
                    for bar in [
                        Bar::Graph,
                        Bar::Hash,
                        Bar::Author,
                        Bar::When,
                        Bar::Date,
                        Bar::Refs,
                    ] {
                        let _bar = stage(format_args!("exchange {bar:?}"));
                        let geometry = painted(&h)?;
                        let donor = Bar::ALL
                            .into_iter()
                            .filter(|&other| other != bar && other != Bar::Refs)
                            .max_by(|&a, &b| {
                                (geometry.cols.width(a) - a.min())
                                    .total_cmp(&(geometry.cols.width(b) - b.min()))
                            })
                            .ok_or_else(|| {
                                "each divider must have a visible sibling donor".to_owned()
                            })?;
                        let donation = (geometry.cols.width(donor) - donor.min()).min(30.0);
                        assert!(donation >= 10.0);
                        release_exchange(&mut h, donor, -donation)?;
                        let start = painted(&h)?;
                        let saved = Settings::get(&h.ctx);
                        let grow = (start.cols.description(start.header).width()
                            - crate::repo::columns::MIN_DESCRIPTION_W)
                            .min(bar.max() - start.cols.width(bar));
                        let shrink = start.cols.width(bar) - bar.min();
                        let preferred_delta = start.prefs.width(bar) - start.cols.width(bar);
                        let press = press_divider(&mut h, bar, offset)?;
                        for change in [
                            10.0,
                            preferred_delta - 1.0,
                            preferred_delta,
                            preferred_delta + 1.0,
                            grow - 1.0,
                            grow,
                            grow + 1.0,
                            5000.0,
                            10.0,
                            0.0,
                            -1.0,
                            -shrink,
                            -shrink - 1.0,
                            -5000.0,
                            0.0,
                        ] {
                            pointer_delta(&mut h, press, bar, change);
                            assert_exchange(&h, start, &saved, bar, change)?;
                        }
                        let end = pointer_delta(&mut h, press, bar, 10.0);
                        pointer_button(&mut h, end, false);
                        assert_exchange(&h, start, &saved, bar, 10.0)?;
                    }
                    run(format_args!("drop harness"), || drop(h));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn sequential_exchange_survives_restart_visibility_and_viewport_visits() -> Result<(), String> {
    for auto in [false, true] {
        let settings = Settings {
            show_hash: true,
            show_date: true,
            auto_refs_width: auto,
            ..exchange_settings()
        };
        let mut h = exchange_harness(&settings, 760.0, auto);
        release_exchange(&mut h, Bar::Author, -80.0)?;
        release_exchange(&mut h, Bar::Graph, 10.0)?;
        let saved = Settings::get(&h.ctx);
        let original = painted(&h)?;
        assert!(saved.description_space.compression.is_some());
        let encoded = ron::to_string(&saved).map_err(|e| e.to_string())?;
        let restored: Settings = ron::from_str(&encoded).map_err(|e| e.to_string())?;
        assert_eq!(restored, saved);
        let mut restarted = exchange_harness(&restored, 760.0, auto);
        assert_eq!(painted(&restarted)?.cols, original.cols);
        // Each metadata visibility toggle is reversible without changing saved preferences
        // or the persisted allocation. A restart while hidden follows the same policy.
        for bar in [Bar::Hash, Bar::Author, Bar::When, Bar::Date] {
            let mut hidden = saved.clone();
            match bar {
                Bar::Hash => hidden.show_hash = false,
                Bar::Author => hidden.show_author = false,
                Bar::When => hidden.show_when = false,
                Bar::Date => hidden.show_date = false,
                _ => unreachable!(),
            }
            hidden.put_for_test(&restarted.ctx);
            restarted.run_steps(3);
            assert_eq!(painted(&restarted)?.cols.width(bar), 0.0);
            assert_eq!(Settings::get(&restarted.ctx), hidden);
            let encoded = ron::to_string(&hidden).map_err(|e| e.to_string())?;
            let restored: Settings = ron::from_str(&encoded).map_err(|e| e.to_string())?;
            let hidden_restart = exchange_harness(&restored, 760.0, auto);
            assert_eq!(painted(&hidden_restart)?.cols, painted(&restarted)?.cols);
            saved.put_for_test(&restarted.ctx);
            restarted.run_steps(3);
            assert_eq!(painted(&restarted)?.cols, original.cols);
        }
        let mut visits = std::collections::HashMap::new();
        for width in [
            380, 500, 600, 740, 760, 780, 1000, 1800, 600, 380, 1800, 760,
        ] {
            restarted.set_size(vec2(width as f32 + 16.0, 120.0));
            restarted.run_steps(4);
            let geometry = painted(&restarted)?;
            assert_eq!(Settings::get(&restarted.ctx), saved);
            if let Some(previous) = visits.insert(width, geometry.cols) {
                assert_eq!(geometry.cols, previous);
            }
            if width == 1800 {
                for bar in Bar::ALL {
                    assert_eq!(
                        geometry.cols.width(bar),
                        bar.bounded(geometry.prefs.width(bar))
                    );
                }
            }
            if width == 380 {
                for bar in Bar::ALL {
                    assert_eq!(geometry.cols.width(bar), bar.min());
                }
                assert_eq!(geometry.cols.description(geometry.header).width(), 140.0);
            }
        }
        assert_eq!(painted(&restarted)?.cols, original.cols);
        restarted.set_size(vec2(516.0, 120.0));
        restarted.run_steps(3);
        let blocked = painted(&restarted)?;
        assert_eq!(blocked.cols.description(blocked.header).width(), 140.0);
        for bar in Bar::ALL {
            let press = press_divider(&mut restarted, bar, 4.0)?;
            for change in [10.0, 5000.0, -10.0, -5000.0, 0.0] {
                pointer_delta(&mut restarted, press, bar, change);
                assert_exchange(&restarted, blocked, &saved, bar, change)?;
            }
            pointer_button(&mut restarted, press, false);
        }
        // Reset clears the full reservation while preserving visibility choices.
        let mut reset = saved.clone();
        reset.show_author = false;
        reset.reset_columns();
        assert_eq!(reset.description_space, Default::default());
        assert!(!reset.show_author);
    }
    Ok(())
}

#[test]
fn automatic_refs_and_graph_modes_follow_production_inputs_after_an_exchange() -> Result<(), String>
{
    let settings = Settings {
        auto_refs_width: true,
        graph_width: 0.0,
        ..exchange_settings()
    };
    let mut h = exchange_harness(&settings, 760.0, true);
    let initial = painted(&h)?;
    assert_ne!(initial.prefs.refs, settings.refs_width);
    release_exchange(&mut h, Bar::Author, -80.0)?;
    let start = painted(&h)?;
    let saved = Settings::get(&h.ctx);
    assert!(saved.fit_visible_refs());
    assert_eq!(saved.graph_width, 0.0);
    let press = press_divider(&mut h, Bar::Graph, -4.0)?;
    for change in [10.0, 5000.0, 10.0, 0.0] {
        pointer_delta(&mut h, press, Bar::Graph, change);
        let now = Settings::get(&h.ctx);
        if change == 0.0 {
            assert_eq!(now.graph_width, 0.0);
            assert_eq!(now.description_space, saved.description_space);
            assert_eq!(painted(&h)?.cols, start.cols);
        } else {
            assert!(now.graph_width > 0.0);
        }
        assert_eq!(now.refs_width, saved.refs_width);
        assert!(now.fit_visible_refs());
    }
    pointer_button(&mut h, press, false);
    // Actual visible-pill measurements, rather than the raw default width, remain active.
    for name in ["a-much-longer-visible-branch-name-for-auto-sizing", "a"] {
        Arc::make_mut(&mut h.state_mut().commits[0]).refs = vec![name.into()];
        h.state_mut().rebuild_graph_labels();
        h.run_steps(4);
        assert_eq!(Settings::get(&h.ctx).refs_width, settings.refs_width);
        assert_eq!(Settings::get(&h.ctx).graph_width, 0.0);
        assert!(Settings::get(&h.ctx).fit_visible_refs());
        if name == "a" {
            assert_eq!(painted(&h)?.prefs.refs, initial.prefs.refs);
            assert_eq!(painted(&h)?.cols, start.cols);
        } else {
            assert!(painted(&h)?.prefs.refs > initial.prefs.refs);
        }
    }
    // A lane-fit change also remains automatic and is reversible with the same reservation.
    let width = h.state().graph.width;
    h.state_mut().graph.width = 1;
    h.run_steps(3);
    assert_eq!(Settings::get(&h.ctx).graph_width, 0.0);
    assert!(painted(&h)?.prefs.graph < start.prefs.graph);
    h.state_mut().graph.width = width;
    h.run_steps(3);
    assert_eq!(painted(&h)?.cols, start.cols);
    Ok(())
}

#[test]
fn full_height_minimap_reservation_keeps_visible_date_divider_interactive() -> Result<(), String> {
    let settings = Settings {
        show_hash: true,
        show_date: true,
        ..exchange_settings()
    };
    let mut h = exchange_harness_height(&settings, 1100.0, false, 540.0);
    let start = painted(&h)?;
    assert_eq!(
        start.header.width(),
        1016.0,
        "the minimap owns its separate 84-point strip"
    );
    let saved = Settings::get(&h.ctx);
    let press = press_divider(&mut h, Bar::Date, -4.0)?;
    let end = pointer_delta(&mut h, press, Bar::Date, 10.0);
    pointer_button(&mut h, end, false);
    assert_exchange(&h, start, &saved, Bar::Date, 10.0)?;
    Ok(())
}
