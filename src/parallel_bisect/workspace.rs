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
    close_presentation: Option<super::presentation::Presentation>,
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
            close_presentation: None,
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
    pub(super) fn execute(
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

    /// Prepare under actual-machine mutation authority, before the test can queue.
    /// A cancelled queued target can then return a real empty log, without an absence
    /// fallback. Failed creation/truncation is a setup error and admits no dispatch.
    pub(super) fn prepare_test_log(&self, n: usize) -> Result<String, String> {
        let log = format!("{}/{}.log", self.base_there, n);
        self.shell(": > \"$1\"", &[&log], None)?;
        Ok(log)
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

    pub(crate) fn send_with_presentation(
        &self,
        source: &Path,
        wanted: &[String],
        presentation: &mut Option<super::presentation::Presentation>,
    ) -> Result<(), String> {
        *presentation = None;
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
        let authority = self.authority(FenceMode::Mutation)?;
        let transfer = random_transfer()?;
        let attempt = bundle(source, &missing, &present, &authority.generation, &transfer)
            .and_then(|bundle| self.fetch_with_presentation(&bundle, presentation));
        match attempt {
            Ok(()) => Ok(()),
            // Only a confirmed Git failure may retry with the complete requested history.
            Err(TransferFailure::Git(_)) if !present.is_empty() => {
                *presentation = None;
                self.authority(FenceMode::Mutation)?;
                let whole = bundle(
                    source,
                    wanted,
                    &[],
                    &authority.generation,
                    &random_transfer()?,
                )
                .map_err(TransferFailure::message)?;
                self.fetch_with_presentation(&whole, presentation)
                    .map_err(TransferFailure::message)
            }
            Err(error) => Err(error.message()),
        }
    }

    /// Every chunk and repository mutation reaches the actual owner's generation fence.
    /// Ref creation is an expected-absent transaction, never a force update of shared refs.
    fn fetch_with_presentation(
        &self,
        bundle: &TransferBundle,
        presentation: &mut Option<super::presentation::Presentation>,
    ) -> Result<(), TransferFailure> {
        *presentation = None;
        let there = format!("{}/incoming-{}.bundle", self.base_there, bundle.nonce);
        let size = bundle.bytes.len().to_string();
        let digest = transfer_digest(&bundle.bytes);
        let mut created = false;
        let result = (|| {
            for (index, chunk) in bundle.bytes.chunks(16 << 20).enumerate() {
                let offset = (index * (16 << 20)).to_string();
                let end = (index * (16 << 20) + chunk.len()).to_string();
                self.shell(WRITE_CHUNK, &[&there, &offset, &end], Some(chunk.to_vec()))
                    .map_err(TransferFailure::Operation)?;
                created = true;
            }
            self.shell(VERIFY_TRANSFER, &[&there, &size, &digest], None)
                .map_err(TransferFailure::Operation)?;
            let mut args = vec!["fetch", "-q", "--no-tags", "--no-write-fetch-head", &there];
            args.extend(bundle.refs.iter().map(|(name, _)| name.as_str()));
            let fetched = self
                .run_git(&self.repo, &args, std::time::Duration::from_secs(1800))
                .map_err(TransferFailure::Operation)?;
            if fetched.code != Some(0) {
                return Err(TransferFailure::Git(
                    String::from_utf8_lossy(&fetched.stderr).into_owned(),
                ));
            }
            let authority = self
                .authority(FenceMode::Mutation)
                .map_err(TransferFailure::Operation)?;
            let record = self
                .owner_record(&authority)
                .map_err(TransferFailure::Operation)?;
            let mut owned = git::WorkspaceCommand::new(&authority);
            owned.input = Some(bundle.ref_transaction().into_bytes());
            let published = self
                .execute(
                    &self.repo,
                    &["update-ref", "--stdin"],
                    &record,
                    std::time::Duration::from_secs(30),
                    owned,
                )
                .map_err(TransferFailure::Operation)?;
            if published.code != Some(0) {
                return Err(TransferFailure::Operation(format!(
                    "could not create transfer refs: {}",
                    String::from_utf8_lossy(&published.stderr)
                )));
            }
            Ok(())
        })();
        // A failed/uncertain operation retains its primary error. A denied cleanup never
        // turns it into permission to repeat a Git operation or erase recovery evidence.
        let removed = if created {
            self.shell(REMOVE_TRANSFER, &[&there], None)
        } else {
            Ok(())
        };
        transfer_finished(result, removed, presentation)
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
        self.close_with_presentation(&mut None)
    }
    pub(crate) fn close_with_presentation(
        &mut self,
        presentation: &mut Option<super::presentation::Presentation>,
    ) -> Result<(), String> {
        *presentation = None;
        if self.closed {
            return replay_close(&self.close_error, &self.close_presentation, presentation);
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
        let result = close_finished(cleared, released, presentation);
        self.close_error = result.as_ref().err().cloned();
        self.close_presentation = presentation.clone();
        result
    }

    /// Simulate loss of a client after its operations completed, keeping real durable
    /// owner evidence for the next claimant. This fixture never leaves a live target.
    #[cfg(test)]
    pub(super) fn leave_completed_fixture(mut self) -> Result<(), String> {
        self.closed = true;
        self.lease
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close()
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
        close_presentation: None,
    };
    let _ = ws.clear();
    let _ = ws
        .lease
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .close();
}

/// The folder for bisecting in `repo` (on `machine`): as the app names it, and as a path there.
#[cfg(test)]
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

pub(super) fn canonical_common(repo: &Path) -> Result<String, String> {
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

#[derive(Debug)]
enum TransferFailure {
    Git(String),
    Operation(String),
}
impl TransferFailure {
    fn message(self) -> String {
        match self {
            Self::Git(s) | Self::Operation(s) => s,
        }
    }
}
struct TransferBundle {
    nonce: String,
    refs: Vec<(String, String)>,
    bytes: Vec<u8>,
}
impl TransferBundle {
    fn header(
        with: &[String],
        without: &[String],
        generation: &str,
        nonce: &str,
    ) -> Result<Self, String> {
        let valid_hex = |s: &str, size: usize| {
            s.len() == size
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        let width = with.first().map_or(0, String::len);
        if with.is_empty()
            || with.len() + without.len() > 4096
            || !matches!(width, 40 | 64)
            || !with.iter().chain(without).all(|h| valid_hex(h, width))
            || !valid_hex(generation, 64)
            || !valid_hex(nonce, 32)
        {
            return Err("invalid or excessive transfer commit identifiers".into());
        }
        let refs: Vec<_> = with
            .iter()
            .enumerate()
            .map(|(i, h)| {
                (
                    format!("refs/{FOLDER}/{generation}/sent-{nonce}-{i}"),
                    h.clone(),
                )
            })
            .collect();
        let mut header = if width == 64 {
            "# v3 git bundle\n@object-format=sha256\n".to_owned()
        } else {
            "# v2 git bundle\n".to_owned()
        };
        for hash in without {
            header.push_str(&format!("-{hash} gitcraft prerequisite\n"));
        }
        for (name, hash) in &refs {
            header.push_str(&format!("{hash} {name}\n"));
        }
        header.push('\n');
        Ok(Self {
            nonce: nonce.into(),
            refs,
            bytes: header.into_bytes(),
        })
    }
    fn ref_transaction(&self) -> String {
        let mut input = String::from("start\n");
        for (name, hash) in &self.refs {
            input.push_str(&format!("create {name} {hash}\n"));
        }
        input.push_str("prepare\ncommit\n");
        input
    }
}
fn random_transfer() -> Result<String, String> {
    let mut random = [0; 16];
    gitcraft_os::random_bytes(&mut random).map_err(|e| e.to_string())?;
    Ok(random.iter().map(|b| format!("{b:02x}")).collect())
}
fn transfer_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
/// Assemble the documented bundle header around read-only pack output. No source refs
/// are created, updated or removed, including failure/cancellation paths.
fn bundle(
    source: &Path,
    with: &[String],
    without: &[String],
    generation: &str,
    nonce: &str,
) -> Result<TransferBundle, TransferFailure> {
    let mut bundle = TransferBundle::header(with, without, generation, nonce)
        .map_err(TransferFailure::Operation)?;
    let mut revisions = with.join("\n");
    revisions.push('\n');
    for hash in without {
        revisions.push_str(&format!("^{hash}\n"));
    }
    let pack = git::transfer_pack(
        source,
        revisions.into_bytes(),
        !without.is_empty(),
        MAX_SEND - bundle.bytes.len(),
    )
    .map_err(TransferFailure::Operation)?;
    if pack.code != Some(0) {
        return Err(TransferFailure::Git(format!(
            "could not pack transfer: {}",
            String::from_utf8_lossy(&pack.stderr)
        )));
    }
    if !pack.stdout.starts_with(b"PACK") || bundle.bytes.len() + pack.stdout.len() > MAX_SEND {
        return Err(TransferFailure::Operation("invalid pack or history exceeds 256 MiB transfer limit; clone the repository there and name its folder".into()));
    }
    bundle.bytes.extend(pack.stdout);
    Ok(bundle)
}
const WRITE_CHUNK: &str = r#"set -eu
f=$1; offset=$2; end=$3
[ ! -L "$f" ] || exit 1
if [ "$offset" = 0 ]; then (umask 077; set -C; : >"$f") || exit 1; fi
[ -f "$f" ] && [ "$(wc -c <"$f" | tr -d '[:space:]')" = "$offset" ] || exit 1
cat >>"$f" || exit 1
[ "$(wc -c <"$f" | tr -d '[:space:]')" = "$end" ] || exit 1
"#;
const VERIFY_TRANSFER: &str = r#"set -eu
[ ! -L "$1" ] && [ -f "$1" ] && [ "$(wc -c <"$1" | tr -d '[:space:]')" = "$2" ] || exit 1
if command -v sha256sum >/dev/null 2>&1; then digest=$(sha256sum "$1")
elif command -v shasum >/dev/null 2>&1; then digest=$(shasum -a 256 "$1")
elif command -v python3 >/dev/null 2>&1; then digest=$(python3 -c 'import hashlib,sys; h=hashlib.sha256(); f=open(sys.argv[1],"rb"); exec("while True:\n b=f.read(1048576)\n if not b: break\n h.update(b)\n"); print(h.hexdigest())' "$1")
else digest=$(perl -MDigest::SHA -e 'open(my $f,"<",$ARGV[0]) or die $!; binmode($f); print Digest::SHA->new(256)->addfile($f)->hexdigest' "$1"); fi
case "$digest" in "$3"|"$3 "*) ;; *) printf '%s\n' 'transferred bundle digest differs' >&2; exit 1;; esac
"#;
const REMOVE_TRANSFER: &str = r#"set -eu
[ ! -L "$1" ] || exit 1
if [ -e "$1" ]; then [ -f "$1" ] && rm -- "$1" || exit 1; fi
[ ! -e "$1" ] && [ ! -L "$1" ]
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_transfer_has_no_source_ref_mutations_and_creates_exact_generation_refs()
    -> Result<(), Box<dyn std::error::Error>> {
        for width in [40, 64] {
            let wanted = vec!["a".repeat(width), "b".repeat(width)];
            let present = vec!["c".repeat(width)];
            let bundle =
                TransferBundle::header(&wanted, &present, &"d".repeat(64), &"e".repeat(32))?;
            let header = String::from_utf8(bundle.bytes.clone())?;
            assert!(header.starts_with(if width == 40 {
                "# v2 git bundle\n"
            } else {
                "# v3 git bundle\n@object-format=sha256\n"
            }));
            assert!(header.contains(&format!("-{} gitcraft prerequisite\n", present[0])));
            assert!(header.ends_with("\n\n"));
            for (name, hash) in &bundle.refs {
                assert!(
                    name.starts_with(&format!("refs/gitcraft-bisect/{}/sent-", "d".repeat(64)))
                );
                assert!(header.contains(&format!("{hash} {name}\n")));
            }
            let transaction = bundle.ref_transaction();
            assert!(transaction.starts_with("start\ncreate refs/gitcraft-bisect/"));
            assert!(!transaction.contains("update ") && !transaction.contains("delete "));
            assert!(transaction.ends_with("prepare\ncommit\n"));
        }
        for bad in [
            "HEAD".to_owned(),
            "a".repeat(39),
            format!("{}\nrefs/heads/main", "a".repeat(40)),
        ] {
            assert!(TransferBundle::header(&[bad], &[], &"d".repeat(64), &"e".repeat(32)).is_err());
        }
        assert_eq!(
            transfer_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        Ok(())
    }
    #[test]
    fn recording_cleanup_requires_exact_generation_and_structured_complete_receipt()
    -> Result<(), Box<dyn std::error::Error>> {
        let authority = WorkspaceAuthority {
            root: "/synthetic".into(),
            anchor: "original".into(),
            generation: "a".repeat(64),
            mode: FenceMode::Mutation,
        };
        let mut receipt = serde_json::json!({"v":1,"state":"cleaned","anchor":"original","generation":authority.generation,"records":2,"records_sha256":"b".repeat(64)});
        assert!(cleaned_receipt(&serde_json::to_vec(&receipt)?, &authority));
        receipt["generation"] = "old".into();
        assert!(!cleaned_receipt(&serde_json::to_vec(&receipt)?, &authority));
        assert!(!cleaned_receipt(b"true", &authority));
        Ok(())
    }
}

// Pure finishing seams used by the actual transfer and close branches.
fn transfer_finished(
    result: Result<(), TransferFailure>,
    removed: Result<(), String>,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<(), TransferFailure> {
    *presentation = None;
    match (result, removed) {
        (result, Ok(())) => result,
        (Ok(()), Err(error)) => Err(TransferFailure::Operation(error)),
        (Err(primary), Err(cleanup)) => {
            let primary = primary.message();
            let raw = format!("{primary}; transfer cleanup: {cleanup}");
            *presentation = Some(super::presentation::Presentation::capture(
                raw.clone(),
                super::presentation::Cleanup::WorkspaceTransfer { primary, cleanup },
            ));
            Err(TransferFailure::Operation(raw))
        }
    }
}
pub(crate) fn close_finished(
    cleared: Result<(), String>,
    released: Result<(), String>,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<(), String> {
    *presentation = None;
    match (cleared, released) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
        (Err(primary), Err(release)) => {
            let raw = format!("{primary}; lease release: {release}");
            *presentation = Some(super::presentation::Presentation::capture(
                raw.clone(),
                super::presentation::Cleanup::WorkspaceLeaseRelease { primary, release },
            ));
            Err(raw)
        }
    }
}

fn replay_close(
    error: &Option<String>,
    saved: &Option<super::presentation::Presentation>,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<(), String> {
    let result = error.clone().map_or(Ok(()), Err);
    *presentation = saved.clone();
    super::presentation::retain_error(&result, presentation);
    result
}
#[cfg(test)]
pub(crate) fn supplied_transfer(
    primary: &str,
    cleanup: Option<&str>,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<(), String> {
    transfer_finished(
        Err(TransferFailure::Operation(primary.into())),
        cleanup.map_or(Ok(()), |e| Err(e.into())),
        presentation,
    )
    .map_err(TransferFailure::message)
}
#[cfg(test)]
mod execution_bridge_tests {
    use super::*;
    #[test]
    fn execution_bridge_close_replays_only_matching_error_and_clears_other_results() {
        let mut receipt = None;
        let result = close_finished(
            Err("owner; lease release: provider".into()),
            Err("lease); framing".into()),
            &mut receipt,
        );
        let saved = receipt.clone();
        for _ in 0..2 {
            assert_eq!(
                replay_close(&result.as_ref().err().cloned(), &saved, &mut receipt),
                result
            );
            assert!(receipt.is_some());
        }
        assert!(replay_close(&None, &saved, &mut receipt).is_ok());
        assert!(receipt.is_none());
        assert!(replay_close(&Some("unrelated".into()), &saved, &mut receipt).is_err());
        assert!(receipt.is_none());
        let mut receipt = saved;
        assert!(
            transfer_finished(
                Err(TransferFailure::Git("git denied".into())),
                Ok(()),
                &mut receipt
            )
            .is_err()
        );
        assert!(receipt.is_none());
        assert!(matches!(
            transfer_finished(
                Err(TransferFailure::Git("git denied".into())),
                Ok(()),
                &mut receipt
            ),
            Err(TransferFailure::Git(_))
        ));
        assert!(transfer_finished(Ok(()), Ok(()), &mut receipt).is_ok());
        assert!(receipt.is_none());
    }
}
