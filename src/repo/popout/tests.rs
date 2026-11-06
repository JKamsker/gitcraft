//! Tests of popping views out: what is popped as views open and close, the windows' ids and
//! titles, and the windows themselves, natively (through a stand-in for eframe's renderer)
//! and inside the main window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use egui_kittest::kittest::Queryable;

use super::{Panel, Popouts, Request, shorten, title, viewport_id};
use crate::git;
use crate::repo::{Action, RepoTab};

#[test]
fn popping_out_and_docking() {
    let mut p = Popouts::default();
    assert!(p.popped().next().is_none());
    assert!(p.pop_out(Panel::Diff));
    assert!(p.is_popped(Panel::Diff));
    assert!(!p.is_popped(Panel::Terminal));
    // Its window closed: back in the main window.
    assert!(p.dock(Panel::Diff));
    assert!(!p.is_popped(Panel::Diff));
    assert!(!p.dock(Panel::Diff));
    // And it pops out again.
    assert!(p.pop_out(Panel::Diff));
    assert!(p.is_popped(Panel::Diff));
}

#[test]
fn popping_out_twice_keeps_one_window() {
    let mut p = Popouts::default();
    assert!(p.pop_out(Panel::Pr));
    assert!(!p.pop_out(Panel::Pr));
    assert_eq!(p.popped().collect::<Vec<_>>(), [Panel::Pr]);
    assert!(p.dock(Panel::Pr));
    assert!(p.popped().next().is_none());
}

#[test]
fn several_views_pop_out_in_a_steady_order() {
    let mut p = Popouts::default();
    for panel in [Panel::Ci, Panel::Terminal, Panel::Diff, Panel::Pr] {
        p.pop_out(panel);
    }
    assert_eq!(p.popped().collect::<Vec<_>>(), Panel::ALL);
}

#[test]
fn a_view_that_closes_takes_its_window_with_it() {
    let mut p = Popouts::default();
    p.pop_out(Panel::Diff);
    p.pop_out(Panel::Terminal);
    // The diff was closed (Esc in the main window): its pop-out is forgotten.
    let gone = p.forget_closed(|panel| panel == Panel::Terminal);
    assert_eq!(gone, [Panel::Diff]);
    assert!(!p.is_popped(Panel::Diff));
    assert!(p.is_popped(Panel::Terminal));
    // Nothing more to forget while the terminal is open.
    assert!(p.forget_closed(|_| true).is_empty());
    assert!(p.is_popped(Panel::Terminal));
}

#[test]
fn every_tab_and_view_has_a_window_of_its_own() {
    let a = Path::new("/repos/app");
    let b = Path::new("/repos/lib");
    assert_eq!(viewport_id(a, Panel::Diff), viewport_id(a, Panel::Diff));
    assert_ne!(viewport_id(a, Panel::Diff), viewport_id(b, Panel::Diff));
    assert_ne!(viewport_id(a, Panel::Diff), viewport_id(a, Panel::Terminal));
    assert_ne!(viewport_id(a, Panel::Diff), egui::ViewportId::ROOT);
    let ids: std::collections::HashSet<_> = Panel::ALL
        .into_iter()
        .flat_map(|p| [viewport_id(a, p), viewport_id(b, p)])
        .collect();
    assert_eq!(ids.len(), 8);
}

#[test]
fn titles_name_the_repository_and_the_view() {
    assert_eq!(
        title("app", "diff of src/main.rs"),
        "gitcraft — app: diff of src/main.rs"
    );
    assert_eq!(
        title("  ünï cødé ", "CI of 1234abcd"),
        "gitcraft — ünï cødé: CI of 1234abcd"
    );
    // A long path keeps its start and its file name.
    let deep = format!("diff of {}/main.rs", "dir/".repeat(60));
    let t = title("app", &deep);
    assert!(t.ends_with("/main.rs"), "{t}");
    assert!(t.contains('…'));
    assert!(t.chars().count() < 150, "{t}");
    // Cut on characters, not bytes.
    let wide = "日本語のパス/".repeat(30);
    let t = title(&"レポ".repeat(40), &wide);
    assert!(t.contains('…'));
    assert!(t.chars().count() < 150);
}

#[test]
fn shortening_cuts_in_the_middle() {
    assert_eq!(shorten("short", 10), "short");
    assert_eq!(shorten("", 10), "");
    assert_eq!(shorten("abcdefghij", 10), "abcdefghij");
    assert_eq!(shorten("abcdefghijk", 10), "abcd…ghijk");
    assert_eq!(shorten("ééééééééééé", 5).chars().count(), 5);
    assert_eq!(shorten("abc", 0), "…");
}

/// A repository with one commit, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft popout {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

/// What the stand-in renderer saw of one native window: its id and title.
type Seen = Rc<RefCell<Vec<(egui::ViewportId, String)>>>;

/// Stands in for eframe's renderer of native windows on this thread: draws each window
/// asked for in a pass of its own, as eframe does, with a close request when `close` says.
fn native_windows(close: Rc<RefCell<bool>>) -> Seen {
    let seen: Seen = Rc::default();
    let log = seen.clone();
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let id = viewport.ids.this;
        log.borrow_mut()
            .push((id, viewport.builder.title.clone().unwrap_or_default()));
        let mut input = egui::RawInput {
            viewport_id: id,
            ..Default::default()
        };
        let mut info = egui::ViewportInfo::default();
        if *close.borrow() {
            info.events.push(egui::ViewportEvent::Close);
        }
        input.viewports.insert(id, info);
        let mut output = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
        output.textures_delta.clear();
    });
    seen
}

/// One frame of the app's: every tab's pop-outs.
fn frame(ctx: &egui::Context, tab: &mut RepoTab) {
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        tab.popouts_ui(ui.ctx());
    });
    output.textures_delta.clear();
}

#[test]
fn a_popped_out_view_gets_a_native_window_until_it_is_closed() {
    let dir = repo("native");
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    ctx.set_embed_viewports(false);
    let close = Rc::new(RefCell::new(false));
    let seen = native_windows(close.clone());
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(
        Action::OpenCi {
            sha: "a".repeat(40),
            run: None,
        },
        &ctx,
    );
    tab.apply(Action::Popout(Request::PopOut(Panel::Ci)), &ctx);
    assert!(!tab.docked(Panel::Ci));
    frame(&ctx, &mut tab);
    let windows = seen.borrow().clone();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].0, viewport_id(&dir, Panel::Ci));
    assert!(
        windows[0].1.contains("CI of aaaaaaaa"),
        "{:?}",
        windows[0].1
    );
    assert!(windows[0].1.contains(&tab.name), "{:?}", windows[0].1);

    // Its close button: back in the main window, the view still open.
    *close.borrow_mut() = true;
    frame(&ctx, &mut tab);
    assert!(tab.docked(Panel::Ci));
    assert!(tab.ci_view.is_some());
    seen.borrow_mut().clear();
    *close.borrow_mut() = false;
    frame(&ctx, &mut tab);
    assert!(seen.borrow().is_empty());

    // Popped out again, then the view closed: the window goes with it.
    tab.apply(Action::Popout(Request::PopShown), &ctx);
    frame(&ctx, &mut tab);
    assert_eq!(seen.borrow().len(), 1);
    tab.apply(Action::CloseCi, &ctx);
    seen.borrow_mut().clear();
    frame(&ctx, &mut tab);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(seen.borrow().is_empty());
    assert!(tab.docked(Panel::Ci));
}

#[test]
fn without_native_windows_a_pop_out_is_a_window_inside_and_comes_back() {
    let dir = repo("embedded");
    let ctx = egui::Context::default();
    // egui's default: no native windows, as on platforms with only one.
    assert!(ctx.embed_viewports());
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(
        Action::OpenCi {
            sha: "b".repeat(40),
            run: None,
        },
        &ctx,
    );
    tab.apply(Action::Popout(Request::PopShown), &ctx);
    assert!(!tab.docked(Panel::Ci));
    let window_title = title(&tab.name, "CI of bbbbbbbb");
    // The first frame only puts in the fonts, which take from the next one on.
    let mut harness = egui_kittest::Harness::new_ui_state(
        |ui, (fonts, tab): &mut (bool, Box<RepoTab>)| {
            if !std::mem::replace(fonts, true) {
                crate::theme::install_fonts(ui.ctx());
            } else {
                tab.popouts_ui(ui.ctx());
            }
        },
        (false, Box::new(tab)),
    );
    harness.run_steps(2);
    // The window inside the main one, with its title, offers Bring back.
    let has_title = harness.query_by_label(&window_title).is_some();
    harness
        .get_by_label("Bring the CI view back into the main window")
        .click();
    harness.run_steps(2);
    let docked = harness.state().1.docked(Panel::Ci);
    let still_open = harness.state().1.ci_view.is_some();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(has_title, "no window titled {window_title}");
    assert!(docked);
    assert!(still_open);
}

#[test]
fn popping_out_what_isnt_open_says_so() {
    let dir = repo("nothing");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(Action::Popout(Request::PopShown), &ctx);
    let shown = tab.status.clone();
    tab.apply(Action::Popout(Request::PopOut(Panel::Diff)), &ctx);
    let diff = tab.status.clone();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(shown, Some((m, true)) if m.contains("Nothing to pop out")));
    assert!(matches!(diff, Some((m, true)) if m.contains("no diff")));
    assert!(tab.popouts.popped().next().is_none());
}
