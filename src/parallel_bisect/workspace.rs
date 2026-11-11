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

mod repository;
#[cfg(test)]
pub(super) use repository::base_for;
pub(super) use repository::canonical_common;
use repository::*;

mod transfer;
use transfer::*;

#[cfg(test)]
mod tests;

mod completion;
pub(crate) use completion::close_finished;
#[cfg(test)]
pub(crate) use completion::supplied_transfer;
use completion::{replay_close, transfer_finished};

#[cfg(test)]
mod execution_bridge_tests;
