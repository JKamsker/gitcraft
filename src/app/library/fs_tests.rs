//! The library against real file systems: one row per checkout however it is reached
//! (links, case, Unicode), large trees stopped within bounds, and the window's cost of a
//! full library.

use std::time::{Duration, Instant};

use super::model::{MAX_INDEX, Outcome, ScanResult, Source};
use super::scheduler::{Job, MAX_WORKERS, Scheduler};
use super::*;
use crate::app::App;
use crate::app::repo_scan::Stopped;

fn tree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-libfs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    assert!(crate::git::run(Some(dir), &["init", "-q"]).is_ok());
}

fn root_job(lib: &mut Library, path: &Path, depth: u8) -> Job {
    let id = lib.add_root(path.to_owned(), depth);
    Job {
        source: Source::Root(id),
        path: path.to_owned(),
        limits: ScanLimits {
            depth: usize::from(depth),
            ..ROOT_LIMITS
        },
    }
}

fn run_all(s: &mut Scheduler, lib: &mut Library) {
    let end = Instant::now() + Duration::from_secs(20);
    while (s.busy() || s.threads() > 0) && Instant::now() < end {
        s.poll(1, lib, 1);
        assert!(s.threads() <= MAX_WORKERS);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!s.busy(), "timed out");
}

/// A root reached through a link and the same folder named directly, and a folder with
/// spaces and Unicode in its name: one row per checkout, with both roots as its sources.
#[test]
fn one_row_per_checkout_however_it_is_reached() {
    let base = tree("identity");
    let real = base.join("my code/Größe 日本");
    init(&real.join("app"));
    init(&real.join("tool"));
    let mut lib = Library::default();
    let mut s = Scheduler::default();
    let job = root_job(&mut lib, &real, 1);
    s.queue(1, job);
    #[cfg(unix)]
    {
        let link = base.join("link");
        let _ = std::os::unix::fs::symlink(&real, &link);
        let job = root_job(&mut lib, &link, 1);
        s.queue(1, job);
    }
    // Named again with other case where the file system ignores it: the same root.
    if cfg!(any(windows, target_os = "macos")) {
        let upper = PathBuf::from(real.to_string_lossy().to_ascii_uppercase());
        let before = lib.roots.len();
        lib.add_root(upper, 1);
        assert_eq!(lib.roots.len(), before);
    }
    run_all(&mut s, &mut lib);
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(lib.entries.len(), 2, "{:?}", lib.entries);
    let sources = if cfg!(unix) { 2 } else { 1 };
    assert!(lib.entries.iter().all(|e| e.sources.len() == sources));
    assert!(lib.entries.iter().all(|e| !e.stale()));
    let names: Vec<_> = lib
        .entries
        .iter()
        .filter_map(|e| e.path.parent().and_then(|p| p.file_name()))
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| n == "Größe 日本"), "{names:?}");
}

/// A Windows junction inside a root is never followed (a link the same as any other).
#[cfg(windows)]
#[test]
fn junctions_are_not_followed() {
    let base = tree("junction");
    init(&base.join("real/app"));
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(base.join("jump"))
        .arg(base.join("real"))
        .output()
        .is_ok_and(|o| o.status.success());
    let stop = std::sync::atomic::AtomicBool::new(false);
    let limits = ScanLimits {
        depth: 2,
        ..ROOT_LIMITS
    };
    let found = crate::app::repo_scan::walk(&base, limits, &stop, &mut |_| {});
    let _ = std::fs::remove_dir_all(&base);
    if made {
        let found = found.unwrap_or_default();
        assert_eq!(found.repos.len(), 1, "{:?}", found.repos);
    }
}

/// A wide and deep tree stops at the folder limit, quickly, with an honest partial result;
/// what it found before stays when a later scan is cut short sooner.
#[test]
fn a_large_tree_stops_within_its_bounds() {
    let base = tree("large");
    for a in 0..40 {
        for b in 0..40 {
            let _ = std::fs::create_dir_all(base.join(format!("a{a:02}/b{b:02}")));
        }
        init(&base.join(format!("a{a:02}/repo")));
    }
    let mut lib = Library::default();
    let mut s = Scheduler::default();
    let mut job = root_job(&mut lib, &base, 2);
    job.limits.max_dirs = 600;
    let source = job.source.clone();
    let started = Instant::now();
    s.queue(1, job.clone());
    run_all(&mut s, &mut lib);
    let took = started.elapsed();
    let record = lib.record_mut(&source).and_then(|r| r.clone());
    assert_eq!(
        record.map(|r| r.outcome),
        Some(Outcome::Partial(Stopped::Folders))
    );
    assert!(took < Duration::from_secs(10), "{took:?}");
    let found = lib.count(&source);
    assert!(found > 0 && found < 40, "{found}");
    // Cut short sooner: none taken away, all kept as stale.
    job.limits.max_dirs = 2;
    s.queue(1, job);
    run_all(&mut s, &mut lib);
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(lib.count(&source), found);
    assert!(lib.entries.iter().all(|e| e.stale()));
}

/// The start page with a full library: rows are made once, not every frame, and only those
/// in view are laid out; how long a frame takes is measured and reported.
#[test]
fn a_full_library_costs_the_window_little() {
    let mut lib = Library::default();
    let id = lib.add_root(PathBuf::from("/huge"), 1);
    let repos: Vec<PathBuf> = (0..MAX_INDEX)
        .map(|i| PathBuf::from(format!("/huge/repo{i:05}")))
        .collect();
    lib.finish(
        &Source::Root(id),
        ScanResult {
            repos,
            outcome: Outcome::Complete,
            at: now_secs(),
        },
    );
    let mut h = egui_kittest::Harness::builder()
        .with_size([900.0, 1200.0])
        .build_ui_state(
            |ui, state: &mut Option<App>| {
                if state.is_none() {
                    let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                    *state = Some(App::launched_using(&cc, None, || {
                        Ok("git version 2.43.0 (recorded)".into())
                    }));
                    return;
                }
                if let Some(app) = state {
                    let t = crate::theme::Tokens::get(ui.ctx());
                    let _ = app.library_section(ui, &t);
                }
            },
            None,
        );
    h.step();
    if let Some(app) = h.state_mut().as_mut() {
        let profile = app.profile();
        app.library.saved.insert(profile, lib);
        app.library.rev += 1;
    }
    h.step();
    let made = h.state().as_ref().map(|a| a.library.rows.rows.as_ptr());
    let started = Instant::now();
    for _ in 0..10 {
        h.step();
    }
    let per_frame = started.elapsed() / 10;
    eprintln!("library section, {MAX_INDEX} repositories: {per_frame:?} a frame");
    let after = h.state().as_ref().map(|a| a.library.rows.rows.as_ptr());
    assert_eq!(made, after, "rows made again without a change");
    assert_eq!(
        h.state().as_ref().map_or(0, |a| a.library.rows.rows.len()),
        MAX_INDEX
    );
    use egui_kittest::kittest::Queryable;
    let laid_out = h
        .query_all_by(|n| n.label().is_some_and(|l| l.starts_with("/huge/repo")))
        .count();
    assert!((1..=12).contains(&laid_out), "{laid_out}");
    // Generous: a debug build on a busy CI machine; a regression to per-frame rebuilding of
    // every row costs far more.
    assert!(per_frame < Duration::from_millis(500), "{per_frame:?}");
}
