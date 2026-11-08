//! A retained actual-machine claim. It performs no cleanup or repository dispatch.
use super::workspace_authority::{FenceMode, WorkspaceAuthority};
use crate::{processes, tasks::Cancellation};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
const MARKER: &[u8] = b"__GITCRAFT_WORKSPACE_CLAIM__ ";
const LIFETIME: Duration = Duration::from_secs(7 * 86400);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    root: std::path::PathBuf,
    generation: String,
    milliseconds: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    root: std::path::PathBuf,
    anchor: String,
    generation: String,
}
#[derive(Default)]
struct Frame {
    bytes: Vec<u8>,
    done: bool,
}
impl Frame {
    fn feed(&mut self, bytes: &[u8], generation: &str) -> Option<WorkspaceAuthority> {
        if self.done {
            return None;
        }
        for byte in bytes {
            if *byte == b'\n' {
                let line = std::mem::take(&mut self.bytes);
                if let Some(value) = line.strip_prefix(MARKER) {
                    self.done = true;
                    let reply: Reply = serde_json::from_slice(value).ok()?;
                    if reply.generation != generation
                        || reply.anchor.is_empty()
                        || reply.anchor.len() > 128
                        || !(reply.root.is_absolute()
                            || reply.root.to_str().is_some_and(|p| p.starts_with('/')))
                    {
                        return None;
                    }
                    return Some(WorkspaceAuthority {
                        root: reply.root,
                        anchor: reply.anchor,
                        generation: reply.generation,
                        mode: FenceMode::Mutation,
                    });
                }
            } else if self.bytes.len() < 4096 {
                self.bytes.push(*byte);
            } else {
                self.done = true;
                return None;
            }
        }
        None
    }
}
pub(crate) struct Lease {
    ticket: Option<processes::Ticket>,
    authority: WorkspaceAuthority,
    lost: Arc<std::sync::atomic::AtomicBool>,
    cancel: Cancellation,
}
impl Lease {
    pub(crate) fn native(root: &Path) -> Result<Self, String> {
        let mut random = [0; 32];
        gitcraft_os::random_bytes(&mut random).map_err(|e| e.to_string())?;
        let generation: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let request = Request {
            root: root.into(),
            generation: generation.clone(),
            milliseconds: LIFETIME.as_millis() as u64,
        };
        let mut input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        input.push(b'\n');
        let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        #[cfg(not(test))]
        command.arg("--gitcraft-workspace-claim");
        #[cfg(test)]
        command
            .args([
                "--exact",
                "remote::workspace_claim::tests::helper_entrypoint",
                "--nocapture",
            ])
            .env("GITCRAFT_CLAIM_HELPER", "1");
        Self::start(command, input, generation, true)
    }
    pub(crate) fn remote(host: &super::Host, root: &str) -> Result<Self, String> {
        let mut random = [0; 32];
        gitcraft_os::random_bytes(&mut random).map_err(|e| e.to_string())?;
        let generation: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let request = Request {
            root: root.into(),
            generation: generation.clone(),
            milliseconds: LIFETIME.as_millis() as u64,
        };
        let data = format!(
            "{}\n",
            serde_json::to_string(&request).map_err(|e| e.to_string())?
        );
        let (command, input, connected) = super::session::claim_command(host, &data);
        let lease = Self::start(command, input, generation, false)?;
        if let Some(connected) = connected {
            connected();
        }
        Ok(lease)
    }
    pub(super) fn start(
        command: Command,
        input: Vec<u8>,
        generation: String,
        native: bool,
    ) -> Result<Self, String> {
        let cancel = crate::tasks::cancellation();
        let (send, receive) = mpsc::sync_channel(1);
        let mut frame = Frame::default();
        let lost = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ticket = processes::submit_lease(
            command,
            processes::Options {
                native_controller: native,
                input: Some(input),
                hold_input: true,
                cancel: cancel.clone(),
                timeout: Some(LIFETIME),
                output_limit: 4096,
                stderr_limit: Some(4096),
                stdout: Some(Box::new(move |bytes| {
                    if let Some(reply) = frame.feed(bytes, &generation) {
                        let _ = send.try_send(reply);
                    }
                })),
                ..Default::default()
            },
        )?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Ok(authority) = receive.try_recv() {
                return Ok(Self {
                    ticket: Some(ticket),
                    authority,
                    lost,
                    cancel,
                });
            }
            if let Some(result) = ticket.try_result() {
                return Err(format!(
                    "workspace claim unavailable: {}",
                    match result {
                        Err(e) => e,
                        Ok(out) => String::from_utf8_lossy(&out.stderr).into_owned(),
                    }
                ));
            }
            if cancel.is_cancelled() || Instant::now() >= deadline {
                ticket.cancel();
                return Err("workspace claim cancelled or handshake expired".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub(crate) fn authority(&self, mode: FenceMode) -> Result<WorkspaceAuthority, String> {
        use std::sync::atomic::Ordering;
        if self.cancel.is_cancelled()
            || self
                .ticket
                .as_ref()
                .is_none_or(|t| t.try_result().is_some())
        {
            self.lost.store(true, Ordering::Release);
        }
        if self.lost.load(Ordering::Acquire) {
            return Err("workspace claim lost; mutation refused".into());
        }
        let mut authority = self.authority.clone();
        authority.mode = mode;
        Ok(authority)
    }
    pub(crate) fn close(&mut self) -> Result<(), String> {
        if let Some(ticket) = self.ticket.take() {
            ticket.cancel();
            let out = ticket.wait()?;
            if out.stop_kind == Some(processes::StopKind::Uncertain) {
                return Err(out
                    .stopped
                    .unwrap_or_else(|| "workspace lease release unknown".into()));
            }
        }
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket.take() {
            ticket.cancel();
        }
    }
}
fn helper() -> Result<i32, String> {
    gitcraft_os::detach_controller().map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::new(std::io::stdin());
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(4097)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4096 || !bytes.ends_with(b"\n") {
        return Err("invalid workspace claim request".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !request.root.is_absolute()
        || request.milliseconds == 0
        || request.milliseconds > LIFETIME.as_millis() as u64
    {
        return Err("invalid workspace claim lifetime/root".into());
    }
    let cancel = Cancellation::default();
    let eof = cancel.clone();
    std::thread::spawn(move || {
        let mut byte = [0];
        loop {
            match reader.read(&mut byte) {
                Ok(0) | Err(_) => {
                    eof.cancel();
                    break;
                }
                Ok(_) => {}
            }
        }
    });
    let deadline = Instant::now() + Duration::from_millis(request.milliseconds);
    let claim = gitcraft_os::WorkspaceClaim::acquire(&request.root, &request.generation, || {
        !cancel.is_cancelled() && Instant::now() < deadline
    })
    .map_err(|e| e.to_string())?;
    let grant = claim.grant();
    if cancel.is_cancelled() {
        return Err("workspace claim cancelled before acknowledgement".into());
    }
    let reply = Reply {
        root: grant.root,
        anchor: grant.anchor,
        generation: grant.generation,
    };
    let bytes = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
    std::io::stdout()
        .write_all(MARKER)
        .and_then(|()| std::io::stdout().write_all(&bytes))
        .and_then(|()| std::io::stdout().write_all(b"\n"))
        .and_then(|()| std::io::stdout().flush())
        .map_err(|e| e.to_string())?;
    while !cancel.is_cancelled() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(claim);
    Ok(0)
}
pub(crate) fn entrypoint() -> i32 {
    match helper() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("gitcraft workspace claim: {e}");
            125
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    #[ignore = "runtime hold: exact source-reviewed invocation only"]
    fn owned_runtime_claim_fence_turnover() -> Result<(), Box<dyn std::error::Error>> {
        use super::super::{native_owner, owned_runtime as fixture, workspace_authority};
        // The existing native Lease API has a singleton, dedicated Service lane. This
        // exact test owns that lane; no other test may run in this invocation.
        struct LeaseLane;
        impl Drop for LeaseLane {
            fn drop(&mut self) {
                processes::shutdown_leases();
            }
        }
        struct PendingClaim {
            cancel: Cancellation,
            thread: Option<std::thread::JoinHandle<()>>,
        }
        impl Drop for PendingClaim {
            fn drop(&mut self) {
                self.cancel.cancel();
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
        }
        let mut scratch = fixture::Scratch::new()?;
        let _leases = LeaseLane;
        let service = processes::owned_fixture_service()?;
        let canary = fixture::canary(&service, &scratch.0)?;
        fixture::challenge(&canary, &scratch.0, b"before-claim")?;
        let root = scratch.0.join("ledger");
        let mut winner = Lease::native(&root)?;
        let authority = winner.authority(FenceMode::Mutation)?;
        let generation = std::fs::read(root.join("generation"))?;
        assert_eq!(generation, format!("{}\n", authority.generation).as_bytes());
        let namespace_before: std::collections::BTreeSet<_> = std::fs::read_dir(&root)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<_, _>>()?;
        // Acquisition fails before exposing authority: the loser cannot publish, clear
        // or dispatch through a Lease, and cannot change the winner's namespace.
        let loser = Lease::native(&root);
        assert!(loser.is_err());
        let error = loser.err().ok_or("busy claimant unexpectedly acquired")?;
        assert!(error.contains("another workspace claimant"), "{error}");
        assert_eq!(std::fs::read(root.join("generation"))?, generation);
        let namespace_after: std::collections::BTreeSet<_> = std::fs::read_dir(&root)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<_, _>>()?;
        assert_eq!(namespace_after, namespace_before);
        assert_eq!(
            winner.authority(FenceMode::Mutation)?.generation,
            authority.generation
        );

        let operation = scratch.0.join("operation");
        std::fs::create_dir(&operation)?;
        let name = format!("{}-mutation-{}.owner", authority.generation, "a".repeat(32));
        let record = root.join(&name);
        let native = native_owner::command_with_authority(
            fixture::command("gate", &operation)?,
            &record,
            fixture::LIMIT,
            Some(authority.clone()),
        )?;
        let mut options = fixture::options();
        let command = native.configure(&mut options);
        let mutation = service.submit(command, options)?;
        fixture::until(|| Ok(operation.join("target-ready").exists()))?;
        let owner_bytes = std::fs::read(&record)?;
        let endpoint_bytes =
            workspace_authority::bound_endpoint(&name, &owner_bytes, &authority.anchor)?;
        let endpoint = native_owner::Endpoint::from_record(&endpoint_bytes)?;
        assert!(endpoint.terminal().is_err());
        assert!(!operation.join("mutation-effect").exists());
        // Cancellation of this retained controller closes only its owned input. The
        // claim helper sees EOF and exits; no numeric signal or target stop is used.
        let lease_ticket = winner.ticket.take().ok_or("winner ticket missing")?;
        lease_ticket.cancel();
        let released = fixture::wait(&lease_ticket)?;
        assert_eq!(released.stop_kind, Some(processes::StopKind::Cancelled));
        assert_eq!(released.status.code(), Some(0));
        assert!(winner.authority(FenceMode::Mutation).is_err());
        drop(winner);

        let cancel = Cancellation::default();
        let child_cancel = cancel.clone();
        let child_root = root.clone();
        let (send, receive) = mpsc::sync_channel(1);
        let pending = PendingClaim {
            cancel,
            thread: Some(
                std::thread::Builder::new()
                    .name("finite-claim-turnover".into())
                    .spawn(move || {
                        let result = crate::tasks::with_cancellation(child_cancel, || {
                            Lease::native(&child_root)
                        });
                        let _ = send.send(result);
                    })?,
            ),
        };
        // The old helper has exited. A busy primary now proves the new actual helper
        // owns primary and is waiting behind the mutation owner's kernel fence.
        let primary = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("primary"))?;
        fixture::until(|| match primary.try_lock() {
            Ok(()) => {
                primary.unlock().map_err(|e| e.to_string())?;
                Ok(false)
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(std::fs::TryLockError::Error(error)) => Err(error.to_string()),
        })?;
        assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert_eq!(std::fs::read(root.join("generation"))?, generation);
        assert_eq!(std::fs::read(&record)?, owner_bytes);
        assert!(endpoint.terminal().is_err());
        fixture::challenge(&canary, &scratch.0, b"fence-retained-after-eof")?;

        std::fs::write(operation.join("release"), b"release owned mutation")?;
        let output = fixture::wait(&mutation)?;
        assert!(output.stopped.is_none(), "{:?}", output.stopped);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, fixture::stdout(b"mutation-complete\n"));
        assert!(output.stderr.is_empty());
        assert_eq!(
            std::fs::read(operation.join("mutation-effect"))?,
            b"owned mutation"
        );
        let receipt = endpoint.terminal()?;
        assert!(endpoint.confirms(&receipt));
        let proof: serde_json::Value = serde_json::from_slice(&receipt)?;
        assert_eq!(proof["code"], 0);
        assert_eq!(proof["complete"], true);
        let mut successor = receive.recv_timeout(fixture::LIMIT)??;
        let new_authority = successor.authority(FenceMode::Mutation)?;
        assert_ne!(new_authority.generation, authority.generation);
        assert_eq!(new_authority.anchor, authority.anchor);
        assert_eq!(
            std::fs::read(root.join("generation"))?,
            format!("{}\n", new_authority.generation).as_bytes()
        );
        // Real WorkspaceFence/Grant rejects old authority before any publication.
        let deadline = Instant::now() + fixture::LIMIT;
        assert!(
            gitcraft_os::WorkspaceFence::acquire(&authority.native_grant(), || Instant::now()
                < deadline)
            .is_err()
        );
        let stale_directory = scratch.0.join("stale");
        std::fs::create_dir(&stale_directory)?;
        let stale_record = root.join(format!(
            "{}-mutation-{}.owner",
            authority.generation,
            "b".repeat(32)
        ));
        let stale = native_owner::command_with_authority(
            fixture::command("touch", &stale_directory)?,
            &stale_record,
            fixture::LIMIT,
            Some(authority),
        )?;
        let mut options = fixture::options();
        let command = stale.configure(&mut options);
        let refused = service.submit(command, options)?;
        let refused = fixture::wait(&refused)?;
        assert_eq!(refused.status.code(), Some(125));
        assert_eq!(refused.stop_kind, Some(processes::StopKind::Uncertain));
        assert!(String::from_utf8_lossy(&refused.stderr).contains("stale workspace generation"));
        assert!(!stale_record.exists());
        assert!(!stale_directory.join("unexpected-effect").exists());
        assert_eq!(std::fs::read(&record)?, owner_bytes);
        assert_eq!(endpoint.terminal()?, receipt);
        assert_eq!(super::super::recover_owner(&record)?, owner_bytes);
        let fence = gitcraft_os::WorkspaceFence::acquire(&new_authority.native_grant(), || {
            Instant::now() < deadline
        })?;
        fence.verify(&new_authority.native_grant())?;
        drop(fence);
        endpoint.retire_proven(&receipt)?;
        endpoint.retire_proven(&receipt)?;
        fixture::challenge(&canary, &scratch.0, b"after-turnover-and-recovery")?;
        successor.close()?;
        drop(pending);
        canary.cancel();
        assert_eq!(
            fixture::wait(&canary)?.stop_kind,
            Some(processes::StopKind::Cancelled)
        );
        drop(service);
        processes::shutdown_leases();
        scratch.complete();
        Ok(())
    }
    #[test]
    fn helper_entrypoint() {
        if std::env::var_os("GITCRAFT_CLAIM_HELPER").is_some() {
            std::process::exit(entrypoint());
        }
    }
    #[test]
    fn recording_claim_acknowledgement_requires_exact_generation_and_complete_bounded_frame()
    -> Result<(), Box<dyn std::error::Error>> {
        let generation = "a".repeat(64);
        let reply = Reply {
            root: "/synthetic/repo/ledger".into(),
            anchor: "unix:1:2".into(),
            generation: generation.clone(),
        };
        let mut frame = MARKER.to_vec();
        frame.extend(serde_json::to_vec(&reply)?);
        frame.push(b'\n');
        for split in 0..frame.len() {
            let mut parser = Frame::default();
            assert!(parser.feed(&frame[..split], &generation).is_none());
            assert!(parser.feed(&frame[split..], &generation).is_some());
            assert!(parser.feed(&frame, &generation).is_none());
        }
        assert!(Frame::default().feed(&frame, &"b".repeat(64)).is_none());
        assert!(
            Frame::default()
                .feed(&vec![b'x'; 4097], &generation)
                .is_none()
        );
        Ok(())
    }
}
