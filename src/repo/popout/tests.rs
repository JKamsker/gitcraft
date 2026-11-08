//! Tests of popping views out: what is popped as views open and close, the windows' ids and
//! titles, and the windows themselves, natively (through a stand-in for eframe's renderer)
//! and inside the main window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use egui_kittest::kittest::Queryable;

use super::{Panel, Popouts, Request, closes, shorten, title, viewport_id};
use crate::git;
use crate::repo::{Action, RepoTab};

#[test]
fn popping_out_and_docking() {
    let mut p = Popouts::default();
    assert!(!p.any());
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
fn several_views_pop_out_in_a_steady_order_and_come_back_together() {
    let mut p = Popouts::default();
    for panel in [Panel::Ci, Panel::Terminal, Panel::Diff, Panel::Pr] {
        p.pop_out(panel);
    }
    assert_eq!(p.popped().collect::<Vec<_>>(), Panel::ALL);
    // A worktree parked, or Bring back in the palette.
    assert!(p.dock_all());
    assert!(!p.any());
    assert!(!p.dock_all());
}

#[test]
fn a_window_opens_the_frame_after_its_view_popped_out() {
    let mut p = Popouts::default();
    p.pop_out(Panel::Diff);
    // The main window drew the diff this frame: its window waits for the next.
    assert!(p.take_drawn().is_empty());
    assert_eq!(p.take_drawn(), [Panel::Diff]);
    p.pop_out(Panel::Ci);
    assert_eq!(p.take_drawn(), [Panel::Diff]);
    assert_eq!(p.take_drawn(), [Panel::Diff, Panel::Ci]);
    // Popped out and back within a frame: nothing to draw, nothing waiting.
    p.pop_out(Panel::Pr);
    p.dock(Panel::Pr);
    assert_eq!(p.take_drawn(), [Panel::Diff, Panel::Ci]);
}

#[test]
fn only_closing_a_view_on_purpose_takes_its_window() {
    assert_eq!(closes(&Action::CloseDiff, 0), Some(Panel::Diff));
    assert_eq!(closes(&Action::ClosePr, 0), Some(Panel::Pr));
    assert_eq!(closes(&Action::CloseCi, 0), Some(Panel::Ci));
    assert_eq!(
        closes(&Action::Popout(Request::Close(Panel::Terminal)), 2),
        Some(Panel::Terminal)
    );
    // The last shell closed takes the terminal; one of several doesn't.
    assert_eq!(closes(&Action::CloseTerminal(0), 1), Some(Panel::Terminal));
    assert_eq!(closes(&Action::CloseTerminal(0), 2), None);
    // Selecting a commit closes the diff as a side effect: the window stays for the next.
    assert_eq!(closes(&Action::Select(3), 0), None);
    assert_eq!(closes(&Action::Popout(Request::Dock(Panel::Diff)), 0), None);
    let mut p = Popouts::default();
    p.pop_out(Panel::Diff);
    p.pop_out(Panel::Terminal);
    p.note(&Action::Select(0), 1);
    assert!(p.is_popped(Panel::Diff));
    p.note(&Action::CloseDiff, 1);
    assert!(!p.is_popped(Panel::Diff));
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
fn titles_name_the_view_the_repository_and_the_app() {
    assert_eq!(
        title("app", "Diff of src/main.rs"),
        "Diff of src/main.rs — app — GitCraft"
    );
    assert_eq!(
        title("  ünï cødé ", "Workflow runs of 1234abcd"),
        "Workflow runs of 1234abcd — ünï cødé — GitCraft"
    );
    // A long path keeps its start and its file name.
    let deep = format!("Diff of {}/main.rs", "dir/".repeat(60));
    let t = title("app", &deep);
    assert!(t.contains("/main.rs — app"), "{t}");
    assert!(t.starts_with("Diff of dir/"), "{t}");
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

/// One frame of the app's: the tab's pop-outs, as the tab shown.
fn frame(ctx: &egui::Context, tab: &mut RepoTab) {
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        tab.popouts_ui(ui.ctx(), true);
    });
    output.textures_delta.clear();
}

/// A tab on a fresh repository, with the builds of a commit open (no forge: nothing is asked).
fn tab_with_ci(name: &str, ctx: &egui::Context) -> (PathBuf, RepoTab) {
    let dir = repo(name);
    let mut tab = RepoTab::new_deferred(dir.clone(), ctx);
    tab.apply(
        Action::OpenCi {
            sha: "a".repeat(40),
            run: None,
        },
        ctx,
    );
    (dir, tab)
}

/// A context drawing native windows through [`native_windows`].
fn native(close: &Rc<RefCell<bool>>) -> (egui::Context, Seen) {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    ctx.set_embed_viewports(false);
    (ctx, native_windows(close.clone()))
}

#[test]
fn a_popped_out_view_gets_a_native_window_until_it_is_closed() {
    let close = Rc::new(RefCell::new(false));
    let (ctx, seen) = native(&close);
    let (dir, mut tab) = tab_with_ci("native", &ctx);
    tab.apply(Action::Popout(Request::PopOut(Panel::Ci)), &ctx);
    assert!(!tab.docked(Panel::Ci));
    // The main window drew it this frame; its window opens on the next.
    frame(&ctx, &mut tab);
    assert!(seen.borrow().is_empty());
    frame(&ctx, &mut tab);
    let windows = seen.borrow().clone();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].0, viewport_id(&dir, Panel::Ci));
    assert!(
        windows[0].1.starts_with("Workflow runs of aaaaaaaa — "),
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
fn selecting_a_commit_leaves_popped_out_views_where_they_are() {
    let close = Rc::new(RefCell::new(false));
    let (ctx, seen) = native(&close);
    let (dir, mut tab) = tab_with_ci("select", &ctx);
    tab.apply(Action::Popout(Request::PopOut(Panel::Ci)), &ctx);
    tab.apply(
        Action::OpenDiff(crate::repo::DiffKey::Commit {
            hash: "b".repeat(40),
            path: "a b/ünï.rs".to_owned(),
        }),
        &ctx,
    );
    tab.apply(Action::Popout(Request::PopOut(Panel::Diff)), &ctx);
    frame(&ctx, &mut tab);
    // A commit clicked in the main window's graph: in the main window, the diff closes and
    // so would the builds; in windows of their own, the builds stay and the diff's window
    // waits for the next diff.
    tab.apply(Action::SelectHash("c".repeat(40)), &ctx);
    assert!(tab.ci_view.is_some());
    // What selecting a loaded commit does to the diff (this tab has read no history).
    tab.diff = None;
    assert!(!tab.docked(Panel::Diff));
    seen.borrow_mut().clear();
    frame(&ctx, &mut tab);
    let titles: Vec<String> = seen.borrow().iter().map(|(_, t)| t.clone()).collect();
    assert_eq!(titles.len(), 2, "{titles:?}");
    assert!(titles[0].starts_with("Diff — "), "{titles:?}");
    // The next diff opens there.
    tab.apply(
        Action::OpenDiff(crate::repo::DiffKey::Commit {
            hash: "d".repeat(40),
            path: "next.rs".to_owned(),
        }),
        &ctx,
    );
    seen.borrow_mut().clear();
    frame(&ctx, &mut tab);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(seen.borrow()[0].1.starts_with("Diff of next.rs — "));
}

#[test]
fn the_main_windows_keys_leave_a_popped_out_view_alone() {
    let close = Rc::new(RefCell::new(false));
    let (ctx, _seen) = native(&close);
    let (dir, mut tab) = tab_with_ci("keys", &ctx);
    assert!(tab.main_keys(&ctx, Panel::Ci));
    tab.apply(Action::Popout(Request::PopOut(Panel::Ci)), &ctx);
    assert!(!tab.main_keys(&ctx, Panel::Ci));
    // Without native windows, its window is inside the main one, whose keys are the only ones.
    ctx.set_embed_viewports(true);
    assert!(tab.main_keys(&ctx, Panel::Ci));
    // The toolbar's Terminal raises a popped-out terminal rather than hiding it.
    tab.terminal_open = true;
    tab.popouts.pop_out(Panel::Terminal);
    tab.apply(Action::ToggleTerminal, &ctx);
    let still_open = tab.terminal_open;
    // Its window's close button hides it, and takes the pop-out.
    tab.apply(Action::Popout(Request::Close(Panel::Terminal)), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(still_open);
    assert!(!tab.terminal_open);
    assert!(tab.docked(Panel::Terminal));
}

#[test]
fn without_native_windows_a_pop_out_is_a_window_inside_and_comes_back() {
    let ctx = egui::Context::default();
    // egui's default: no native windows, as on platforms with only one.
    assert!(ctx.embed_viewports());
    let (dir, mut tab) = tab_with_ci("embedded", &ctx);
    tab.apply(Action::Popout(Request::PopShown), &ctx);
    assert!(!tab.docked(Panel::Ci));
    let window_title = title(&tab.name, "Workflow runs of aaaaaaaa");
    // The first frame only puts in the fonts, which take from the next one on.
    let mut harness = egui_kittest::Harness::new_ui_state(
        |ui, (fonts, tab): &mut (bool, Box<RepoTab>)| {
            if !std::mem::replace(fonts, true) {
                crate::theme::install_fonts(ui.ctx());
            } else {
                tab.popouts_ui(ui.ctx(), true);
            }
        },
        (false, Box::new(tab)),
    );
    harness.run_steps(3);
    // The window inside the main one, with its title, offers Bring back; so does the main
    // window's reminder of it.
    let has_title = harness.query_by_label(&window_title).is_some();
    let hint = harness
        .query_by_role_and_label(egui::accesskit::Role::Button, "Show the CI view window")
        .is_some();
    let backs = harness
        .query_all_by_label("Bring the CI view back into the main window")
        .count();
    if let Some(button) = harness
        .get_all_by_label("Bring the CI view back into the main window")
        .next()
    {
        button.click();
    }
    harness.run_steps(2);
    let docked = harness.state().1.docked(Panel::Ci);
    let still_open = harness.state().1.ci_view.is_some();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(has_title, "no window titled {window_title}");
    assert!(hint);
    assert_eq!(backs, 2);
    assert!(docked);
    assert!(still_open);
}

#[test]
fn popping_out_what_isnt_open_says_so() {
    let dir = repo("nothing");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(Action::Popout(Request::PopShown), &ctx);
    let shown = tab.status.as_deref().cloned();
    tab.apply(Action::Popout(Request::PopOut(Panel::Diff)), &ctx);
    let diff = tab.status.as_deref().cloned();
    tab.apply(Action::Popout(Request::DockAll), &ctx);
    let dock = tab.status.as_deref().cloned();
    // The interactive rebase takes the big area: what is under it doesn't pop out.
    tab.apply(
        Action::OpenCi {
            sha: "a".repeat(40),
            run: None,
        },
        &ctx,
    );
    tab.rebase = Some(crate::repo::rebase::RebaseEditor::new(
        "x".into(),
        "x".into(),
        Vec::new(),
    ));
    tab.apply(Action::Popout(Request::PopShown), &ctx);
    let rebase = tab.status.as_deref().cloned();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(shown, Some((m, true)) if m.starts_with("Nothing to pop out")));
    assert!(matches!(diff, Some((m, true)) if m == "There's no diff open"));
    assert!(matches!(dock, Some((m, true)) if m.contains("No view")));
    assert!(matches!(rebase, Some((m, true)) if m.contains("interactive rebase")));
    assert!(!tab.popouts.any());
}
