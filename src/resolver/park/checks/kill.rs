//! Real process termination and restart (#205, slice 3). The test binary re-executes itself as
//! an owned child that runs production park, bring back or drop and calls
//! `std::process::abort` at one effect boundary: no unwinding, no destructor, no error
//! handler, no lock release by the program. The parent waits for that one child (no signal,
//! no PID search) and then restarts through production list, bring back, drop and forget.
//! Git stays the recording fake; its refs live in the fixture's `.git`, so they outlive the
//! child like Git's. This proves process-kill behaviour only, never power loss.
use super::recording_checks::{Fixture, labels, trace};
use super::recording_regressions::refs_clean;
use super::*;
use std::io::Write as _;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// `<action>\n<boundary number>\n<fixture>`: set only by [`kill`] for its own child.
const CHILD_ENV: &str = "GITCRAFT_RECOVERY_KILL";
const MARKER: &str = "recovery-kill: aborting at boundary ";
const CHILD_DEADLINE: Duration = Duration::from_secs(120);
pub(super) const UNTRACKED: &[u8] = b"\xffuntracked user file\0\r\n";

/// Where each kill stops: the first or last boundary starting with `prefix`, optionally only
/// among those before the first `anchor`.
pub(super) struct At(
    pub &'static str,
    pub &'static str,
    pub bool,
    pub Option<&'static str>,
);

/// One kill per transition the durability audit lists (P2-P12, R2-R8, C1-C5, D1-D2).
const KEY: &[At] = &[
    At("park", "archive-reserved", false, None),
    At("park", "phase:Archiving", false, None),
    At("park", "archive-index-copied", false, None),
    At("park", "git:update-ref refs/gitcraft/parked/", false, None),
    At("park", "archive-raw-copied", false, None),
    At("park", "archive-metadata-copied", true, None),
    At(
        "park",
        "atomic-temp-synced",
        true,
        Some("archive-manifest-written"),
    ),
    At("park", "archive-manifest-written", false, None),
    At("park", "archive-reachable", false, None),
    At("park", "phase:Archived", false, None),
    At("park", "phase:Clearing", false, None),
    At("park", "git:reset --hard -q HEAD", false, None),
    At("park", "phase:Parked", false, None),
    At("restore", "untouched-evidence-durable", true, None),
    At("restore", "phase:Restoring", false, None),
    At("restore", "git:read-tree", false, None),
    At("restore", "restore-plan-durable", false, None),
    At("restore", "index-lock-synced", false, None),
    At("restore", "destination-temp-durable", false, None),
    At("restore", "destination-published", false, None),
    At("restore", "restore-copy-applied", true, None),
    At("restore", "live-restore-synced", false, None),
    At("restore", "phase:Restored", false, None),
    At("restore", "phase:Cleanup", false, None),
    At(
        "restore",
        "git:update-ref refs/gitcraft/parked-recovery/",
        false,
        None,
    ),
    At("restore", "archive-moved", false, None),
    At(
        "restore",
        "git:update-ref -d refs/gitcraft/parked/",
        false,
        None,
    ),
    At("drop", "phase:Dropping", false, None),
    At("drop", "git:update-ref refs/heads/", false, None),
    At("drop", "archive-moved", false, None),
    At(
        "drop",
        "git:update-ref -d refs/gitcraft/parked/",
        false,
        None,
    ),
];

pub(super) fn act(action: &str, repo: &Path, parked: Option<&Parked>) -> Result<(), String> {
    match (action, parked) {
        ("park", _) => park(repo, "test-id", &["file".into()]).map(drop),
        ("restore", Some(p)) => bring_back(repo, p),
        ("drop", Some(p)) => drop_to_branch(repo, p).map(drop),
        _ => Err(format!("unknown kill action {action}")),
    }
}

/// A fixture with an untracked file too; for bring back and drop, already parked.
pub(super) fn fixture(action: &str) -> (Fixture, Option<Parked>) {
    let f = Fixture::new();
    std::fs::write(f.0.join("untracked"), UNTRACKED).unwrap();
    let parked = (action != "park").then(|| f.park());
    trace(None);
    (f, parked)
}

/// The boundary number (1-based, counted from the action's start) of `at`, and its label.
pub(super) fn boundary(at: &At) -> (usize, String) {
    let (f, parked) = fixture(at.0);
    act(at.0, &f.0, parked.as_ref()).unwrap();
    let all = labels();
    let end =
        at.3.map_or(all.len(), |a| all.iter().position(|l| l == a).unwrap());
    let mut hits = all[..end]
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with(at.1));
    let found = if at.2 { hits.last() } else { hits.next() };
    let (n, label) = found.unwrap_or_else(|| panic!("{} has no boundary {}", at.0, at.1));
    (n + 1, label.clone())
}

/// The child: runs `action` on the parent's fixture and aborts at the boundary it was given.
#[test]
#[ignore = "child of the kill/restart tests: only their owned parent process runs it"]
fn child() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let [action, stop, repo]: [&str; 3] = spec.splitn(3, '\n').collect::<Vec<_>>()[..]
        .try_into()
        .unwrap();
    let stop: usize = stop.parse().unwrap();
    let repo = PathBuf::from(repo);
    let parked = (action != "park").then(|| list(&repo).unwrap().remove(0));
    let mut seen = 0;
    effects::HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move |label| {
            seen += 1;
            if seen == stop {
                // Not libtest's captured print: straight to the parent's log file.
                let _ = writeln!(std::io::stderr(), "{MARKER}{stop}: {label}");
                std::process::abort();
            }
        }));
    });
    let result = act(action, &repo, parked.as_ref());
    let _ = writeln!(
        std::io::stderr(),
        "recovery-kill: {stop} not reached: {result:?}"
    );
}

#[cfg(unix)]
fn aborted(status: ExitStatus) -> bool {
    std::os::unix::process::ExitStatusExt::signal(&status) == Some(6)
}

/// std's abort is `__fastfail`: STATUS_STACK_BUFFER_OVERRUN, no handler or unwinding.
#[cfg(windows)]
fn aborted(status: ExitStatus) -> bool {
    status.code() == Some(0xC000_0409_u32 as i32)
}

/// Runs the child for `action` on `repo`, killed at boundary `stop`; returns its log.
pub(super) fn kill(action: &str, stop: usize, repo: &Path) -> String {
    let name = module_path!().split_once("::").unwrap().1.to_owned() + "::child";
    let log = repo.with_extension("kill.log");
    let out = std::fs::File::create(&log).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            &name,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, format!("{action}\n{stop}\n{}", repo.display()))
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + CHILD_DEADLINE;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            // Only this owned handle is stopped; nothing else is looked up or signalled.
            let _ = child.kill();
            let _ = child.wait();
            panic!("kill child for {action}:{stop} exceeded {CHILD_DEADLINE:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let text = String::from_utf8_lossy(&std::fs::read(&log).unwrap()).into_owned();
    std::fs::remove_file(&log).unwrap();
    assert!(aborted(status), "{action}:{stop} ended {status:?}\n{text}");
    text
}

/// Every path and its bytes under the fixture: an idempotent replay leaves it identical.
pub(super) fn tree(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut all = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                all.push((relative, None));
                pending.push(path);
            } else {
                all.push((relative, Some(std::fs::read(&path).unwrap())));
            }
        }
    }
    all.sort();
    all
}

pub(super) fn live_original(f: &Fixture) {
    assert_eq!(std::fs::read(f.0.join("file")).unwrap(), b"\xffraw\0\r\n");
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).unwrap(),
        b"DIRC-original"
    );
    assert_eq!(
        std::fs::read(f.0.join(".git/MERGE_HEAD")).unwrap(),
        b"theirs\r\n"
    );
}

pub(super) fn untouched(f: &Fixture) {
    f.proof();
    assert_eq!(std::fs::read(f.0.join("untracked")).unwrap(), UNTRACKED);
}

/// A restart after the kill: production entry points only, byte-exact, then replayed.
fn restart(f: &Fixture, action: &str, parked: Option<&Parked>, at: &str) {
    untouched(f);
    let lock = std::fs::read(f.0.join(".git/index.lock")).ok();
    let first = list(&f.0).unwrap();
    assert_eq!(list(&f.0).unwrap(), first, "{at}");
    let Some(p) = parked.or(first.first()).cloned() else {
        // Killed before the manifest was published: the worktree was never touched.
        assert_eq!(action, "park", "{at}");
        live_original(f);
        forget(&f.0, "test-id");
        assert!(list(&f.0).unwrap().is_empty(), "{at}");
        let settled = tree(&f.0);
        forget(&f.0, "test-id");
        assert!(list(&f.0).unwrap().is_empty(), "{at}");
        assert_eq!(tree(&f.0), settled, "{at}: forget replay changed files");
        return untouched(f);
    };
    match bring_back(&f.0, &p) {
        Ok(()) => live_original(f),
        Err(e) => assert!(
            [
                "Interrupted recovery",
                "progress",
                "locked",
                "Commit or stash"
            ]
            .iter()
            .any(|s| e.contains(s)),
            "{at}: {e}"
        ),
    }
    // An index lock the killed child left is never adopted or removed.
    assert_eq!(
        std::fs::read(f.0.join(".git/index.lock")).ok(),
        lock,
        "{at}"
    );
    untouched(f);
    let branch = drop_to_branch(&f.0, &p).unwrap();
    refs_clean(f, &p);
    let settled = tree(&f.0);
    // Idempotent replay: recovery once more is a no-op, byte for byte.
    assert_eq!(list(&f.0).unwrap(), vec![p.clone()], "{at}");
    assert_eq!(drop_to_branch(&f.0, &p).unwrap(), branch, "{at}");
    assert_eq!(list(&f.0).unwrap(), vec![p.clone()], "{at}");
    assert_eq!(tree(&f.0), settled, "{at}: replay changed files");
    untouched(f);
}

/// Recording only, no child: the child counts boundaries from the start of its action, so
/// every key boundary must exist and sit at the same number on every run.
#[test]
fn key_boundaries_exist_and_are_numbered_deterministically() {
    for at in KEY {
        assert_eq!(boundary(at), boundary(at), "{} {}", at.0, at.1);
    }
}

#[test]
fn a_real_kill_at_each_key_boundary_restarts_byte_exact_and_idempotently() {
    for at in KEY {
        let (stop, label) = boundary(at);
        let (f, parked) = fixture(at.0);
        let log = kill(at.0, stop, &f.0);
        assert!(log.contains(&format!("{MARKER}{stop}: {label}")), "{log}");
        restart(
            &f,
            at.0,
            parked.as_ref(),
            &format!("{}:{stop} {label}", at.0),
        );
        eprintln!("killed {} at boundary {stop} ({label}): restart ok", at.0);
    }
    eprintln!("real kills: {} boundaries restarted", KEY.len());
}
