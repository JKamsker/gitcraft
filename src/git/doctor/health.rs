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
    /// A shallow clone: git doesn't use a commit-graph in one.
    pub shallow: bool,
    /// A partial clone: objects it lacks are fetched as they are needed.
    pub partial: bool,
    /// It runs on another machine (WSL, SSH): its timings include the connection.
    pub remote: bool,
}

/// Reads the repository's health. Several reads, side by side, all in the background.
pub fn health(repo: &Path, unix_there: bool) -> Result<Health, String> {
    std::thread::scope(|scope| {
        // Counting every commit is the slow one on a big history: beside the rest.
        let commits = scope.spawn(crate::connections::inherit(|| {
            run(
                Some(repo),
                &[
                    "rev-list",
                    "--count",
                    // As the graph counts them: not stashes, notes or prefetched refs.
                    "--exclude=refs/stash",
                    "--exclude=refs/notes/*",
                    "--exclude=refs/prefetch/*",
                    "--all",
                ],
            )
            .ok()
            .and_then(|n| n.trim().parse().ok())
        }));
        let objects = run(Some(repo), &["count-objects", "-v"])
            .map(|out| parse_count_objects(&out))
            .map_err(|e| {
                crate::i18n::message(
                    "backend-could-not-count-the-objects-e-bb5e79",
                    &[("detail", (e).to_string().into())],
                )
            })?;
        // The best of two: the first may be reading a cold disk.
        let status_took = (0..2)
            .filter_map(|_| {
                let started = Instant::now();
                crate::git::status(repo).ok().map(|_| started.elapsed())
            })
            .min();
        let health = Health {
            objects,
            commits: None,
            status_took,
            commit_graph: commit_graph(repo),
            maintenance: maintenance_registered(repo),
            fsmonitor: Some(fsmonitor(repo, unix_there)),
            untracked_cache: config_bool(repo, "core.untrackedCache"),
            shallow: run(Some(repo), &["rev-parse", "--is-shallow-repository"])
                .is_ok_and(|v| v.trim() == "true"),
            partial: partial_clone(repo),
            remote: crate::remote::is_remote(repo),
        };
        Ok(Health {
            commits: commits.join().ok().flatten(),
            ..health
        })
    })
}

/// Whether objects are fetched as needed: a promisor remote (`--filter=…` clones).
fn partial_clone(repo: &Path) -> bool {
    run_ok_codes(
        Some(repo),
        &[
            "config",
            "--get-regexp",
            r"^(remote\..*\.promisor|extensions\.partialclone)$",
        ],
        &[0, 1],
    )
    .is_ok_and(|out| {
        out.lines().any(|l| {
            l.split_once(' ')
                .is_some_and(|(_, v)| !matches!(v.trim(), "" | "false"))
        })
    })
}

fn config_bool(repo: &Path, key: &str) -> bool {
    run(Some(repo), &["config", "--bool", "--get", key]).is_ok_and(|v| v.trim() == "true")
}

/// Whether git uses a commit-graph here: a file, or the list of a split chain, and not turned
/// off with `core.commitGraph=false`.
fn commit_graph(repo: &Path) -> bool {
    if run(
        Some(repo),
        &["config", "--bool", "--get", "core.commitGraph"],
    )
    .is_ok_and(|v| v.trim() == "false")
    {
        return false;
    }
    let Ok(out) = run(
        Some(repo),
        &[
            "rev-parse",
            "--git-path",
            "objects/info/commit-graph",
            "--git-path",
            // Not the folder: it stays, empty, once a chain was written into one file again.
            "objects/info/commit-graphs/commit-graph-chain",
        ],
    ) else {
        return false;
    };
    out.lines().any(|printed| {
        let path = crate::remote::resolve(repo, printed);
        crate::remote::try_is_file(&path).unwrap_or(false)
    })
}

/// Whether `git maintenance start` registered the repository: its top folder, as git on its
/// machine resolves it (`~/x` over SSH is written absolute), is one of the global
/// `maintenance.repo` values.
fn maintenance_registered(repo: &Path) -> bool {
    let Ok(top) = run(Some(repo), &["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    let top = top.trim_end_matches(['\n', '\r']).to_owned();
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
    let value = run(
        Some(repo),
        &["config", "--type=bool-or-str", "--get", "core.fsmonitor"],
    )
    .map(|v| v.trim().to_owned())
    .unwrap_or_default();
    fsmonitor_state(&value, unix_there)
}

/// [`fsmonitor`] from the value git normalised: `true` / `false`, or a hook's path.
pub fn fsmonitor_state(value: &str, unix_there: bool) -> State {
    match value {
        // git's own monitor: none on Linux, whatever the setting says.
        "" | "false" | "true" if unix_there => State::Unsupported,
        "" | "false" => State::Off,
        // `true`, or a hook (Watchman) named.
        _ => State::On,
    }
}

/// A speed-up the doctor suggests: why, whether it is on, and the commands that turn it on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// Stable suggestion identity used for confirmation and special-case control decisions.
    pub title: &'static str,
    pub why: String,
    pub state: State,
    /// What turns it on, as git commands.
    pub ops: Vec<Vec<String>>,
}

impl Suggestion {
    /// Translate only the displayed title; unknown caller-supplied identities stay opaque.
    pub fn display_title(&self) -> &str {
        let id = match self.title {
            "Commit-graph" => "backend-health-commit-graph",
            "Scheduled maintenance" => "backend-scheduled-maintenance-f688c9",
            "File system monitor" => "backend-file-system-monitor-bd6322",
            "Untracked cache" => "backend-untracked-cache-de0942",
            "Pack the objects" => "backend-pack-the-objects-5a853d",
            _ => return self.title,
        };
        crate::i18n::text(id)
    }
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
        Some(t) => crate::i18n::message(
            if h.remote {
                "backend-health-status-remote"
            } else {
                "backend-health-status-local"
            },
            &[("milliseconds", (t.as_millis() as u64).into())],
        ),
        None => crate::i18n::text("backend-health-status-unknown").to_owned(),
    };
    // A partial clone would fetch every tree it lacks to compute the changed paths.
    let write_graph: &[&str] = if h.partial {
        &["commit-graph", "write", "--reachable"]
    } else {
        &["commit-graph", "write", "--reachable", "--changed-paths"]
    };
    let mut out = vec![
        Suggestion {
            title: "Commit-graph",
            why: if h.shallow {
                crate::i18n::text("backend-a-file-of-every-commit-s-parents-and-dates-that-makes-walking-his-c7c844")
                    .to_owned()
            } else {
                crate::i18n::text("backend-a-file-of-every-commit-s-parents-and-dates-that-makes-walking-his-302dd7")
                    .to_owned()
            },
            state: if h.shallow {
                State::Unsupported
            } else {
                on(h.commit_graph)
            },
            ops: ops(&[
                write_graph,
                &["config", "--local", "fetch.writeCommitGraph", "true"],
            ]),
        },
        Suggestion {
            title: "Scheduled maintenance",
            why: crate::i18n::text(
                "backend-git-maintenance-start-registers-the-repository-and-has-the-system-3c11ab",
            )
            .to_owned(),
            state: on(h.maintenance),
            ops: ops(&[&["maintenance", "start"]]),
        },
        Suggestion {
            title: "File system monitor",
            why: crate::i18n::message(
                if slow {
                    "backend-health-monitor-slow"
                } else {
                    "backend-health-monitor"
                },
                &[("status", status_note.as_str().into())],
            ),
            state: h.fsmonitor.unwrap_or(State::Off),
            ops: ops(&[&["config", "--local", "core.fsmonitor", "true"]]),
        },
        Suggestion {
            title: "Untracked cache",
            why: crate::i18n::message(
                "backend-health-untracked",
                &[("status", status_note.as_str().into())],
            ),
            state: on(h.untracked_cache),
            ops: ops(&[
                &["update-index", "--test-untracked-cache"],
                &["config", "--local", "core.untrackedCache", "true"],
                &["update-index", "--untracked-cache"],
            ]),
        },
    ];
    if h.objects.loose > MANY_LOOSE || (h.objects.packs > 50 && !h.partial) {
        out.push(Suggestion {
            title: "Pack the objects",
            why: crate::i18n::message(
                "backend-loose-objects-in-kib-and-packs-git-gc-packs-them-into-one-which-s-cc160c",
                &[
                    ("objects", h.objects.loose.into()),
                    ("kib", h.objects.loose_kib.into()),
                    ("packs", h.objects.packs.into()),
                ],
            ),
            state: State::Off,
            ops: ops(&[&["gc", "--quiet"]]),
        });
    }
    out
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod backend_fluent_suggestion_identity_tests;
