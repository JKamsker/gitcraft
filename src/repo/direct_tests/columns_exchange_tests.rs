use super::*;
#[test]
fn released_space_does_not_restore_an_unrelated_compressed_preference() -> Result<(), String> {
    let initial = Cols {
        refs: 280.0,
        graph: 100.0,
        hash: 0.0,
        author: 150.0,
        when: 100.0,
        date: 0.0,
        push: 0.0,
    };
    let shown = initial.allocated(600.0, DescriptionSpace::default());
    assert_eq!((shown.refs, shown.graph), (150.0, 60.0));
    let saved = Bar::Refs.drag_shown(&initial, &shown, -20.0, 600.0);
    let space = DescriptionSpace {
        viewport: 600.0,
        width: 160.0,
        ..Default::default()
    };
    let after = saved.allocated(600.0, space);
    assert_eq!(
        (after.refs, after.graph, after.author, after.when),
        (130.0, 60.0, 150.0, 100.0)
    );
    assert_eq!(saved.graph, initial.graph);
    assert_eq!(saved.allocated(580.0, space).graph, 60.0);
    assert_eq!(space.floor(580.0), 140.0);
    assert_eq!(saved.allocated(640.0, space).graph, 100.0);
    let encoded = ron::to_string(&space).map_err(|e| e.to_string())?;
    let round_trip: DescriptionSpace = ron::from_str(&encoded).map_err(|e| e.to_string())?;
    for width in [1800.0, 380.0, 580.0, 600.0, 420.0, 600.0] {
        assert_eq!(
            saved.allocated(width, space),
            saved.allocated(width, round_trip)
        );
    }
    Ok(())
}
