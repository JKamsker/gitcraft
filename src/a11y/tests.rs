//! The names screen readers hear, the reduced-motion answers, and the keyboard moving through
//! painted widgets in a headless egui.

use egui::{Id, Key, Sense, WidgetType, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use super::labels::{self, MAX_CHARS};
use super::motion;
use super::{a11y, a11y_selected, keyboard_taken, list_has_focus, list_row_sense, list_stop};

mod repo_view;

#[test]
fn clean_makes_one_trimmed_line() {
    assert_eq!(
        labels::clean("  Merge\tbranch\n\n 'dev'  "),
        "Merge branch 'dev'"
    );
    assert_eq!(labels::clean(""), "");
    assert_eq!(labels::clean(" \n\t "), "");
    assert_eq!(labels::clean("Ünïcödé ✓ 日本語"), "Ünïcödé ✓ 日本語");
}

#[test]
fn clean_cuts_huge_text_on_a_character() {
    let exact = "é".repeat(MAX_CHARS);
    assert_eq!(
        labels::clean(&exact),
        exact,
        "the limit itself is kept whole"
    );
    let huge = "日本".repeat(10_000);
    let cut = labels::clean(&huge);
    assert_eq!(cut.chars().count(), MAX_CHARS + 1);
    assert!(cut.ends_with('…'));
    // A space at the cut doesn't stay before the ellipsis.
    let words = format!("{} next", "a".repeat(MAX_CHARS));
    assert_eq!(labels::clean(&words), format!("{}…", "a".repeat(MAX_CHARS)));
}

#[test]
fn join_leaves_out_empty_parts() {
    assert_eq!(labels::join(["a", "", "  ", "b c"]), "a, b c");
    assert_eq!(labels::join(Vec::<String>::new()), "");
}

#[test]
fn a_commit_reads_like_the_row() {
    let refs = ["branch dev, checked out".to_owned(), "tag v1".to_owned()];
    assert_eq!(
        labels::commit("a301da9", "Merge…", "Jonas", "26 minutes ago", &refs),
        "commit a301da9, Merge…, Jonas, 26 minutes ago, branch dev, checked out, tag v1"
    );
    assert_eq!(
        labels::commit("a301da9", "  ", "", "just now", &[]),
        "commit a301da9, no message, just now"
    );
    assert_eq!(
        labels::commit("b0", "Fix\nthe   thing", "Jöns Ñ", "2 days ago", &[]),
        "commit b0, Fix the thing, Jöns Ñ, 2 days ago"
    );
}

#[test]
fn the_changes_row_says_what_it_holds() {
    assert_eq!(labels::changes(0, 0, 0), "Uncommitted changes");
    assert_eq!(
        labels::changes(1, 2, 3),
        "Uncommitted changes, 1 conflict, 2 staged, 3 unstaged"
    );
    assert_eq!(
        labels::changes(2, 0, 1),
        "Uncommitted changes, 2 conflicts, 1 unstaged"
    );
}

#[test]
fn files_branches_and_counts() {
    assert_eq!(
        labels::file("src/my file.rs", 'M', None),
        "src/my file.rs, modified"
    );
    assert_eq!(labels::file("new.txt", '?', None), "new.txt, untracked");
    assert_eq!(
        labels::file("b.rs", 'R', Some("a.rs")),
        "b.rs, renamed, from a.rs"
    );
    assert_eq!(labels::file("same.rs", ' ', None), "same.rs");
    assert_eq!(
        labels::branch("main", true, 2, 0),
        "branch main, checked out, 2 ahead"
    );
    assert_eq!(
        labels::branch("feature/äpfel", false, 0, 5),
        "branch feature/äpfel, 5 behind"
    );
    assert_eq!(labels::count(1, "file", "files"), "1 file");
    assert_eq!(labels::count(0, "file", "files"), "0 files");
    assert_eq!(labels::ref_name("refs/heads/a b"), "a b");
    assert_eq!(labels::ref_name("refs/remotes/origin/x"), "origin/x");
    assert_eq!(labels::ref_name("main"), "main");
}

#[test]
fn stashes_and_shortcuts() {
    assert_eq!(
        labels::stash("stash@{2}", "On main: wip"),
        "stash 2, On main: wip"
    );
    assert_eq!(labels::stash("odd", ""), "odd");
    assert_eq!(labels::without_keys("Reload (F5)"), "Reload");
    assert_eq!(
        labels::without_keys("Undo commit x (Ctrl+Z)"),
        "Undo commit x"
    );
    assert_eq!(labels::without_keys("Redo (Ctrl+Y / Ctrl+Shift+Z)"), "Redo");
    assert_eq!(
        labels::without_keys("Close (ends the shell)"),
        "Close (ends the shell)"
    );
    assert_eq!(labels::without_keys("Fix (#42)"), "Fix (#42)");
    assert_eq!(labels::without_keys("F (F)"), "F (F)");
    assert_eq!(labels::without_keys("()"), "()");
    assert_eq!(
        labels::name("  Search commits (Ctrl+F)\n"),
        "Search commits"
    );
}

#[test]
fn sidebar_rows() {
    assert_eq!(
        labels::with_notes("branch x", &[(true, "upstream gone"), (false, "hidden")]),
        "branch x, upstream gone"
    );
    assert_eq!(labels::section("LOCAL", "2", true), "LOCAL, 2, open");
    assert_eq!(
        labels::section("folder a b", "1 item", false),
        "folder a b, 1 item, closed"
    );
    assert_eq!(
        labels::worktree(
            "/tmp/my wt",
            Some("dev"),
            Some(3),
            &[(true, "open here"), (false, "locked")]
        ),
        "worktree /tmp/my wt, dev, 3 changes, open here"
    );
    assert_eq!(
        labels::worktree("C:\\wt", None, Some(0), &[]),
        "worktree C:\\wt, detached HEAD"
    );
    assert_eq!(
        labels::pull_request(42, "Fix ✓", "jö", "fix/x", true),
        "pull request 42, Fix ✓, jö, fix/x, draft"
    );
    assert_eq!(
        labels::submodule("libs/a b", "v1.2-3-gabc", false),
        "submodule libs/a b, v1.2-3-gabc"
    );
    assert_eq!(
        labels::submodule("s", "abc", true),
        "submodule s, abc, modified"
    );
}

#[test]
fn reduced_motion_answers() {
    assert_eq!(motion::from_env(Some("1")), Some(true));
    assert_eq!(motion::from_env(Some(" On ")), Some(true));
    assert_eq!(motion::from_env(Some("false")), Some(false));
    assert_eq!(motion::from_env(Some("maybe")), None);
    assert_eq!(motion::from_env(None), None);

    assert!(motion::gnome_reduced("false\n"));
    assert!(!motion::gnome_reduced("true\n"));
    assert!(motion::kde_reduced("0\n"));
    assert!(motion::kde_reduced("0.0"));
    assert!(!motion::kde_reduced("1"));
    assert!(!motion::kde_reduced(""));
    assert!(motion::macos_reduced("1\n"));
    assert!(!motion::macos_reduced("0\n"));
    let reg = "\r\nHKEY_CURRENT_USER\\Control Panel\\Desktop\\WindowMetrics\r\n    MinAnimate    REG_SZ    0\r\n";
    assert!(motion::windows_reduced(reg));
    assert!(!motion::windows_reduced(&reg.replace("    0\r", "    1\r")));
    assert!(!motion::windows_reduced(""));
}

#[test]
fn reduced_motion_stops_animations() {
    assert_eq!(motion::shortened(true, 0.15), 0.0);
    assert_eq!(motion::shortened(false, 0.15), 0.15);
    let ctx = egui::Context::default();
    let before = ctx.global_style().animation_time;
    assert!(before > 0.0);
    motion::stop_egui_animations(&ctx, false);
    assert_eq!(ctx.global_style().animation_time, before);
    motion::stop_egui_animations(&ctx, true);
    assert_eq!(ctx.global_style().animation_time, 0.0);
    assert_eq!(ctx.style_of(egui::Theme::Light).animation_time, 0.0);
    assert_eq!(ctx.style_of(egui::Theme::Dark).animation_time, 0.0);
}

/// Painted widgets as the window has them: a row, a selectable row, and a text field.
struct Painted {
    clicks: usize,
    text: String,
    taken: Vec<bool>,
}

fn painted_ui(ui: &mut egui::Ui, state: &mut Painted) {
    for (n, name) in ["Fetch", "Pull"].iter().enumerate() {
        let (_, resp) = ui.allocate_exact_size(vec2(120.0, 24.0), Sense::click());
        if n == 0 {
            a11y(&resp, WidgetType::Button, *name);
        } else {
            a11y_selected(&resp, || format!("{name} row"), true);
        }
        if resp.clicked() {
            state.clicks += 1;
        }
    }
    ui.text_edit_singleline(&mut state.text);
    state.taken.push(keyboard_taken(ui.ctx()));
    super::end_frame(ui.ctx());
}

#[test]
fn tab_reaches_painted_widgets_and_enter_clicks_them() {
    let state = Painted {
        clicks: 0,
        text: String::new(),
        taken: Vec::new(),
    };
    let mut harness = Harness::new_ui_state(painted_ui, state);
    harness.run();
    let fetch = harness.get_by_role_and_label(egui::accesskit::Role::Button, "Fetch");
    assert!(!fetch.is_focused());

    harness.key_press(Key::Tab);
    harness.run();
    assert!(harness.get_by_label("Fetch").is_focused());
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(harness.state().clicks, 1, "Enter clicks the focused row");
    harness.key_press(Key::Space);
    harness.run();
    assert_eq!(harness.state().clicks, 2, "and so does Space");
    // A painted widget with the keyboard leaves the shortcuts free.
    assert_eq!(harness.state().taken.last(), Some(&false));

    harness.key_press(Key::Tab);
    harness.run();
    let pull = harness.get_by_label("Pull row");
    assert!(pull.is_focused());
    assert_eq!(
        pull.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "a selected row says so"
    );

    // A text field takes the keys.
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!(harness.state().taken.last(), Some(&true));

    harness.key_press_modifiers(egui::Modifiers::SHIFT, Key::Tab);
    harness.run();
    assert!(
        harness.get_by_label("Pull row").is_focused(),
        "Shift+Tab goes back"
    );
}

/// A long list with one Tab stop, the selected row, as the graph has it: each row has a dot
/// of its own (a build status) that is focusable only on the stop row. `first_in_view` stands
/// in for the stop while the selection is scrolled away.
struct List {
    selected: usize,
    first_in_view: Option<usize>,
}

const LIST_LEN: usize = 40;

fn list_ui(ui: &mut egui::Ui, state: &mut List) {
    let list = Id::new("test-list");
    let down = ui.input(|i| i.key_pressed(Key::ArrowDown));
    let up = ui.input(|i| i.key_pressed(Key::ArrowUp));
    if list_has_focus(ui.ctx(), list) {
        if down {
            state.selected = (state.selected + 1).min(LIST_LEN - 1);
        } else if up {
            state.selected = state.selected.saturating_sub(1);
        }
    }
    let (_, before) = ui.allocate_exact_size(vec2(200.0, 10.0), Sense::click());
    a11y(&before, WidgetType::Button, "before the list");
    ui.spacing_mut().item_spacing.y = 0.0;
    let stop_row = state.first_in_view.unwrap_or(state.selected);
    for i in 0..LIST_LEN {
        let stop = i == stop_row;
        ui.horizontal(|ui| {
            let (_, resp) = ui.allocate_exact_size(vec2(200.0, 10.0), list_row_sense(stop));
            a11y_selected(&resp, || format!("row {i}"), i == state.selected);
            if stop {
                list_stop(&resp, list, i == state.selected);
            }
            let (_, dot) = ui.allocate_exact_size(vec2(10.0, 10.0), list_row_sense(stop));
            a11y(&dot, WidgetType::Button, || format!("dot {i}"));
        });
    }
    let (_, after) = ui.allocate_exact_size(vec2(200.0, 10.0), Sense::click());
    a11y(&after, WidgetType::Button, "after the list");
    super::end_frame(ui.ctx());
}

fn focused_label<S>(harness: &Harness<'_, S>) -> Option<String> {
    harness
        .query_by(|n| n.is_focused())
        .and_then(|n| n.accesskit_node().label())
}

fn list_harness(selected: usize) -> Harness<'static, List> {
    Harness::builder().with_size([300.0, 700.0]).build_ui_state(
        list_ui,
        List {
            selected,
            first_in_view: None,
        },
    )
}

#[test]
fn a_long_list_is_one_tab_stop_whose_arrows_move_the_keyboard() {
    let mut harness = list_harness(3);
    harness.run();
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!(focused_label(&harness).as_deref(), Some("before the list"));
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!(
        focused_label(&harness).as_deref(),
        Some("row 3"),
        "Tab lands on the stop"
    );

    for expected in ["row 4", "row 5"] {
        harness.key_press(Key::ArrowDown);
        harness.run();
        assert_eq!(focused_label(&harness).as_deref(), Some(expected));
    }
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(focused_label(&harness).as_deref(), Some("row 4"));
    assert_eq!(harness.state().selected, 4);
    // ← / → don't throw the keyboard out of the list onto what lies beside the row.
    for key in [Key::ArrowRight, Key::ArrowLeft] {
        harness.key_press(key);
        harness.run();
        assert_eq!(focused_label(&harness).as_deref(), Some("row 4"));
    }

    // The stop row's dot, then out of the list: past the other rows and their dots.
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!(focused_label(&harness).as_deref(), Some("dot 4"));
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!(focused_label(&harness).as_deref(), Some("after the list"));
    // And the list doesn't take the keyboard back on its own.
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(harness.state().selected, 4);

    harness.key_press_modifiers(egui::Modifiers::SHIFT, Key::Tab);
    harness.run();
    assert_eq!(focused_label(&harness).as_deref(), Some("dot 4"));
    harness.key_press_modifiers(egui::Modifiers::SHIFT, Key::Tab);
    harness.run();
    assert_eq!(focused_label(&harness).as_deref(), Some("row 4"));
}

#[test]
fn scrolling_the_selection_away_does_not_move_the_keyboard() {
    let mut harness = list_harness(3);
    harness.run();
    for _ in 0..2 {
        harness.key_press(Key::Tab);
        harness.run();
    }
    assert_eq!(focused_label(&harness).as_deref(), Some("row 3"));
    // The selected row scrolls out: the first row in view becomes the stop.
    harness.state_mut().first_in_view = Some(10);
    harness.run();
    assert_ne!(focused_label(&harness).as_deref(), Some("row 10"));
}

/// Rows whose button shows only on hover, or while the keyboard is on the row or the button.
fn hover_buttons_ui(ui: &mut egui::Ui, _: &mut ()) {
    for i in 0..3 {
        let (_, row) = ui.allocate_exact_size(vec2(200.0, 20.0), Sense::click());
        a11y(&row, WidgetType::Button, || format!("row {i}"));
        let button_id = row.id.with("button");
        if super::keyboard_on(&row) || ui.memory(|m| m.has_focus(button_id)) {
            let rect = egui::Rect::from_min_size(row.rect.right_top(), vec2(20.0, 20.0));
            let button = ui.interact(rect, button_id, Sense::click());
            a11y(&button, WidgetType::Button, || format!("button {i}"));
        }
    }
    super::end_frame(ui.ctx());
}

#[test]
fn tab_reaches_a_button_shown_while_the_row_has_the_keyboard() {
    let mut harness = Harness::new_ui_state(hover_buttons_ui, ());
    harness.run();
    let mut seen = Vec::new();
    for _ in 0..4 {
        harness.key_press(Key::Tab);
        harness.run();
        seen.push(focused_label(&harness).unwrap_or_default());
    }
    assert_eq!(seen, ["row 0", "button 0", "row 1", "button 1"]);
}
