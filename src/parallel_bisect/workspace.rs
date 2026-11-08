//! Machine-side bisect workspace. A stable external ledger and retained claim live under
//! the canonical Git common directory; the removable work folder is its separate sibling.
//! Git metadata paths keep build output outside the tab's watched working tree. Every
//! mutation/test carries an immutable generation to its actual durable owner.

use crate::remote::workspace_authority::{FenceMode, WorkspaceAuthority};
use crate::remote::workspace_claim::Lease;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use super::machine::Machine;
use super::runner;
use crate::git;
use crate::remote;

/// The refs the bisect makes, under `refs/`.
const FOLDER: &str = "gitcraft-bisect";

/// The most history sent to another machine in one go (through its shell).
const MAX_SEND: usize = 256 << 20;

/// Numbers the bundles made in this process, so that two bisects sending at once never share a
/// ref.
static SENDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    lease: Mutex<Lease>,
    uncertain: AtomicBool,
    close_error: Option<String>,
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
        let common = canonical_common(&repo)?;
        let root = format!("{common}/gitcraft-bisect-owners");
        let lease = match machine {
            Machine::Here => Lease::native(Path::new(&root))?,
            Machine::There(host) => Lease::remote(host, &root)?,
        };
        let base_there = format!("{common}/gitcraft-bisect-work");
        // Cleanup-enabled ownership is constructed only after actual-machine claim success.
        // Initialization errors leave Drop with only its own lease to release.
        let mut ws = Workspace {
            machine: machine.clone(),
            repo,
            base: machine.path(&base_there),
            base_there,
            adding: Mutex::new(()),
            lease: Mutex::new(lease),
            uncertain: AtomicBool::new(false),
            closed: true,
            close_error: None,
        };
        ws.clear()?;
        ws.make_private()?;
        ws.closed = false;
        Ok(ws)
    }
    pub(crate) fn authority(&self, mode: FenceMode) -> Result<WorkspaceAuthority, String> {
        if self.uncertain.load(Ordering::Acquire) {
            return Err(
                "workspace ownership uncertain; external recovery evidence retained".into(),
            );
        }
        self.lease
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .authority(mode)
    }
    pub(crate) fn owner_record(&self, authority: &WorkspaceAuthority) -> Result<PathBuf, String> {
        let mut random = [0; 16];
        gitcraft_os::random_bytes(&mut random).map_err(|e| e.to_string())?;
        let nonce: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let mode = match authority.mode {
            FenceMode::Dispatch => "dispatch",
            FenceMode::Mutation => "mutation",
        };
        let root = authority.root.to_string_lossy().replace('\\', "/");
        Ok(self.machine.path(&format!(
            "{root}/{}-{mode}-{nonce}.owner",
            authority.generation
        )))
    }
    fn execute(
        &self,
        dir: &Path,
        args: &[&str],
        record: &Path,
        limit: std::time::Duration,
        owned: git::WorkspaceCommand<'_>,
    ) -> Result<git::Exit, String> {
        let result = crate::processes::checked(git::run_workspace_command(
            dir,
            args,
            record,
            limit,
            crate::tasks::cancellation(),
            owned,
        ));
        let out = match result {
            Ok(out) => out,
            Err(error) => {
                if matches!(error, crate::processes::Failure::Failed(_)) {
                    self.uncertain.store(true, Ordering::Release);
                }
                return Err(error.message());
            }
        };
        if let Some(reason) = out.stopped {
            if out.stop_kind == Some(crate::processes::StopKind::Uncertain) {
                self.uncertain.store(true, Ordering::Release);
            }
            return Err(format!(
                "{reason}; {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(git::Exit {
            code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }
    pub(crate) fn invalidate(&self) {
        self.uncertain.store(true, Ordering::Release);
    }
    pub(crate) fn run_git(
        &self,
        dir: &Path,
        args: &[&str],
        limit: std::time::Duration,
    ) -> Result<git::Exit, String> {
        let authority = self.authority(FenceMode::Mutation)?;
        let record = self.owner_record(&authority)?;
        self.execute(
            dir,
            args,
            &record,
            limit,
            git::WorkspaceCommand::new(&authority),
        )
    }
    fn shell(&self, script: &str, args: &[&str], input: Option<Vec<u8>>) -> Result<(), String> {
        let authority = self.authority(FenceMode::Mutation)?;
        let record = self.owner_record(&authority)?;
        let words = runner::sh_alias("gitcraft-bisect-mutation", script, args);
        let words: Vec<_> = words.iter().map(String::as_str).collect();
        let mut owned = git::WorkspaceCommand::new(&authority);
        owned.input = input;
        let out = self.execute(
            &self.repo,
            &words,
            &record,
            std::time::Duration::from_secs(1800),
            owned,
        )?;
        if out.code == Some(0) {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
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

    fn clear(&self) -> Result<(), String> {
        let authority = self.authority(FenceMode::Mutation)?;
        let record = self.owner_record(&authority)?;
        let ledger = authority.root.to_string_lossy().replace('\\', "/");
        let own = record
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("invalid cleanup owner name")?;
        let words = runner::sh_alias(
            "gitcraft-bisect-clear",
            CLEAR,
            &[&self.base_there, &ledger, own, &authority.generation],
        );
        let words: Vec<_> = words.iter().map(String::as_str).collect();
        let mut owned = git::WorkspaceCommand::new(&authority);
        owned.cleanup = true;
        let out = self.execute(
            &self.repo,
            &words,
            &record,
            std::time::Duration::from_secs(1800),
            owned,
        )?;
        if out.code != Some(0) {
            return Err(format!(
                "workspace cleanup partial; external ledger retained: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        if !cleaned_receipt(&out.stdout, &authority) {
            self.uncertain.store(true, Ordering::Release);
            return Err("missing or mismatched cleaned receipt; external ledger retained".into());
        }
        Ok(())
    }
    fn make_private(&self) -> Result<(), String> {
        if matches!(self.machine, Machine::Here) {
            let authority = self.authority(FenceMode::Mutation)?;
            let record = self.owner_record(&authority)?;
            let mut owned = git::WorkspaceCommand::new(&authority);
            owned.directory = Some(&self.base);
            let out = self.execute(
                &self.repo,
                &["--version"],
                &record,
                std::time::Duration::from_secs(30),
                owned,
            )?;
            if out.code == Some(0) {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).into_owned())
            }
        } else {
            self.shell("umask 077; mkdir -- \"$1\"", &[&self.base_there], None)
        }
    }
    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return self.close_error.clone().map_or(Ok(()), Err);
        }
        self.closed = true;
        let cleared = if self.uncertain.load(Ordering::Acquire) {
            Err("workspace ownership uncertain; external recovery evidence retained".into())
        } else {
            self.clear()
        };
        let released = self
            .lease
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close();
        let result = match (cleared, released) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
            (Err(primary), Err(release)) => Err(format!("{primary}; lease release: {release}")),
        };
        self.close_error = result.as_ref().err().cloned();
        result
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.close();
        }
    }
}
fn cleaned_receipt(bytes: &[u8], authority: &WorkspaceAuthority) -> bool {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Receipt {
        v: u8,
        state: String,
        anchor: String,
        generation: String,
        records: usize,
        records_sha256: String,
    }
    bytes.len() <= 4096
        && serde_json::from_slice::<Receipt>(bytes).is_ok_and(|r| {
            r.v == 1
                && r.state == "cleaned"
                && r.anchor == authority.anchor
                && r.generation == authority.generation
                && r.records <= 4096
                && r.records_sha256.len() == 64
                && r.records_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}
pub fn clear_leftovers(repo: &Path) {
    let machine = Machine::of(repo);
    let Ok(common) = canonical_common(repo) else {
        return;
    };
    let root = format!("{common}/gitcraft-bisect-owners");
    // A read avoids initializing an empty coordination namespace for every tab open.
    if !remote::try_is_dir(&machine.path(&root)).unwrap_or(false) {
        return;
    }
    let lease = match &machine {
        Machine::Here => Lease::native(Path::new(&root)),
        Machine::There(host) => Lease::remote(host, &root),
    };
    let Ok(lease) = lease else {
        return;
    };
    let base_there = format!("{common}/gitcraft-bisect-work");
    let ws = Workspace {
        base: machine.path(&base_there),
        base_there,
        machine,
        repo: repo.into(),
        adding: Mutex::new(()),
        lease: Mutex::new(lease),
        uncertain: AtomicBool::new(false),
        closed: true,
        close_error: None,
    };
    let _ = ws.clear();
    let _ = ws
        .lease
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .close();
}

/// The folder for bisecting in `repo` (on `machine`): as the app names it, and as a path there.
pub fn base_for(repo: &Path, machine: &Machine) -> Result<(PathBuf, String), String> {
    let there = format!("{}/gitcraft-bisect-work", canonical_common(repo)?);
    Ok((machine.path(&there), there))
}
const CLEAR: &str = r#"LC_ALL=C; export LC_ALL
b=$1
ledger=$2
own=$3
generation=$4
shift 4
fail() { printf 'gitcraft: %s; external recovery ledger retained\n' "$1" >&2; exit 1; }
[ "$b" = "${ledger%/*}/gitcraft-bisect-work" ] || fail 'cleanup base is outside claimed repository'
[ ! -L "$b" ] && [ ! -L "$ledger" ] || fail 'workspace or ledger became a symbolic link'
for f in "$b"/*.pid "$b"/*.owner "$b"/owner; do
  [ ! -e "$f" ] && [ ! -L "$f" ] || fail 'unresolved legacy evidence in workspace base'
done
IFS=' ' read -r kind count || fail 'missing generation proof'
[ "$kind" = G ] || fail 'invalid generation proof'
case $count in ''|*[!0-9]*) fail 'invalid generation count';; esac
[ "$count" -gt 0 ] && [ "$count" -le 4097 ] || fail 'generation proof bound'
generations=''
i=0
while [ "$i" -lt "$count" ]; do
  IFS= read -r g || fail 'truncated generation proof'
  [ "${#g}" -eq 64 ] || fail 'invalid generation length'
  case $g in *[!0-9a-f]*) fail 'invalid generation';; esac
  generations="$generations $g"
  i=$((i+1))
done
case " $generations " in *" $generation "*) ;; *) fail 'current generation missing';; esac
IFS=' ' read -r kind count || fail 'missing record proof'
[ "$kind" = R ] || fail 'invalid record proof'
case $count in ''|*[!0-9]*) fail 'invalid record count';; esac
[ "$count" -le 4096 ] || fail 'record proof bound'
i=0
for f in "$ledger"/*.owner; do
  [ -e "$f" ] || [ -L "$f" ] || continue
  [ "${f##*/}" != "$own" ] || continue
  IFS= read -r expected && IFS= read -r proof || fail 'owner records changed'
  [ "${f##*/}" = "$expected" ] && [ ! -L "$f" ] || fail 'owner record changed'
  # The actual fenced owner validated private regular files before this bounded reread.
  actual=$(od -An -v -tx1 -N 2049 "$f") || fail 'cannot inspect owner record'
  actual=$(printf '%s' "$actual" | tr -d ' \n') || fail 'cannot encode owner record'
  [ "${#actual}" -le 4096 ] && [ "$actual" = "$proof" ] || fail 'owner record replaced'
  i=$((i+1))
done
[ "$i" -eq "$count" ] || fail 'owner record set changed'
if IFS= read -r extra; then fail 'unexpected trailing recovery proof'; fi
refs=$(git for-each-ref --format='%(refname) %(objectname)' refs/gitcraft-bisect/) || fail 'cannot list bisect refs'
while IFS=' ' read -r r expected; do
  [ -n "$r" ] || continue
  suffix=${r#refs/gitcraft-bisect/}
  g=${suffix%%/*}
  case " $generations " in *" $g "*) ;; *) fail "ref has no recovered generation: $r";; esac
done <<GITCRAFT_REFS
$refs
GITCRAFT_REFS
worktrees=$(git -c core.quotePath=false worktree list --porcelain) || fail 'cannot list worktrees'
while IFS= read -r line; do
  case $line in 'worktree "'*) fail 'quoted worktree path requires manual recovery';; esac
done <<GITCRAFT_WORKTREES
$worktrees
GITCRAFT_WORKTREES
while IFS= read -r line; do
  case $line in 'worktree '*) w=${line#worktree }; case $w in "$b"/*)
    git worktree remove --force --force "$w" >/dev/null || fail "cannot remove worktree $w";; esac;; esac
done <<GITCRAFT_WORKTREES
$worktrees
GITCRAFT_WORKTREES
remaining=$(git -c core.quotePath=false worktree list --porcelain) || fail 'cannot verify remaining worktrees'
while IFS= read -r line; do
  case $line in "worktree $b/"*) fail 'a bisect worktree remains registered';; esac
done <<GITCRAFT_WORKTREES
$remaining
GITCRAFT_WORKTREES
c=$(git rev-parse --git-common-dir) || fail 'cannot resolve worktree metadata'
for g in "$c"/worktrees/*/gitdir; do
  [ -f "$g" ] || continue
  target=$(cat "$g") || fail 'cannot read worktree metadata'
  case $target in "$b"/*)
    rm -rf -- "${g%/gitdir}" || fail 'cannot remove bisect worktree metadata'
    [ ! -e "${g%/gitdir}" ] && [ ! -L "${g%/gitdir}" ] || fail 'worktree metadata remains';;
  esac
done
while IFS=' ' read -r r expected; do
  [ -n "$r" ] || continue
  git update-ref -d "$r" "$expected" || fail "cannot remove unchanged ref $r"
done <<GITCRAFT_REFS
$refs
GITCRAFT_REFS
remaining=$(git for-each-ref --format='%(refname)' refs/gitcraft-bisect/) || fail 'cannot verify remaining refs'
[ -z "$remaining" ] || fail 'bisect refs remain'
[ ! -L "$b" ] || fail 'workspace became a symbolic link'
rm -rf -- "$b" || fail "cannot remove workspace $b"
[ ! -e "$b" ] && [ ! -L "$b" ] || fail 'workspace remains after removal'
"#;

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

fn canonical_common(repo: &Path) -> Result<String, String> {
    let common = git_common_dir(repo)?;
    let canonical = if remote::parse(repo).is_some() {
        runner::shell(repo, "cd \"$1\" && pwd -P", &[&common])?
            .trim()
            .to_owned()
    } else {
        crate::app::strip_verbatim(std::fs::canonicalize(&common).map_err(|e| e.to_string())?)
            .to_string_lossy()
            .replace('\\', "/")
    };
    if canonical.contains(['\n', '\r', '\0'])
        || !(canonical.starts_with('/') || canonical.get(1..2) == Some(":"))
    {
        return Err("unsupported canonical repository path".into());
    }
    Ok(canonical)
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
    fn recording_cleanup_requires_exact_generation_and_structured_complete_receipt() {
        let authority = WorkspaceAuthority {
            root: "/synthetic".into(),
            anchor: "original".into(),
            generation: "a".repeat(64),
            mode: FenceMode::Mutation,
        };
        let mut receipt = serde_json::json!({"v":1,"state":"cleaned","anchor":"original","generation":authority.generation,"records":2,"records_sha256":"b".repeat(64)});
        assert!(cleaned_receipt(
            &serde_json::to_vec(&receipt).unwrap(),
            &authority
        ));
        receipt["generation"] = "old".into();
        assert!(!cleaned_receipt(
            &serde_json::to_vec(&receipt).unwrap(),
            &authority
        ));
        assert!(!cleaned_receipt(b"true", &authority));
    }
}
