use super::*;

fn issue(number: u64, title: &str, labels: &[&str]) -> Issue {
    Issue {
        tracker: None,
        acceptance: None,
        number,
        title: title.into(),
        body: String::new(),
        url: String::new(),
        labels: labels.iter().map(|l| (*l).to_owned()).collect(),
    }
}

#[test]
fn the_filter_goes_by_number_title_and_label() -> Result<(), String> {
    let i = issue(42, "Fix Login crash", &["Bug"]);
    assert!(matches(&i, ""));
    assert!(matches(&i, "#42"));
    assert!(matches(&i, "42"));
    assert!(matches(&i, "login"));
    assert!(matches(&i, "bug"));
    assert!(!matches(&i, "#7"));
    assert!(!matches(&i, "feature"));
    Ok(())
}

#[test]
fn picking_an_issue_suggests_its_branch() -> Result<(), String> {
    let mut picker = Picker::new("GitHub", String::new());
    assert!(picker.chosen_issue().is_none());
    let list = vec![issue(42, "Fix login crash", &[]), issue(7, "Other", &[])];
    picker.listed(Ok(list.clone()));
    picker.choose(0);
    assert_eq!(picker.name, "42-fix-login-crash");
    assert_eq!(picker.chosen_issue().map(|i| i.number), Some(42));
    // One that isn't in the list picks nothing.
    picker.choose(99);
    assert_eq!(picker.chosen, Some(0));
    // A list asked again without it: nothing is picked any more.
    picker.listed(Ok(Vec::new()));
    assert!(picker.chosen_issue().is_none());
    Ok(())
}

#[test]
fn tracker_keys_choose_distinct_rows_without_forge_numbers() -> Result<(), String> {
    let make = |key: &str, id: &str| Issue {
        tracker: Some(crate::trackers::Identity {
            kind: crate::trackers::Kind::Linear,
            origin: "https://api.linear.app".into(),
            account: "me@example.org".into(),
            viewer: "viewer".into(),
            id: id.into(),
            key: key.into(),
        }),
        ..issue(0, "Work", &[])
    };
    let mut picker = Picker::new("Linear", String::new());
    picker.listed(Ok(vec![make("ENG-9", "uuid-a"), make("OPS-9", "uuid-b")]));
    picker.choose(1);
    assert_eq!(
        picker.chosen_issue().ok_or("missing selected issue")?.key(),
        "OPS-9"
    );
    assert_eq!(picker.name, "ops-9-work");
    picker.filter = "eng-9".into();
    assert_eq!(picker.shown(), [0]);
    Ok(())
}

fn tracker(key: &str, id: &str) -> Issue {
    Issue {
        tracker: Some(crate::trackers::Identity {
            kind: crate::trackers::Kind::Linear,
            origin: "https://api.linear.app".into(),
            account: "me@example.org".into(),
            viewer: "viewer".into(),
            id: id.into(),
            key: key.into(),
        }),
        ..issue(0, "Work", &[])
    }
}
fn render_rows(
    ctx: &egui::Context,
    picker: &mut Picker,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            events,
            ..Default::default()
        },
        |ui| {
            choose(ui, picker);
        },
    )
}
#[test]
fn tracker_row_ids_are_stable_and_source_qualified() -> Result<(), String> {
    let a = tracker("ENG-7", "uuid-a");
    let b = tracker("OPS-7", "uuid-b");
    assert_ne!(a.identity(), b.identity());
    let mut renamed = a.clone();
    renamed
        .tracker
        .as_mut()
        .ok_or("missing tracker identity")?
        .key = "NEW-9".into();
    renamed.title = "Renamed".into();
    assert_eq!(a.identity(), renamed.identity());
    let mut other = a.clone();
    other
        .tracker
        .as_mut()
        .ok_or("missing tracker identity")?
        .origin = "https://different.invalid".into();
    assert_ne!(a.identity(), other.identity());
    Ok(())
}
#[test]
fn multiple_tracker_rows_support_distinct_pointer_and_keyboard_selection() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut picker = Picker::new("Linear", String::new());
    picker.listed(Ok(vec![
        tracker("ENG-7", "uuid-a"),
        tracker("OPS-7", "uuid-b"),
    ]));
    for (index, key) in [(1, "OPS-7"), (0, "ENG-7"), (1, "OPS-7")] {
        let out = render_rows(&ctx, &mut picker, vec![]);
        let p = out
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    (t.galley.text() == key).then_some(t.pos + t.galley.size() * 0.5)
                } else {
                    None
                }
            })
            .ok_or("missing tracker row")?;
        out.drop_without_applying_deltas();
        let event = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        render_rows(
            &ctx,
            &mut picker,
            vec![egui::Event::PointerMoved(p), event(true)],
        )
        .drop_without_applying_deltas();
        render_rows(&ctx, &mut picker, vec![event(false)]).drop_without_applying_deltas();
        assert_eq!(picker.chosen, Some(index));
        assert_eq!(
            picker.chosen_issue().ok_or("missing selected issue")?.key(),
            key
        );
        assert_eq!(picker.name, format!("{}-work", key.to_lowercase()));
    }
    picker.focus = Some(Field::Filter);
    render_rows(&ctx, &mut picker, vec![]).drop_without_applying_deltas();
    for (key, index) in [(Key::ArrowUp, 0), (Key::ArrowDown, 1)] {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        };
        render_rows(&ctx, &mut picker, vec![event(true)]).drop_without_applying_deltas();
        render_rows(&ctx, &mut picker, vec![event(false)]).drop_without_applying_deltas();
        assert_eq!(picker.chosen, Some(index));
    }
    Ok(())
}

#[test]
fn the_filter_stays_when_asked_again() -> Result<(), String> {
    let mut picker = Picker::new("Forgejo", "crash".into());
    picker.listed(Ok(vec![
        issue(1, "Login crash", &[]),
        issue(2, "Docs", &[]),
        issue(3, "Crash on save", &["bug"]),
    ]));
    assert_eq!(picker.shown(), [0, 2]);
    picker.filter = "#2".into();
    assert_eq!(picker.shown(), [1]);
    Ok(())
}
