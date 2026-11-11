//! Back and forward: what each state of the big area is as a place, the keys and buttons that
//! step, and stepping through a real repository's views.

use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, RawInput};
use egui_kittest::kittest::Queryable;

use super::super::nav_history::NavHistory;
use super::{View, keyboard_step, nav_step, record};
use crate::git;
use crate::repo::file_view::{FileTab, FileView};
use crate::repo::{Action, DiffKey, DiffView, RepoTab, WIP_HASH};
use crate::settings::Settings;

/// A tab on no repository, for what its state maps to.
fn bare_tab() -> RepoTab {
    let ctx = egui::Context::default();
    RepoTab::new_deferred("/nonexistent/nav-test".into(), &ctx)
}

fn commit_key(hash: &str, path: &str) -> DiffKey {
    DiffKey::Commit {
        hash: hash.to_owned(),
        path: path.to_owned(),
    }
}

fn work_file(path: &str, area: git::Area) -> git::StatusFile {
    git::StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'M',
        area,
    }
}

/// A diff's place, with the commit selected meanwhile.
fn diff_at(key: DiffKey, selected: Option<&str>) -> View {
    View::Diff {
        key,
        selected: selected.map(str::to_owned),
    }
}

fn graph(selected: Option<&str>) -> View {
    View::Graph {
        selected: selected.map(str::to_owned),
    }
}

/// The diff shows below the graph from now on, for this context.
fn below_the_graph(ctx: &egui::Context, on: bool) {
    let mut settings = Settings::get(ctx);
    settings.diff_beside_graph = on;
    settings.put_for_test(ctx);
}

#[test]
fn nothing_open_is_the_graph_with_its_selection() {
    let tab = bare_tab();
    assert_eq!(tab.current_view(), Some(graph(None)));
}

#[test]
fn an_open_diff_is_the_diff() {
    let mut tab = bare_tab();
    let key = commit_key("abc", "dir with space/ünï.txt");
    tab.diff = Some(DiffView::new(key.clone()));
    assert_eq!(tab.current_view(), Some(diff_at(key, None)));
    let work = DiffKey::Work(work_file("a b.rs", git::Area::Unstaged));
    tab.diff = Some(DiffView::new(work.clone()));
    assert_eq!(tab.current_view(), Some(diff_at(work, None)));
}

#[test]
fn a_file_view_is_its_path_rev_and_tab() {
    let mut tab = bare_tab();
    tab.file_view = Some(FileView::new(
        "söme/file.txt",
        Some("abc".into()),
        FileTab::Blame,
    ));
    assert_eq!(
        tab.current_view(),
        Some(View::File {
            path: "söme/file.txt".into(),
            rev: Some("abc".into()),
            tab: FileTab::Blame,
        })
    );
    // A diff over it (not beside its history) is what shows.
    let key = commit_key("def", "söme/file.txt");
    tab.diff = Some(DiffView::new(key.clone()));
    assert_eq!(tab.current_view(), Some(diff_at(key, None)));
}

#[test]
fn editors_are_not_places() {
    let mut tab = bare_tab();
    // A conflicted file whose editor is still being read.
    tab.diff = Some(DiffView::new(DiffKey::Work(work_file(
        "c.txt",
        git::Area::Conflicted,
    ))));
    assert_eq!(tab.current_view(), None);
}

#[test]
fn builds_and_pull_requests_are_places() {
    let ctx = egui::Context::default();
    let mut tab = bare_tab();
    tab.open_pr(7, &ctx);
    assert_eq!(tab.current_view(), Some(View::Pr(7)));
    // A file of the pull request belongs to it.
    tab.diff = Some(DiffView::new(DiffKey::Range {
        from: "a".into(),
        to: Some("b".into()),
        path: "x.rs".into(),
    }));
    assert_eq!(tab.current_view(), Some(View::Pr(7)));
    tab.diff = None;
    tab.open_ci("0123456789abcdef".into(), None, &ctx);
    assert_eq!(
        tab.current_view(),
        Some(View::Ci("0123456789abcdef".into()))
    );
}

#[test]
fn selecting_in_the_graph_refines_its_place() {
    let mut h = NavHistory::with_capacity(10);
    record(&mut h, graph(None), false);
    record(&mut h, graph(Some("a")), false);
    record(&mut h, graph(Some("b")), false);
    assert_eq!(h.entries(), &[graph(Some("b"))]);
    let diff = diff_at(commit_key("b", "f"), Some("b"));
    record(&mut h, diff.clone(), false);
    record(&mut h, diff.clone(), false);
    // Commits selected above the same diff (below the graph) refine its place.
    record(&mut h, diff_at(commit_key("b", "f"), Some("c")), false);
    let diff = diff_at(commit_key("b", "f"), Some("c"));
    record(&mut h, graph(Some("c")), false);
    assert_eq!(
        h.entries(),
        &[graph(Some("b")), diff.clone(), graph(Some("c"))]
    );
    // Back on the first graph place, selecting keeps the diff forward.
    h.go(-1);
    h.go(-1);
    record(&mut h, graph(Some("d")), false);
    assert_eq!(h.current(), Some(&graph(Some("d"))));
    assert_eq!(h.peek(1), Some(&diff));
}

#[test]
fn following_a_link_in_the_graph_is_a_step() {
    let mut h = NavHistory::with_capacity(10);
    record(&mut h, graph(Some("a")), false);
    record(&mut h, graph(Some("b")), true);
    record(&mut h, graph(Some("c")), true);
    // The same commit again is no step.
    record(&mut h, graph(Some("c")), true);
    assert_eq!(
        h.entries(),
        &[graph(Some("a")), graph(Some("b")), graph(Some("c"))]
    );
    // A click after the link refines the link's place.
    record(&mut h, graph(Some("d")), false);
    assert_eq!(h.current(), Some(&graph(Some("d"))));
    assert_eq!(h.entries().len(), 3);
}

#[test]
fn labels_say_where_a_step_goes() {
    let label = |v: View| v.label(false);
    assert_eq!(label(graph(None)), "the graph");
    assert_eq!(
        label(graph(Some("fc02e181f5672558"))),
        "the graph at fc02e181"
    );
    assert_eq!(label(graph(Some(WIP_HASH))), "the graph at the changes");
    assert_eq!(graph(Some("abc")).label(true), "the graph without the diff");
    assert_eq!(
        label(diff_at(commit_key("fc02e181f567", "dir/ünï cödé.rs"), None)),
        "ünï cödé.rs in fc02e181"
    );
    let work = |area| diff_at(DiffKey::Work(work_file("src/mod.rs", area)), None);
    assert_eq!(
        label(work(git::Area::Unstaged)),
        "the unstaged changes of mod.rs"
    );
    assert_eq!(
        label(work(git::Area::Staged)),
        "the staged changes of mod.rs"
    );
    assert_eq!(label(work(git::Area::Conflicted)), "the conflict in mod.rs");
    assert_eq!(label(work(git::Area::Untracked)), "the new file mod.rs");
    let range = |to: Option<&str>| {
        diff_at(
            DiffKey::Range {
                from: "aaaaaaaaaa".into(),
                to: to.map(str::to_owned),
                path: "a b.txt".into(),
            },
            None,
        )
    };
    assert_eq!(
        label(range(Some("bbbbbbbbbb"))),
        "a b.txt from aaaaaaaa to bbbbbbbb"
    );
    assert_eq!(
        label(range(None)),
        "a b.txt from aaaaaaaa to the working tree"
    );
    let file = |rev: Option<&str>, tab| View::File {
        path: "docs/README.md".into(),
        rev: rev.map(str::to_owned),
        tab,
    };
    assert_eq!(
        label(file(None, FileTab::History)),
        "the history of README.md"
    );
    assert_eq!(
        label(file(Some("0123456789"), FileTab::Blame)),
        "the blame of README.md at 01234567"
    );
    assert_eq!(label(View::Pr(3)), "pull request #3");
    assert_eq!(
        label(View::Ci("0123456789".into())),
        "the builds of 01234567"
    );
}

/// What `nav_step` makes of one frame's input.
fn step_for(events: Vec<Event>, modifiers: Modifiers, keys: bool) -> Option<i32> {
    let ctx = egui::Context::default();
    let mut step = None;
    let mut all = vec![Event::ModifiersChanged(modifiers)];
    all.extend(events);
    let input = RawInput {
        events: all,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        step = keyboard_step(ui.ctx(), keys).or_else(|| ui.ctx().input(nav_step));
    });
    output.textures_delta.clear();
    step
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn mouse(button: PointerButton) -> Vec<Event> {
    let pos = egui::pos2(10.0, 10.0);
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    ]
}

#[test]
fn alt_arrows_and_mouse_buttons_step() {
    let alt = Modifiers::ALT;
    assert_eq!(
        step_for(vec![key(Key::ArrowLeft, alt)], alt, true),
        Some(-1)
    );
    assert_eq!(
        step_for(vec![key(Key::ArrowRight, alt)], alt, true),
        Some(1)
    );
    assert_eq!(
        step_for(mouse(PointerButton::Extra1), Modifiers::NONE, true),
        Some(-1)
    );
    assert_eq!(
        step_for(mouse(PointerButton::Extra2), Modifiers::NONE, true),
        Some(1)
    );
}

#[test]
fn other_keys_and_text_fields_do_not_step() {
    let alt = Modifiers::ALT;
    // Alt+↑ / Alt+↓ go from change to change in a diff.
    assert_eq!(step_for(vec![key(Key::ArrowUp, alt)], alt, true), None);
    assert_eq!(step_for(vec![key(Key::ArrowDown, alt)], alt, true), None);
    // Plain arrows move in the graph.
    let none = Modifiers::NONE;
    assert_eq!(step_for(vec![key(Key::ArrowLeft, none)], none, true), None);
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    assert_eq!(
        step_for(vec![key(Key::ArrowLeft, alt_shift)], alt_shift, true),
        None
    );
    let alt_cmd = Modifiers::ALT | Modifiers::COMMAND;
    assert_eq!(
        step_for(vec![key(Key::ArrowLeft, alt_cmd)], alt_cmd, true),
        None
    );
    // In a text field Alt+← jumps a word; the mouse's buttons still go back.
    assert_eq!(step_for(vec![key(Key::ArrowLeft, alt)], alt, false), None);
    assert_eq!(
        step_for(mouse(PointerButton::Extra1), none, false),
        Some(-1)
    );
    assert_eq!(step_for(mouse(PointerButton::Primary), none, true), None);
}

// A repository with two commits; the second adds a file in a folder whose names have spaces
// and accents. Gives the folder and the file's path.
include!("tests_repository.rs");
include!("tests_keyboard_ui.rs");

#[path = "manual_history.rs"]
mod manual_history;
