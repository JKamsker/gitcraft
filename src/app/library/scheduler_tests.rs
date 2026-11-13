use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use super::*;
use crate::app::repo_scan::Stopped;

fn job(source: Source, path: &str) -> Job {
    Job {
        source,
        path: PathBuf::from(path),
        limits: repo_scan::NEARBY_LIMITS,
    }
}

/// Polls until `done` holds (or ten seconds pass).
fn until(s: &mut Scheduler, profile: u64, lib: &mut Library, done: impl Fn(&Scheduler) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done(s) && Instant::now() < end {
        s.poll(profile, lib, 1);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(done(s), "timed out");
}

/// A runner that reports `<path>/repo` found, at once.
fn finds_one() -> Runner {
    Arc::new(|job, _, on| {
        let repo = job.path.join("repo");
        on(ScanEvent::Repo(&repo));
        Ok(Found {
            repos: vec![repo],
            stopped: None,
            ..Default::default()
        })
    })
}

/// Scans whose threads block (a hung mount, ignoring cancellation) keep their slots: no more
/// than [`MAX_WORKERS`] threads ever run, however many are asked for and stopped.
#[test]
fn blocked_scans_never_start_more_threads() {
    let gate = Arc::new(AtomicBool::new(false));
    let most = Arc::new(AtomicUsize::new(0));
    let live = Arc::new(AtomicUsize::new(0));
    let (g, m, l) = (gate.clone(), most.clone(), live.clone());
    let mut s = Scheduler::with_runner(Arc::new(move |_, _, _| {
        let now = l.fetch_add(1, Ordering::SeqCst) + 1;
        m.fetch_max(now, Ordering::SeqCst);
        while !g.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        l.fetch_sub(1, Ordering::SeqCst);
        Ok(Found::default())
    }));
    let mut lib = Library::default();
    for i in 0..6 {
        let id = lib.add_root(PathBuf::from(format!("/r{i}")), 1);
        assert!(s.queue(1, job(Source::Root(id), &format!("/r{i}"))));
    }
    assert!(
        !s.queue(1, job(Source::Root(1), "/r0")),
        "asked twice, queued once"
    );
    s.poll(1, &mut lib, 1);
    assert_eq!(s.threads(), MAX_WORKERS);
    s.retire_all();
    for i in 1..=6 {
        s.queue(1, job(Source::Root(i), "/again"));
    }
    for _ in 0..20 {
        s.poll(1, &mut lib, 1);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(s.threads(), MAX_WORKERS);
    gate.store(true, Ordering::SeqCst);
    until(&mut s, 1, &mut lib, |s| !s.busy() && s.threads() == 0);
    assert!(most.load(Ordering::SeqCst) <= MAX_WORKERS);
}

/// A result comes back too late to apply: its root was removed, or the profile changed.
#[test]
fn late_results_are_never_applied() {
    let mut lib = Library::default();
    let gone = lib.add_root(PathBuf::from("/gone"), 1);
    let kept = lib.add_root(PathBuf::from("/kept"), 1);
    let mut s = Scheduler::with_runner(finds_one());
    s.queue(1, job(Source::Root(gone), "/gone"));
    s.queue(1, job(Source::Root(kept), "/kept"));
    s.poll(1, &mut lib, 1);
    lib.remove_root(gone);
    until(&mut s, 1, &mut lib, |s| s.threads() == 0);
    let found: Vec<_> = lib.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(found, [repo_scan::top_folder(PathBuf::from("/kept/repo"))]);
    assert!(lib.root(kept).and_then(|r| r.last.clone()).is_some());

    // Started in profile 1, polled after switching to profile 2.
    let mut other = Library::default();
    let id = other.add_root(PathBuf::from("/kept"), 1);
    s.queue(1, job(Source::Root(id), "/kept"));
    until(&mut s, 2, &mut other, |s| s.threads() == 0);
    assert!(other.entries.is_empty());
    assert!(other.root(id).and_then(|r| r.last.clone()).is_none());

    // Retired while running (the root was edited): dropped too.
    s.queue(1, job(Source::Root(id), "/kept"));
    s.poll(1, &mut other, 1);
    s.retire(&Source::Root(id));
    until(&mut s, 1, &mut other, |s| s.threads() == 0);
    assert!(other.entries.is_empty());
}

/// The real engine on a real tree: found as it goes, then settled; a stop keeps what it had.
#[test]
fn a_real_scan_applies_incrementally() {
    let root = std::env::temp_dir().join(format!("gitcraft-lib-sched-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for name in ["a", "b"] {
        let _ = std::fs::create_dir_all(root.join(name));
        let _ = crate::git::run(Some(&root.join(name)), &["init", "-q"]);
    }
    let mut lib = Library::default();
    let id = lib.add_root(root.clone(), 1);
    let mut s = Scheduler::default();
    let source = Source::Root(id);
    s.queue(
        7,
        Job {
            source: source.clone(),
            path: root.clone(),
            limits: repo_scan::ROOT_LIMITS,
        },
    );
    assert_eq!(s.activity(7, &source), Some(Activity::Queued));
    until(&mut s, 7, &mut lib, |s| s.threads() == 0 && !s.busy());
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(lib.count(&source), 2);
    let names: Vec<_> = lib
        .entries
        .iter()
        .filter_map(|e| e.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert_eq!(names, ["a", "b"]);
    let record = lib.root(id).and_then(|r| r.last.clone());
    assert_eq!(record.map(|r| r.outcome), Some(Outcome::Complete));
    assert!(lib.entries.iter().all(|e| Path::new(&e.path).is_absolute()));

    // Stopped before it starts walking: a partial result, nothing taken away.
    let mut s = Scheduler::with_runner(Arc::new(|_, cancel, _| {
        while !cancel.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(Found {
            repos: Vec::new(),
            stopped: Some(Stopped::Asked),
            ..Default::default()
        })
    }));
    s.queue(7, job(source.clone(), "/x"));
    s.poll(7, &mut lib, 1);
    s.stop(&source);
    until(&mut s, 7, &mut lib, |s| s.threads() == 0);
    assert_eq!(lib.count(&source), 2);
    assert!(lib.entries.iter().all(|e| e.stale()));
}
