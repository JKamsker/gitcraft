//! Where a parallel bisect works on the machine its tests run on: a repository there (the
//! tab's own when it is on that machine, else a copy the commits are sent to), and a folder of
//! its own for that repository, `~/.cache/gitcraft/bisect/run-<hash of its git folder>`, which
//! holds the worktrees, the test logs and what says who is using it. Outside the repository, so
//! that what the tests build there isn't seen by the tab's watcher. Everything the bisect makes
//! is in there (and under `refs/gitcraft-bisect/`), so taking it away is all the cleaning up
//! there is: when the bisect ends however it ends, and when the next one starts after a crash
//! (or the tab opens).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use super::machine::Machine;
use super::runner;
use crate::git;
use crate::remote;

/// The refs the bisect makes, under `refs/`.
const FOLDER: &str = "gitcraft-bisect";

/// Where the folders are, under the home folder; each is `run-<hash>`.
const RUNS: &str = ".cache/gitcraft/bisect";

/// What says the folder is in use: `gitcraft <process> <unix seconds>`, rewritten every
/// [`HEARTBEAT`] while the bisect runs.
const OWNER: &str = "owner";

/// How often the owner is rewritten.
const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(20);

/// A folder whose owner wasn't rewritten for this long is a crashed bisect's.
const STALE_AFTER: i64 = 90;

/// The most history sent to another machine in one go (through its shell).
const MAX_SEND: usize = 256 << 20;

/// Numbers the bundles made in this process, so that two bisects sending at once never share a
/// ref.
static SENDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The folders of the bisects running in this process.
static ACTIVE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub struct Workspace {
    pub machine: Machine,
    /// The repository the worktrees are made from, as the app names it.
    pub repo: PathBuf,
    /// Its `gitcraft-bisect` folder, as the app names it…
    pub base: PathBuf,
    /// …and as a path on its machine.
    pub base_there: String,
    /// Held while a worktree is added.
    adding: Mutex<()>,
    /// The thread saying the folder is in use runs while this holds.
    beating: Arc<AtomicBool>,
    closed: bool,
}

impl Workspace {
    /// Makes ready the folder on `machine` for a bisect of `source`'s commits: in `source`
    /// itself when it is on that machine and no other repository is named (`repo_there`, a
    /// path there), else in a copy kept there for it. What a crashed bisect left there is
    /// cleared first; one still running refuses.
    pub fn open(
        source: &Path,
        machine: &Machine,
        repo_there: Option<&str>,
    ) -> Result<Self, String> {
        let repo = repository_for(source, machine, repo_there)?;
        let (base, base_there) = base_for(&repo, machine)?;
        claim(&base)?;
        if let Err(e) = refuse_if_in_use(&base, machine) {
            release(&base);
            return Err(e);
        }
        // From here on, dropping it releases the claim (and clears what is there).
        let ws = Workspace {
            machine: machine.clone(),
            repo,
            base,
            base_there,
            adding: Mutex::new(()),
            beating: Arc::default(),
            closed: false,
        };
        ws.clear()?;
        ws.make_private().map_err(|e| {
            format!(
                "could not make {} on {}: {e}",
                ws.base_there,
                ws.machine.label()
            )
        })?;
        ws.keep_beating();
        Ok(ws)
    }

    /// Says the folder is in use now and every [`HEARTBEAT`] until it is closed, from a thread
    /// of its own: through every step (sending the commits, reading the range, the rounds).
    fn keep_beating(&self) {
        self.beating.store(true, Ordering::Relaxed);
        let (beating, owner) = (Arc::clone(&self.beating), remote::join(&self.base, OWNER));
        std::thread::spawn(move || {
            let mut last: Option<std::time::Instant> = None;
            while beating.load(Ordering::Relaxed) {
                if last.is_none_or(|at| at.elapsed() >= HEARTBEAT) {
                    last = Some(std::time::Instant::now());
                    let line = format!("gitcraft {} {}\n", std::process::id(), git::now());
                    let _ = remote::write(&owner, line.as_bytes());
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    }

    /// Worktree `n`'s folder: as the app names it, and as a path there.
    pub fn slot(&self, n: usize) -> (PathBuf, String) {
        let name = format!("bisect-{n}");
        (
            remote::join(&self.base, &name),
            format!("{}/{name}", self.base_there),
        )
    }

    /// Runs `add` (which adds a worktree) while no other worktree is being added.
    pub fn adding<T>(&self, add: impl FnOnce() -> T) -> T {
        let _held = self.adding.lock().unwrap_or_else(PoisonError::into_inner);
        add()
    }

    /// Sends the commits `wanted` (and their history) to the repository there, when it is not
    /// `source` and lacks some of them: as a bundle, with only what it lacks when it has the
    /// rest, else whole.
    pub fn send(&self, source: &Path, wanted: &[String]) -> Result<(), String> {
        if remote::same_dir(source, &self.repo) {
            return Ok(());
        }
        let (present, missing): (Vec<String>, Vec<String>) = wanted
            .iter()
            .cloned()
            .partition(|h| has_commit(&self.repo, h));
        if missing.is_empty() {
            return Ok(());
        }
        let thin = bundle(source, &missing, &present).and_then(|b| self.fetch(&b));
        match thin {
            Ok(()) => Ok(()),
            // The commits it has may lack their own history (a shallow clone…): all of it.
            Err(_) if !present.is_empty() => {
                let whole = bundle(source, wanted, &[])?;
                self.fetch(&whole)
            }
            Err(e) => Err(e),
        }
    }

    /// Takes in the commits of `bundle` there, under `refs/gitcraft-bisect/`.
    fn fetch(&self, bundle: &[u8]) -> Result<(), String> {
        let file = remote::join(&self.base, "incoming.bundle");
        remote::write(&file, bundle).map_err(|e| {
            format!(
                "could not write the commits to send on {}: {e}",
                self.machine.label()
            )
        })?;
        let there = format!("{}/incoming.bundle", self.base_there);
        let refspec = format!("+refs/{FOLDER}/send-*:refs/{FOLDER}/sent-*");
        let fetched = git::run(
            Some(&self.repo),
            &[
                "fetch",
                "-q",
                "--no-tags",
                "--no-write-fetch-head",
                &there,
                &refspec,
            ],
        );
        let _ = runner::shell(&self.repo, "rm -f \"$1\"", &[&there]);
        fetched.map(|_| ()).map_err(|e| {
            format!(
                "could not send the commits to {}: {e}",
                self.machine.label()
            )
        })
    }

    /// Takes away everything a bisect made there: stops its tests that still run, removes its
    /// worktrees and its folder, and its refs.
    fn clear(&self) -> Result<(), String> {
        // Every record is authenticated before CLEAR may mutate worktrees or refs. Keep
        // the exact bytes as a final barrier against replaced/new records during recovery.
        let listing = runner::shell(&self.repo, RECORDS, &[&self.base_there])?;
        let mut retired = Vec::new();
        let proof = clear_proof(&listing, |name| {
            let path = remote::join(&self.base, name);
            let bytes = remote::recover_owner(&path).map_err(|e| {
                format!(
                    "unresolved test owner {name}; {} preserved: {e}",
                    self.base_there
                )
            })?;
            retired.push((path, bytes.clone()));
            Ok(bytes)
        })?;
        let pid = std::process::id().to_string();
        let input = format!("{}\n", proof.join("\n")).into_bytes();
        if input.len() > 64 << 20 {
            return Err("recovery proof exceeds 64 MiB; workspace preserved".into());
        }
        let result = runner::shell_input(&self.repo, CLEAR, &[&self.base_there, &pid], Some(input))
            .map(|_| ())
            .map_err(|e| {
                format!(
                    "could not clear {} on {}: {e}",
                    self.base_there,
                    self.machine.label()
                )
            });
        if result.is_ok() {
            for (path, bytes) in retired {
                let _ = remote::retire_owner(&path, &bytes);
            }
        }
        result
    }

    fn make_private(&self) -> Result<(), String> {
        let parent = self.base.parent().ok_or("bisect folder has no parent")?;
        remote::create_dir_all(parent).map_err(|e| e.to_string())?;
        if remote::parse(&self.base).is_some() {
            runner::shell(
                &self.repo,
                "umask 077; mkdir -- \"$1\"",
                &[&self.base_there],
            )
            .map(|_| ())
        } else {
            gitcraft_os::create_private_directory(&self.base).map_err(|e| e.to_string())
        }
    }

    /// Clears what the bisect made (once; dropping it does the same).
    pub fn close(&mut self) -> Result<(), String> {
        if std::mem::replace(&mut self.closed, true) {
            return Ok(());
        }
        self.beating.store(false, Ordering::Relaxed);
        let cleared = self.clear();
        release(&self.base);
        cleared
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// Refuses the folder `base` when another gitcraft is using it (its owner rewritten lately).
fn refuse_if_in_use(base: &Path, machine: &Machine) -> Result<(), String> {
    let Ok(owner) = remote::read_to_string(&remote::join(base, OWNER)) else {
        return Ok(());
    };
    let ours = owner.split_whitespace().nth(1) == Some(std::process::id().to_string().as_str());
    let at: i64 = owner
        .split_whitespace()
        .nth(2)
        .and_then(|w| w.parse().ok())
        .unwrap_or(0);
    // This process's own (claimed by none of its bisects, or `claim` would have refused):
    // left by one whose clean-up failed.
    if !ours && git::now() - at < STALE_AFTER {
        return Err(format!(
            "another gitcraft window is running a parallel bisect in this repository on {}; \
             cancel it there first",
            machine.label()
        ));
    }
    Ok(())
}

/// Clears what a crashed bisect left in `repo`'s git folder, if anything; one running (here
/// or in another window) is left alone.
pub fn clear_leftovers(repo: &Path) {
    let machine = Machine::of(repo);
    let Ok((base, base_there)) = base_for(repo, &machine) else {
        return;
    };
    if !remote::try_is_dir(&base).unwrap_or(false) || claim(&base).is_err() {
        return;
    }
    if refuse_if_in_use(&base, &machine).is_err() {
        release(&base);
        return;
    }
    let mut ws = Workspace {
        machine,
        repo: repo.to_path_buf(),
        base,
        base_there,
        adding: Mutex::new(()),
        beating: Arc::default(),
        closed: false,
    };
    let _ = ws.close();
}

/// The folder for bisecting in `repo` (on `machine`): as the app names it, and as a path there.
pub fn base_for(repo: &Path, machine: &Machine) -> Result<(PathBuf, String), String> {
    let common = git_common_dir(repo)?;
    let home = machine.home()?.replace('\\', "/");
    let digest = format!("{:x}", md5::compute(common.as_bytes()));
    let there = format!(
        "{}/{RUNS}/run-{}",
        home.trim_end_matches('/'),
        &digest[..12]
    );
    Ok((machine.path(&there), there))
}

/// List names only. Legacy PID files are recovery uncertainty, never kill authority.
const RECORDS: &str = r#"for f in "$1"/*.owner "$1"/*.pid; do
  [ -e "$f" ] || [ -L "$f" ] || continue
  printf '%s\000' "${f##*/}"
done
true"#;

fn record_names(listing: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for name in listing.split('\0').filter(|n| !n.is_empty()) {
        let valid = name
            .strip_suffix(".owner")
            .and_then(|n| n.split_once('-'))
            .is_some_and(|(slot, nonce)| {
                !slot.is_empty()
                    && slot.bytes().all(|b| b.is_ascii_digit())
                    && nonce.len() == 32
                    && nonce
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            });
        if !valid {
            return Err(format!(
                "unresolved legacy or invalid owner record {name}; workspace preserved; no signals sent"
            ));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

fn clear_proof(
    listing: &str,
    mut recover: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<String>, String> {
    // Validate the entire set first: a legacy/unknown record grants no signal authority.
    let names = record_names(listing)?;
    let mut proof = Vec::new();
    for name in names {
        let bytes = recover(&name)?;
        proof.push(name);
        proof.push(bytes.iter().map(|b| format!("{b:02x}")).collect());
    }
    Ok(proof)
}

/// Verify exact acknowledged records again before any mutation. Never prune unrelated
/// worktrees. Coordination heartbeat PID is used only to retain this app's send refs.
const CLEAR: &str = r#"b=$1
ours=$2
shift 2
case $b in */.cache/gitcraft/bisect/run-*) ;; *) echo "gitcraft: $b is no bisect folder" >&2; exit 1;; esac
for f in "$b"/*.pid; do
  if [ -e "$f" ] || [ -L "$f" ]; then echo 'gitcraft: unresolved legacy owner; workspace preserved' >&2; exit 1; fi
done
for f in "$b"/*.owner; do
  [ -e "$f" ] || [ -L "$f" ] || continue
  IFS= read -r expected && IFS= read -r receipt || { echo 'gitcraft: owner records changed; workspace preserved' >&2; exit 1; }
  if [ "${f##*/}" != "$expected" ] || [ -L "$f" ]; then echo 'gitcraft: owner records changed; workspace preserved' >&2; exit 1; fi
  value=$(head -c 2049 "$f" | od -An -v -tx1) || exit 1
  value=$(printf '%s' "$value" | tr -d ' \n')
  if [ "$value" != "$receipt" ]; then echo 'gitcraft: owner record replaced; workspace preserved' >&2; exit 1; fi
done
if IFS= read -r extra && [ -n "$extra" ]; then echo 'gitcraft: owner record missing; workspace preserved' >&2; exit 1; fi
git worktree list --porcelain | sed -n 's/^worktree //p' | while IFS= read -r w; do
  case $w in "$b"/*) git worktree remove --force --force "$w" >/dev/null 2>&1;; esac
done
rm -rf "$b"
c=$(git rev-parse --git-common-dir) && for g in "$c"/worktrees/*/gitdir; do
  [ -f "$g" ] || continue
  case $(cat "$g") in "$b"/*) rm -rf "${g%/gitdir}";; esac
done
git for-each-ref --format='%(refname)' refs/gitcraft-bisect/ | while IFS= read -r r; do
  case $r in refs/gitcraft-bisect/send-"$ours"-*) ;; *) git update-ref -d "$r";; esac
done
true"#;

/// Claims `base` for this process's bisect, refusing one already claimed (another tab).
fn claim(base: &Path) -> Result<(), String> {
    let mut active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
    if active.iter().any(|b| b == base) {
        return Err(
            "a parallel bisect is already running in this repository; cancel it first".to_owned(),
        );
    }
    active.push(base.to_path_buf());
    Ok(())
}

fn release(base: &Path) {
    let mut active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
    active.retain(|b| b != base);
}

/// The repository the bisect works in on `machine` (see [`Workspace::open`]).
fn repository_for(
    source: &Path,
    machine: &Machine,
    repo_there: Option<&str>,
) -> Result<PathBuf, String> {
    let named = repo_there.map(str::trim).filter(|p| !p.is_empty());
    if let Some(named) = named {
        let there = match named.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => {
                format!("{}{rest}", machine.home()?)
            }
            _ => named.to_owned(),
        };
        let repo = machine.path(&there);
        git::run(Some(&repo), &["rev-parse", "--git-dir"])
            .map_err(|_| format!("{there} on {} is not a git repository", machine.label()))?;
        return Ok(repo);
    }
    if *machine == Machine::of(source) {
        return Ok(source.to_path_buf());
    }
    scratch_repository(source, machine)
}

/// A repository of its own on `machine` for bisecting `source`'s commits there, made the first
/// time: `~/.cache/gitcraft/bisect/<name>-<hash of where source is>`. It keeps the commits
/// sent to it, so that the next bisect sends less.
fn scratch_repository(source: &Path, machine: &Machine) -> Result<PathBuf, String> {
    let name: String = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let digest = format!("{:x}", md5::compute(source.to_string_lossy().as_bytes()));
    let home = machine.home()?;
    let there = format!(
        "{}/.cache/gitcraft/bisect/{}-{}",
        home.trim_end_matches(['/', '\\']),
        name.trim_start_matches('.'),
        &digest[..8]
    );
    let from = match machine {
        Machine::Here => None,
        Machine::There(_) => Some(machine.path(&home)),
    };
    git::run(from.as_deref(), &["init", "-q", &there]).map_err(|e| {
        format!(
            "could not make a repository for the bisect on {}: {e}",
            machine.label()
        )
    })?;
    Ok(machine.path(&there))
}

/// `repo`'s git folder shared by its worktrees, as an absolute path on its machine.
fn git_common_dir(repo: &Path) -> Result<String, String> {
    let printed = git::run(
        Some(repo),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .or_else(|_| git::run(Some(repo), &["rev-parse", "--git-common-dir"]))?;
    let printed = printed.trim();
    let absolute = printed.starts_with('/') || printed.get(1..2) == Some(":");
    Ok(if absolute {
        printed.to_owned()
    } else {
        format!(
            "{}/{printed}",
            remote::git_path(repo).trim_end_matches(['/', '\\'])
        )
    })
}

fn has_commit(repo: &Path, hash: &str) -> bool {
    git::run(
        Some(repo),
        &["cat-file", "-e", &format!("{hash}^{{commit}}")],
    )
    .is_ok()
}

/// A bundle of the commits `with` (and their history), without what `without` reaches. The
/// refs it needs are made in `source` for a moment.
fn bundle(source: &Path, with: &[String], without: &[String]) -> Result<Vec<u8>, String> {
    let names: Vec<String> = (0..with.len())
        .map(|i| {
            let seq = SENDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            format!("refs/{FOLDER}/send-{}-{seq}-{i}", std::process::id())
        })
        .collect();
    let made: Result<(), String> = names.iter().zip(with).try_for_each(|(name, hash)| {
        git::run(Some(source), &["update-ref", name, hash]).map(|_| ())
    });
    let mut args: Vec<&str> = vec!["bundle", "create", "-q", "-"];
    args.extend(names.iter().map(String::as_str));
    if !without.is_empty() {
        args.push("--not");
        args.extend(without.iter().map(String::as_str));
    }
    let bytes = made.and_then(|()| git::run_bytes(Some(source), &args));
    for name in &names {
        let _ = git::run(Some(source), &["update-ref", "-d", name]);
    }
    let bytes = bytes.map_err(|e| format!("could not bundle the commits to send: {e}"))?;
    if bytes.len() > MAX_SEND {
        return Err(format!(
            "the history to send is {} MB, more than gitcraft sends through a shell; clone the \
             repository there and name its folder instead",
            bytes.len() >> 20
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_recovery_requires_every_receipt_and_rejects_legacy_before_control() {
        let name = format!("0-{}.owner", "a".repeat(32));
        let mut controlled = Vec::new();
        let invalid = format!("{name}\0old.pid\0");
        assert!(
            clear_proof(&invalid, |n| {
                controlled.push(n.to_owned());
                Ok(b"not used".to_vec())
            })
            .is_err()
        );
        assert!(
            controlled.is_empty(),
            "legacy files must not grant control authority"
        );
        let valid = format!("{name}\0");
        assert!(clear_proof(&valid, |_| Err("missing receipt".into())).is_err());
        assert_eq!(
            clear_proof(&valid, |_| Ok(b"verified endpoint\n".to_vec())).unwrap_or_default(),
            [
                name.clone(),
                "766572696669656420656e64706f696e740a".to_owned()
            ]
        );
        for invalid in [
            "../../escape.owner\0",
            "0-abcd.owner\0",
            "owner\0",
            "0-a.pid\0",
        ] {
            assert!(record_names(invalid).is_err());
        }
    }

    /// Two bisects never share a folder in one process; the second one is told why.
    #[test]
    fn a_folder_is_claimed_once() {
        let base = std::env::temp_dir().join(format!("gitcraft-claim-{}", std::process::id()));
        assert!(claim(&base).is_ok());
        let again = claim(&base);
        assert!(again.is_err_and(|e| e.contains("already running")));
        release(&base);
        assert!(claim(&base).is_ok());
        release(&base);
    }
}
