use super::*;

#[test]
fn dragging_another_bar_keeps_the_graph_fitted_to_its_lanes() {
    let prefs = Cols {
        refs: 200.0,
        graph: 100.0,
        hash: 0.0,
        author: 150.0,
        when: 0.0,
        date: 0.0,
        push: 0.0,
    };
    // A narrow view: the graph is squeezed on screen.
    let widths = Bar::Author.drag(&prefs, None, -10.0, 550.0);
    assert_eq!(widths.graph, prefs.graph);
    let mut next = Settings::default();
    save_drag(&mut next, Bar::Author, &prefs, &widths);
    assert_eq!(next.graph_width, 0.0);
    assert_eq!(next.author_width, widths.author);
    assert_eq!(next.last_resized, Some(Bar::Author));
    // Its own bar still sets it.
    let widths = Bar::Graph.drag(&prefs, None, 10.0, 700.0);
    save_drag(&mut next, Bar::Graph, &prefs, &widths);
    assert_eq!(next.graph_width, 110.0);
}
#[test]
fn resizing_auto_refs_fixes_the_width_without_an_initial_jump() {
    let prefs = Cols {
        refs: crate::repo::columns::MIN_REFS_W,
        graph: 60.0,
        hash: 0.0,
        author: 150.0,
        when: 100.0,
        date: 0.0,
        push: 0.0,
    };
    let mut settings = Settings::default();
    let width = Bar::Refs.drag(&prefs, None, 10.0, 900.0);
    save_drag(&mut settings, Bar::Refs, &prefs, &width);
    assert_eq!(settings.refs_width, prefs.refs + 10.0);
    assert!(!settings.auto_refs_width);
    let mut at_minimum = Settings::default();
    let width = Bar::Refs.drag(&prefs, None, -100.0, 900.0);
    save_drag(&mut at_minimum, Bar::Refs, &prefs, &width);
    assert_eq!(at_minimum.refs_width, prefs.refs);

    // Resizing another column must never persist a temporary auto-sized refs width.
    let mut settings = Settings::default();
    let width = Bar::Author.drag(&prefs, None, -100.0, 500.0);
    save_drag(&mut settings, Bar::Author, &prefs, &width);
    assert_eq!(settings.refs_width, Settings::default().refs_width);
    assert!(settings.fit_visible_refs());
}
