//! Repository health: how big it is (objects, packs, commits), how long reading it takes, and
//! what would make it faster (the commit-graph, scheduled maintenance, a file system monitor,
//! the untracked cache, packing loose objects), each with the commands that turn it on.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::git::{run, run_ok_codes};

/// What `git count-objects -v` says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Objects {
    /// Loose objects, and their size in KiB.
    pub loose: u64,
    pub loose_kib: u64,
    /// Objects in packs, the packs, and their size in KiB.
    pub packed: u64,
    pub packs: u64,
    pub pack_kib: u64,
    /// Loose objects that are also in packs (a `git prune-packed` removes them).
    pub prune_packable: u64,
    /// Files in the objects folder that aren't objects, and their size in KiB.
    pub garbage: u64,
    pub garbage_kib: u64,
}

/// Reads `git count-objects -v`: `key: value` lines; unknown keys are skipped.
pub fn parse_count_objects(out: &str) -> Objects {
    let mut o = Objects::default();
    for line in out.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let Ok(n) = value.trim().parse::<u64>() else {
            continue;
        };
        match key.trim() {
            "count" => o.loose = n,
            "size" => o.loose_kib = n,
            "in-pack" => o.packed = n,
            "packs" => o.packs = n,
            "size-pack" => o.pack_kib = n,
            "prune-packable" => o.prune_packable = n,
            "garbage" => o.garbage = n,
            "size-garbage" => o.garbage_kib = n,
            _ => {}
        }
    }
    o
}

/// Whether a speed-up is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    On,
    Off,
    /// Not available where git runs (the built-in file system monitor on Linux).
    Unsupported,
}

/// Everything the health section shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub objects: Objects,
    /// Commits reachable from any ref.
    pub commits: Option<u64>,
    /// How long `git status` took, as gitcraft runs it on every change.
    pub status_took: Option<Duration>,
    pub commit_graph: bool,
    /// The repository is registered for `git maintenance` (`maintenance.repo`).
    pub maintenance: bool,
    pub fsmonitor: Option<State>,
    pub untracked_cache: bool,
}

/// Reads the repository's health. Several reads, all in the background.
pub fn health(repo: &Path, unix_there: bool) -> Result<Health, String> {
    let objects = run(Some(repo), &["count-objects", "-v"])
        .map(|out| parse_count_objects(&out))
        .map_err(|e| format!("Could not count the objects: {e}"))?;
    let commits = run(Some(repo), &["rev-list", "--count", "--all"])
        .ok()
        .and_then(|n| n.trim().parse().ok());
    let started = Instant::now();
    let status_took = crate::git::status(repo).ok().map(|_| started.elapsed());
    Ok(Health {
        objects,
        commits,
        status_took,
        commit_graph: commit_graph(repo),
        maintenance: maintenance_registered(repo),
        fsmonitor: Some(fsmonitor(repo, unix_there)),
        untracked_cache: config_bool(repo, "core.untrackedCache"),
    })
}

fn config_bool(repo: &Path, key: &str) -> bool {
    run(Some(repo), &["config", "--bool", "--get", key]).is_ok_and(|v| v.trim() == "true")
}

/// Whether a commit-graph file (or chain) is there.
fn commit_graph(repo: &Path) -> bool {
    let Ok(out) = run(
        Some(repo),
        &[
            "rev-parse",
            "--git-path",
            "objects/info/commit-graph",
            "--git-path",
            "objects/info/commit-graphs",
        ],
    ) else {
        return false;
    };
    out.lines().any(|printed| {
        let path = crate::remote::resolve(repo, printed);
        crate::remote::try_is_file(&path).unwrap_or(false)
            || crate::remote::try_is_dir(&path).unwrap_or(false)
    })
}

/// Whether `git maintenance start` registered the repository: its top folder is one of the
/// global `maintenance.repo` values.
fn maintenance_registered(repo: &Path) -> bool {
    let top = crate::remote::git_path(repo);
    run_ok_codes(
        Some(repo),
        &["config", "--global", "--get-all", "maintenance.repo"],
        &[0, 1],
    )
    .is_ok_and(|out| registered(&out, &top))
}

/// Whether `top` is among the `maintenance.repo` values `out` lists.
pub fn registered(out: &str, top: &str) -> bool {
    let norm = |s: &str| s.trim().trim_end_matches(['/', '\\']).replace('\\', "/");
    out.lines().any(|line| {
        let (a, b) = (norm(line), norm(top));
        if cfg!(windows) {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        }
    })
}

/// The file system monitor: git's own (Windows and macOS only), or a hook (`core.fsmonitor`
/// naming a program), or off. `unix_there`: git runs on Linux (WSL, SSH, here), where the
/// built-in one doesn't exist.
fn fsmonitor(repo: &Path, unix_there: bool) -> State {
    let value = run(Some(repo), &["config", "--get", "core.fsmonitor"])
        .map(|v| v.trim().to_owned())
        .unwrap_or_default();
    match value.as_str() {
        "" | "false" | "no" | "off" | "0" if unix_there => State::Unsupported,
        "" | "false" | "no" | "off" | "0" => State::Off,
        _ => State::On,
    }
}

/// A speed-up the doctor suggests: why, whether it is on, and the commands that turn it on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub title: &'static str,
    pub why: String,
    pub state: State,
    /// What turns it on, as git commands.
    pub ops: Vec<Vec<String>>,
}

fn ops(list: &[&[&str]]) -> Vec<Vec<String>> {
    list.iter()
        .map(|args| args.iter().map(|a| (*a).to_owned()).collect())
        .collect()
}

fn on(yes: bool) -> State {
    if yes { State::On } else { State::Off }
}

/// Status slower than this is worth speeding up.
const SLOW_STATUS: Duration = Duration::from_millis(300);
/// More loose objects than this are worth packing.
const MANY_LOOSE: u64 = 2000;

/// What would make the repository faster, from its health.
pub fn suggestions(h: &Health) -> Vec<Suggestion> {
    let slow = h.status_took.is_some_and(|t| t >= SLOW_STATUS);
    let status_note = match h.status_took {
        Some(t) => format!("git status took {} ms here", t.as_millis()),
        None => "git status couldn't be timed".to_owned(),
    };
    let mut out = vec![
        Suggestion {
            title: "Commit-graph",
            why: "A file of every commit's parents and dates that makes walking history (the \
                  graph, ahead / behind counts, merges) much faster. Fetches keep it up to date \
                  once fetch.writeCommitGraph is on."
                .to_owned(),
            state: on(h.commit_graph),
            ops: ops(&[
                &["commit-graph", "write", "--reachable", "--changed-paths"],
                &["config", "--local", "fetch.writeCommitGraph", "true"],
            ]),
        },
        Suggestion {
            title: "Scheduled maintenance",
            why: "git maintenance start registers the repository and has the system run git \
                  every hour (and day, and week) to prefetch, repack and keep the commit-graph \
                  up to date in the background (cron, systemd, launchd or the Windows Task \
                  Scheduler)."
                .to_owned(),
            state: on(h.maintenance),
            ops: ops(&[&["maintenance", "start"]]),
        },
        Suggestion {
            title: "File system monitor",
            why: format!(
                "git's own monitor watches the working tree, so status needn't look at every \
                 file; {status_note}{}.",
                if slow {
                    ", which it would speed up"
                } else {
                    ""
                }
            ),
            state: h.fsmonitor.unwrap_or(State::Off),
            ops: ops(&[&["config", "--local", "core.fsmonitor", "true"]]),
        },
        Suggestion {
            title: "Untracked cache",
            why: format!(
                "git remembers which folders have untracked files and only looks again in \
                 those that changed; {status_note}."
            ),
            state: on(h.untracked_cache),
            ops: ops(&[
                &["config", "--local", "core.untrackedCache", "true"],
                &["update-index", "--untracked-cache"],
            ]),
        },
    ];
    if h.objects.loose > MANY_LOOSE || h.objects.packs > 50 {
        out.push(Suggestion {
            title: "Pack loose objects",
            why: format!(
                "{} loose objects in {} KiB and {} packs: packing them (git gc) saves room and \
                 time on every read. Unreachable objects younger than two weeks are kept.",
                h.objects.loose, h.objects.loose_kib, h.objects.packs
            ),
            state: State::Off,
            ops: ops(&[&["gc", "--quiet"]]),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_objects_is_read() {
        let o = parse_count_objects(
            "count: 5\nsize: 20\nin-pack: 1200\npacks: 2\nsize-pack: 830\nprune-packable: 1\n\
             garbage: 0\nsize-garbage: 0\nfuture-key: 9\nbroken line\n",
        );
        assert_eq!(
            (o.loose, o.loose_kib, o.packed, o.packs, o.pack_kib),
            (5, 20, 1200, 2, 830)
        );
        assert_eq!(o.prune_packable, 1);
        assert_eq!(parse_count_objects(""), Objects::default());
    }

    #[test]
    fn maintenance_registration_is_matched() {
        assert!(registered("/home/me/a\n/home/me/repo\n", "/home/me/repo/"));
        assert!(!registered("/home/me/repo2\n", "/home/me/repo"));
        assert!(!registered("", "/x"));
        if cfg!(windows) {
            assert!(registered("C:/Code/Repo\n", "c:\\code\\repo"));
        }
    }

    #[test]
    fn suggestions_follow_the_health() {
        let h = Health {
            objects: Objects {
                loose: 5000,
                ..Default::default()
            },
            status_took: Some(Duration::from_millis(900)),
            fsmonitor: Some(State::Unsupported),
            ..Default::default()
        };
        let s = suggestions(&h);
        assert_eq!(s.len(), 5);
        assert!(s.iter().any(|x| x.title == "Pack loose objects"));
        let fs = s.iter().find(|x| x.title == "File system monitor");
        assert!(fs.is_some_and(|x| x.state == State::Unsupported && x.why.contains("900 ms")));
        let quiet = suggestions(&Health {
            commit_graph: true,
            ..Default::default()
        });
        assert_eq!(quiet.len(), 4);
        assert_eq!(quiet[0].state, State::On);
    }
}
