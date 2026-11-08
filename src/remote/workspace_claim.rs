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
                        || !reply.root.is_absolute()
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
    #[test]
    fn helper_entrypoint() {
        if std::env::var_os("GITCRAFT_CLAIM_HELPER").is_some() {
            std::process::exit(entrypoint());
        }
    }
    #[test]
    fn recording_claim_acknowledgement_requires_exact_generation_and_complete_bounded_frame() {
        let generation = "a".repeat(64);
        let reply = Reply {
            root: "/synthetic/repo/ledger".into(),
            anchor: "unix:1:2".into(),
            generation: generation.clone(),
        };
        let mut frame = MARKER.to_vec();
        frame.extend(serde_json::to_vec(&reply).unwrap());
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
    }
}
