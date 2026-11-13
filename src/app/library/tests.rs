//! The library in the app, on real folders and a headless window: configured roots, the
//! siblings of an opened repository, Recent left as it is, opening, hiding, stopping and
//! retrying, profile switches and restarts.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::super::Tab;
use super::model::{Outcome, Source};
use super::roots_dialog::RootsClick;
use super::*;

/// A folder of its own for a test, under the temporary folder (whose own children are never
/// a nearby folder: see `identity::nearby_parent`).
fn tree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-library-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    super::super::strip_verbatim(std::fs::canonicalize(&dir).unwrap_or(dir))
}

fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    assert!(crate::git::run(Some(dir), &["init", "-q"]).is_ok());
}

/// The start page and the roots dialog of an app made as the window makes it.
fn harness() -> Harness<'static, Option<App>> {
    Harness::builder()
        .with_size([900.0, 1600.0])
        .build_ui_state(
            |ui, state: &mut Option<App>| {
                if state.is_none() {
                    let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                    let mut app = App::launched_using(&cc, None, || {
                        Ok("git version 2.43.0 (recorded)".into())
                    });
                    app.settings.check_updates = false;
                    app.settings.notifications = false;
                    app.settings.set(ui.ctx());
                    *state = Some(app);
                    // Its fonts are in from the next frame.
                    return;
                }
                if let Some(app) = state {
                    let ctx = ui.ctx().clone();
                    app.poll_library(&ctx);
                    app.library_dialog(&ctx);
                    app.home(ui);
                }
            },
            None,
        )
}

fn app<'h>(h: &'h mut Harness<'static, Option<App>>) -> &'h mut App {
    h.step();
    let app = h.state_mut().as_mut();
    assert!(app.is_some());
    app.unwrap_or_else(|| unreachable!())
}

/// Steps until the library's scans are done (or ten seconds pass).
fn settle(h: &mut Harness<'static, Option<App>>) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        h.step();
        let busy = h
            .state()
            .as_ref()
            .is_some_and(|a| a.library.scheduler.busy());
        if !busy || Instant::now() > end {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    h.step();
}

fn listed(app: &App) -> Vec<String> {
    let lib = app.library.get(app.profile());
    let mut names: Vec<String> = lib
        .map(|l| {
            l.entries
                .iter()
                .filter_map(|e| e.path.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Depth 1 finds the root's two child repositories; depth 2 the nested one too. A row opens
/// its repository through the usual path (and into Recent); a hidden one stays hidden when
/// its root is removed and added again; removing a root leaves Recent and tabs alone.
#[test]
fn configured_roots_list_open_and_hide() {
    let base = tree("roots");
    let code = base.join("code");
    for repo in ["alpha", "beta", "group/gamma"] {
        init(&code.join(repo));
    }
    let mut h = harness();
    let a = app(&mut h);
    let profile = a.profile();
    a.library.add_root(profile, code.clone(), 1);
    settle(&mut h);
    let a = app(&mut h);
    assert_eq!(listed(a), ["alpha", "beta"]);
    let id = a.library.get(profile).map_or(0, |l| l.roots[0].id);
    a.roots_click(RootsClick::Depth(id, 2));
    settle(&mut h);
    assert_eq!(listed(app(&mut h)), ["alpha", "beta", "gamma"]);

    let alpha = code.join("alpha");
    h.get_by_label(&crate::remote::display(&alpha)).click();
    h.step();
    let a = app(&mut h);
    assert_eq!(a.recent, std::slice::from_ref(&alpha));
    assert!(matches!(&a.tabs[a.active], Tab::Repo(r) if r.path == alpha));
    assert_eq!(a.tabs.len(), 1, "one tab, for the one opened");

    let beta = code.join("beta");
    let ctx = h.ctx.clone();
    app(&mut h).library_click(LibraryClick::Hide(beta.clone()), &ctx);
    settle(&mut h);
    let a = app(&mut h);
    a.roots_click(RootsClick::Remove(id));
    // Opening alpha listed its folder's repositories as Nearby: those stay, gamma goes.
    assert_eq!(listed(a), ["alpha", "beta"]);
    let lib = a.library.get(profile).cloned().unwrap_or_default();
    assert!(
        lib.entries
            .iter()
            .all(|e| !e.sources.contains(&Source::Root(id)))
    );
    assert_eq!(
        a.recent,
        std::slice::from_ref(&alpha),
        "Recent keeps what was opened"
    );
    a.library.add_root(profile, code.clone(), 1);
    settle(&mut h);
    let a = app(&mut h);
    let lib = a.library.get(profile).cloned().unwrap_or_default();
    let rows = rows::all(&lib, &a.recent);
    assert!(
        rows.iter().all(|r| r.path != beta),
        "hidden survives root changes"
    );
    assert!(rows.iter().any(|r| r.path == alpha));
    let _ = std::fs::remove_dir_all(&base);
}

/// Opening `…/work/projects/app` lists `…/work/projects/tool` as Nearby without any root;
/// `…/work` (a level up) isn't looked in. Turned off, the nearby rows go; Recent stays.
#[test]
fn an_opened_repository_lists_its_siblings() {
    let base = tree("nearby");
    let projects = base.join("work/projects");
    init(&projects.join("app"));
    init(&projects.join("tool"));
    init(&base.join("work/far"));
    let _ = std::fs::create_dir_all(projects.join("notes"));
    let mut h = harness();
    let ctx = h.ctx.clone();
    app(&mut h).open_repo(&projects.join("app"), &ctx);
    settle(&mut h);
    let a = app(&mut h);
    assert_eq!(listed(a), ["app", "tool"]);
    let lib = a.library.get(a.profile()).cloned().unwrap_or_default();
    assert_eq!(lib.nearby.len(), 1);
    assert_eq!(lib.nearby[0].parent, projects);
    assert_eq!(
        lib.nearby[0].last.as_ref().map(|l| l.outcome.clone()),
        Some(Outcome::Complete)
    );
    // Opened again soon: not scanned again.
    a.open_repo(&projects.join("app"), &ctx);
    assert!(!a.library.scheduler.busy());
    let profile = a.profile();
    a.roots_click(RootsClick::Nearby(false));
    assert!(listed(a).is_empty());
    assert_eq!(a.recent, [projects.join("app")]);
    // Off: opening a repository looks nowhere.
    a.open_repo(&projects.join("tool"), &ctx);
    assert!(!a.library.scheduler.busy());
    assert!(a.library.get(profile).is_some_and(|l| l.nearby.is_empty()));
    let _ = std::fs::remove_dir_all(&base);
}

/// Thirty repositories are all listed and searchable; Recent, its order and the tabs stay
/// as they were; only the rows in view are laid out.
#[test]
fn thirty_repositories_leave_recent_alone() {
    let base = tree("thirty");
    for i in 0..32 {
        init(&base.join(format!("repo{i:02}")));
    }
    let mut h = harness();
    let a = app(&mut h);
    let recent = vec![
        PathBuf::from("/elsewhere/one"),
        PathBuf::from("/elsewhere/two"),
    ];
    a.recent = recent.clone();
    let profile = a.profile();
    a.library.add_root(profile, base.clone(), 1);
    settle(&mut h);
    let a = app(&mut h);
    assert_eq!(listed(a).len(), 32);
    assert_eq!(a.recent, recent);
    assert_eq!(a.tabs.len(), 1);
    assert!(matches!(a.tabs[0], Tab::Home));
    h.run_steps(2);
    let laid_out = h
        .query_all_by(|n| {
            n.label()
                .is_some_and(|l| l.contains("repo") && l.contains("gitcraft-library-thirty"))
        })
        .count();
    assert!((1..=12).contains(&laid_out), "{laid_out} rows laid out");
    // The library's search and keys: typed, then Enter opens the match.
    h.ctx.memory_mut(|m| m.request_focus(search_id()));
    h.step();
    h.input_mut()
        .events
        .push(egui::Event::Text("repo31".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    let a = app(&mut h);
    assert_eq!(a.recent[0], base.join("repo31"));
    assert_eq!(a.recent[1..], recent[..]);
    let _ = std::fs::remove_dir_all(&base);
}

/// What a scan of one profile sends after a switch to another is never applied to either;
/// a stopped scan keeps what was found, and Rescan finishes the job.
#[test]
fn profile_switches_stops_and_retries() {
    let base = tree("profiles");
    init(&base.join("a"));
    init(&base.join("b"));
    let mut h = harness();
    let ctx = h.ctx.clone();
    let a = app(&mut h);
    let first = a.profile();
    a.library.add_root(first, base.clone(), 1);
    let other = a.settings.add_profile("Other", false);
    a.settings.set(&ctx);
    assert!(a.switch_profile(other, &ctx));
    settle(&mut h);
    let a = app(&mut h);
    assert!(listed(a).is_empty());
    let root = a.library.get(first).map(|l| l.roots[0].clone());
    assert!(
        root.as_ref().is_some_and(|r| r.last.is_none()),
        "nothing applied to the first"
    );
    assert!(a.switch_profile(first, &ctx));
    let id = root.map_or(0, |r| r.id);
    // Stopped while waiting: nothing changes; Rescan does it.
    a.library.rescan(first, &Source::Root(id));
    a.roots_click(RootsClick::Stop(Source::Root(id)));
    settle(&mut h);
    assert!(listed(app(&mut h)).is_empty());
    app(&mut h).roots_click(RootsClick::Rescan(Source::Root(id)));
    settle(&mut h);
    let a = app(&mut h);
    assert_eq!(listed(a), ["a", "b"]);
    // The dialog shows the root with its state.
    a.library_open = true;
    h.run_steps(2);
    h.get_by_label_contains("2 repositories");
    let _ = std::fs::remove_dir_all(&base);
}

/// The library is saved with the window's state and read back on start, shown at once from
/// the cache; state saved before the library existed reads with an empty one.
#[test]
fn the_library_survives_a_restart() {
    let old: super::super::Saved = ron::from_str("(recent: [\"/r/one\"])").unwrap_or_default();
    assert!(old.library.is_empty());
    assert_eq!(old.recent, [PathBuf::from("/r/one")]);
    let mut lib = Library::default();
    let id = lib.add_root(PathBuf::from("/code"), 1);
    lib.finish(
        &Source::Root(id),
        model::ScanResult {
            repos: vec![PathBuf::from("/code/a")],
            outcome: Outcome::Complete,
            at: 5,
        },
    );
    let saved = super::super::Saved {
        library: Libraries::from([(7, lib.clone())]),
        ..Default::default()
    };
    let text = ron::to_string(&saved).unwrap_or_default();
    let back: super::super::Saved = ron::from_str(&text).unwrap_or_default();
    let state = LibraryState::new(back.library);
    let cached = state.get(7).cloned().unwrap_or_default();
    assert_eq!(cached, lib);
    assert_eq!(rows::all(&cached, &[]).len(), 1);
}
