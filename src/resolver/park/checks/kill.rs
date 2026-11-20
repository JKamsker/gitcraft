//! Real process termination and restart (#205, slice 3). The test binary re-executes itself as
//! an owned child that runs production park, bring back or drop and calls
//! `std::process::abort` at one effect boundary: no unwinding, no destructor, no error
//! handler, no lock release by the program. The parent waits for that one child (no signal,
//! no PID search) and then restarts through production list, bring back, drop and forget.
//! Git stays the recording fake; its refs live in the fixture's `.git`, so they outlive the
//! child like Git's. This proves process-kill behaviour only, never power loss.
use super::recording_checks::{Fixture, labels, trace};
use super::recording_kill_child::{MARKER, kill};
use super::recording_regressions::refs_clean;
use super::*;

pub(super) const UNTRACKED: &[u8] = b"\xffuntracked user file\0\r\n";

/// Where a kill stops: the first or last boundary starting with `prefix`, only among those
/// before the first `before` boundary when that is given.
pub(super) struct At {
    pub action: &'static str,
    pub prefix: &'static str,
    pub last: bool,
    pub before: Option<&'static str>,
}

pub(super) const fn first(action: &'static str, prefix: &'static str) -> At {
    At {
        action,
        prefix,
        last: false,
        before: None,
    }
}

const fn last(action: &'static str, prefix: &'static str) -> At {
    At {
        last: true,
        ..first(action, prefix)
    }
}

/// What restart finds, per the durability audit's kill table.
const UNLISTED: &str = "not listed, live worktree untouched, forgotten";
const BUSY: &str = "listed, bring back refused (progress)";
const STOPPED: &str = "listed, bring back refused (Interrupted recovery)";
const BACK: &str = "listed, brought back byte-exact";

/// One kill per transition the durability audit lists (P2-P12, R1-R8, C1-C5, D1-D2).
const KEY: &[(At, &str)] = &[
    (first("park", "archive-reserved"), UNLISTED),
    (first("park", "phase:Archiving"), UNLISTED),
    (first("park", "archive-index-copied"), UNLISTED),
    (
        first("park", "git:update-ref refs/gitcraft/parked/"),
        UNLISTED,
    ),
    (first("park", "archive-raw-copied"), UNLISTED),
    (last("park", "archive-metadata-copied"), UNLISTED),
    // The manifest's temp file is flushed but not yet renamed into place (gap G2).
    (
        At {
            before: Some("archive-manifest-written"),
            ..last("park", "atomic-temp-synced")
        },
        UNLISTED,
    ),
    (first("park", "archive-manifest-written"), BUSY),
    (first("park", "archive-reachable"), BUSY),
    (first("park", "phase:Archived"), BUSY),
    (first("park", "phase:Clearing"), BUSY),
    (first("park", "git:reset --hard -q HEAD"), BACK),
    (first("park", "phase:Parked"), BACK),
    (last("restore", "untouched-evidence-durable"), BACK),
    (first("restore", "phase:Restoring"), BACK),
    (first("restore", "git:read-tree"), STOPPED),
    (first("restore", "restore-plan-durable"), BACK),
    (first("restore", "index-lock-synced"), STOPPED),
    (first("restore", "destination-temp-durable"), BACK),
    (first("restore", "destination-published"), BACK),
    (last("restore", "restore-copy-applied"), BACK),
    (first("restore", "live-restore-synced"), BACK),
    (first("restore", "phase:Restored"), BACK),
    (first("restore", "phase:Cleanup"), BACK),
    (
        first("restore", "git:update-ref refs/gitcraft/parked-recovery/"),
        BACK,
    ),
    (first("restore", "archive-moved"), BACK),
    (
        first("restore", "git:update-ref -d refs/gitcraft/parked/"),
        BACK,
    ),
    (first("drop", "phase:Dropping"), BACK),
    (first("drop", "git:update-ref refs/heads/"), BACK),
    (first("drop", "archive-moved"), BACK),
    (
        first("drop", "git:update-ref -d refs/gitcraft/parked/"),
        BACK,
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
    let (f, parked) = fixture(at.action);
    act(at.action, &f.0, parked.as_ref()).unwrap();
    let all = labels();
    let end = at
        .before
        .map_or(all.len(), |a| all.iter().position(|l| l == a).unwrap());
    let mut hits = all[..end]
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with(at.prefix));
    let found = if at.last {
        hits.next_back()
    } else {
        hits.next()
    };
    let (n, label) = found.unwrap_or_else(|| panic!("{} has no boundary {}", at.action, at.prefix));
    (n + 1, label.clone())
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
/// Returns what it found, one of the outcomes of [`KEY`].
fn restart(f: &Fixture, action: &str, parked: Option<&Parked>, at: &str) -> &'static str {
    untouched(f);
    let lock = std::fs::read(f.0.join(".git/index.lock")).ok();
    let listed = list(&f.0).unwrap();
    assert_eq!(list(&f.0).unwrap(), listed, "{at}");
    let Some(p) = parked.or(listed.first()).cloned() else {
        // Killed before the manifest was published: the worktree was never touched.
        assert_eq!(action, "park", "{at}");
        live_original(f);
        // Reported as incomplete (the user is told where), then forgotten on request.
        let found = list_all(&f.0).unwrap();
        let reservation = folder(&f.0, "test-id").unwrap();
        assert_eq!(
            found.incomplete,
            vec![("test-id".into(), reservation)],
            "{at}"
        );
        forget(&f.0, "test-id");
        assert_eq!(list_all(&f.0).unwrap(), Listing::default(), "{at}");
        let pinned = run(
            &f.0,
            &[
                "rev-parse",
                "-q",
                "--verify",
                "refs/gitcraft/parked/test-id",
            ],
        );
        assert!(pinned.is_err(), "{at}: forget left the ref");
        let settled = tree(&f.0);
        forget(&f.0, "test-id");
        assert!(list(&f.0).unwrap().is_empty(), "{at}");
        assert_eq!(tree(&f.0), settled, "{at}: forget replay changed files");
        untouched(f);
        return UNLISTED;
    };
    assert_eq!(listed, vec![p.clone()], "{at}");
    let archive = discovery::locate(&f.0, &p.id)
        .unwrap()
        .display()
        .to_string();
    let outcome = match bring_back(&f.0, &p) {
        Ok(()) => {
            live_original(f);
            BACK
        }
        Err(e) if e.contains("in progress") => BUSY,
        Err(e) if e.contains(&format!("Interrupted recovery at {archive}")) => STOPPED,
        Err(e) => panic!("{at}: {e}"),
    };
    // An index lock the killed child left is never adopted or removed.
    let now = std::fs::read(f.0.join(".git/index.lock")).ok();
    assert_eq!(now, lock, "{at}");
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
    outcome
}

/// Recording only, no child: the child counts boundaries from the start of its action, so
/// every key boundary must exist and sit at the same number on every run.
#[test]
fn key_boundaries_exist_and_are_numbered_deterministically() {
    for (at, _) in KEY {
        assert_eq!(boundary(at), boundary(at), "{} {}", at.action, at.prefix);
    }
}

#[test]
fn a_real_kill_at_each_key_boundary_restarts_byte_exact_and_idempotently() {
    for (at, expected) in KEY {
        let (stop, label) = boundary(at);
        let (f, parked) = fixture(at.action);
        let log = kill(at.action, stop, &f.0);
        assert!(log.contains(&format!("{MARKER}{stop}: {label}")), "{log}");
        let name = format!("{} {stop} ({label})", at.action);
        let outcome = restart(&f, at.action, parked.as_ref(), &name);
        assert_eq!(outcome, *expected, "{name}");
        eprintln!("killed {name}: {outcome}; dropped to a branch, replay unchanged");
    }
    eprintln!("real kills: {} boundaries restarted", KEY.len());
}
