use super::*;

#[test]
fn a_long_name_leaves_room_for_the_branch() {
    // All fits.
    assert_eq!(share_room(400.0, 100.0, 14.0, 80.0), (100.0, 80.0));
    // A very long name: the branch keeps about a third, the dot its room.
    let (name, status) = share_room(300.0, 900.0, 14.0, 200.0);
    assert!((status - 105.0).abs() < 0.1, "{status}");
    assert!(name + 14.0 + 10.0 + status <= 300.0 + 0.1);
    // A short branch keeps all it needs.
    let (name, status) = share_room(300.0, 900.0, 0.0, 40.0);
    assert_eq!(status, 40.0);
    assert!((name - 250.0).abs() < 0.1);
    // No room at all.
    assert_eq!(share_room(-5.0, 50.0, 14.0, 50.0), (0.0, 0.0));
}

#[test]
fn keys_act_on_what_the_search_leaves() {
    let mut f = RecentFilter::default();
    let down = SearchKeys {
        down: true,
        ..Default::default()
    };
    let enter = SearchKeys {
        enter: true,
        ..Default::default()
    };
    assert_eq!(apply_keys(&mut f, down, &[3, 5]), None);
    assert_eq!(apply_keys(&mut f, enter, &[3, 5]), Some(5));
    // The list shrank under the highlight (typed in the same frame as Enter).
    assert_eq!(apply_keys(&mut f, enter, &[7]), Some(7));
    assert_eq!(apply_keys(&mut f, enter, &[]), None);
    f.query = "x".into();
    let esc = SearchKeys {
        esc: true,
        ..Default::default()
    };
    assert_eq!(apply_keys(&mut f, esc, &[1]), None);
    assert!(f.query.is_empty());
}

/// Driven like the window: typing filters, Esc clears it (and keeps the keyboard), ↓ and
/// Enter open the picked one, also when typed in the same frame as Enter.
#[test]
fn the_search_is_driven_by_the_keyboard() {
    use egui::Event;
    struct State {
        filter: RecentFilter,
        recent: Vec<PathBuf>,
        opened: Option<PathBuf>,
    }
    let recent: Vec<PathBuf> = ["/x/alpha", "/x/beta", "/x/bravo"]
        .iter()
        .map(PathBuf::from)
        .collect();
    let state = State {
        filter: RecentFilter::default(),
        recent,
        opened: None,
    };
    let mut harness = egui_kittest::Harness::builder().build_ui_state(
        |ui, s: &mut State| {
            let keys = search_ui(ui, &mut s.filter, true);
            let matches = filter(&s.recent, &s.filter.query);
            if let Some(i) = apply_keys(&mut s.filter, keys, &matches) {
                s.opened = Some(s.recent[i].clone());
            }
        },
        state,
    );
    harness.run_steps(3);
    let typed = |h: &mut egui_kittest::Harness<'_, State>, text: &str| {
        h.input_mut().events.push(Event::Text(text.to_owned()));
        h.run_steps(2);
    };
    typed(&mut harness, "br");
    assert_eq!(harness.state().filter.query, "br");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state().filter.query, "", "Esc didn't clear it");
    // Still has the keyboard: typing goes on filtering.
    typed(&mut harness, "b");
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    assert_eq!(harness.state().opened, Some(PathBuf::from("/x/bravo")));
    // Typed and Enter in one frame: Enter goes by what was just typed.
    harness.state_mut().opened = None;
    harness.state_mut().filter.clear();
    harness.run_steps(1);
    harness
        .input_mut()
        .events
        .push(Event::Text("al".to_owned()));
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    assert_eq!(harness.state().opened, Some(PathBuf::from("/x/alpha")));
}
