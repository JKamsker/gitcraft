//! Explicit installed-helper capability for remote identity-safe independence.
//! Remote calls reuse the existing bounded Git alias transport. No helper is deployed.
use crate::{connections, remote};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
const PROTOCOL: &str = "gitcraft-owned-objects/1";
const LIMIT: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub device: u64,
    pub inode: u64,
    pub birth_seconds: u64,
    pub birth_nanos: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ticket {
    pub repo: String,
    pub primary: String,
    pub transaction: String,
    pub identities: Vec<Identity>,
    pub refs: Vec<u8>,
    pub topology: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    repo: String,
    ticket: Option<Ticket>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    protocol: String,
    result: Result<Payload, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
enum Payload {
    Prepared(Ticket),
    Completed {
        transaction: String,
        message: String,
    },
}
#[derive(Clone, Debug)]
pub(super) struct RemotePlan {
    pub repo: PathBuf,
    profile: connections::Context,
    ticket: Ticket,
    attempted: Arc<AtomicBool>,
}
fn request(
    repo: &Path,
    ticket: Option<Ticket>,
    active: &dyn Fn() -> bool,
) -> Result<Payload, String> {
    if !active() {
        return Err(
            "Object independence was cancelled before dispatch; no helper request was sent.".into(),
        );
    }
    let target = remote::parse(repo).ok_or("Owned helper requires an SSH/WSL repository")?;
    request_target(&target, ticket, active)
}
fn request_target(
    target: &remote::Target,
    ticket: Option<Ticket>,
    active: &dyn Fn() -> bool,
) -> Result<Payload, String> {
    if !active() {
        return Err(
            "Object independence was cancelled before dispatch; no helper request was sent.".into(),
        );
    }
    let payload = serde_json::to_vec(&Request {
        protocol: PROTOCOL.into(),
        repo: target.path.clone(),
        ticket,
    })
    .map_err(|e| e.to_string())?;
    if payload.len() > LIMIT {
        return Err(
            "Owned-helper snapshot exceeds 4 MiB; use Git on the repository machine.".into(),
        );
    }
    // All shell text is constant; the path and snapshot travel only as bounded JSON stdin.
    let mut req = super::super::command::object_storage_request(
        target,
        &[
            "-c",
            "alias.gitcraft-owned-objects=!gitcraft-object-storage-helper",
            "gitcraft-owned-objects",
        ],
        payload,
        LIMIT,
    );
    req.timeout = Some(Duration::from_secs(20));
    let out = super::super::command::comparison_remote(&target.host, req, active)?;
    parse_reply(out)
}
fn parse_reply(out: remote::Output) -> Result<Payload, String> {
    if out.stdout.len() > LIMIT || out.stderr.len() > 64 * 1024 {
        return Err("Owned-helper reply exceeds its byte limit".into());
    }
    if out.code != 0 {
        return Err(format!(
            "Install the matching gitcraft-object-storage-helper executable in PATH on the repository machine to enable owned object independence ({PROTOCOL}). No helper is installed automatically. Read-only inspection remains available. {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let reply: Reply = serde_json::from_slice(&out.stdout).map_err(|_| {
        "The installed helper did not return a complete ownership protocol reply".to_owned()
    })?;
    if reply.protocol != PROTOCOL {
        return Err("The installed GitCraft helper uses an incompatible ownership protocol; update it before retrying.".into());
    }
    reply.result
}
pub(super) fn prepare(repo: &Path, active: &dyn Fn() -> bool) -> Result<RemotePlan, String> {
    let Payload::Prepared(ticket) = request(repo, None, active)? else {
        return Err("The owned helper did not prepare an identity snapshot".into());
    };
    let target = remote::parse(repo).ok_or("Missing remote target")?;
    if ticket.repo != target.path
        || ticket.identities.is_empty()
        || ticket.identities.len() > 4096
        || ticket.transaction.is_empty()
        || ticket.transaction.len() > 64
        || !ticket.transaction.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("The owned helper returned an invalid repository identity ticket".into());
    }
    Ok(RemotePlan {
        repo: repo.into(),
        profile: connections::current(),
        ticket,
        attempted: Arc::new(AtomicBool::new(false)),
    })
}
fn claim(plan: &RemotePlan, active: &dyn Fn() -> bool) -> Result<(), String> {
    if !active() {
        return Err(
            "Object independence was cancelled before dispatch; no helper request was sent.".into(),
        );
    }
    if plan.profile != connections::current() {
        return Err("Profile changed; prepare remote independence again.".into());
    }
    // A prepared mutation is one deliberate attempt, shared by every Snapshot clone.
    // Unknown/lost/malformed outcomes never turn the same ticket into a retry action.
    plan.attempted.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "This prepared independence attempt was already used. Inspect borrowing and recovery evidence again before another deliberate action.".to_owned())?;
    Ok(())
}
pub(super) fn execute(plan: &RemotePlan, active: &dyn Fn() -> bool) -> Result<String, String> {
    claim(plan, active)?;
    let recovery = format!(
        "{}/info/alternates.gitcraft-backup-{}",
        plan.ticket.primary, plan.ticket.transaction
    );
    let result=request(&plan.repo,Some(plan.ticket.clone()),active).map_err(|e|format!("{e}. No completion proof was received; do not assume independence or retry automatically. Inspect borrowing and the recovery backup (if created) on that machine: {recovery}"))?;
    completed(result, &plan.ticket.transaction, &recovery)
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    fn plan() -> RemotePlan {
        RemotePlan {
            repo: PathBuf::from("ssh://owned-helper-fixture.invalid/recording-only/repo"),
            profile: connections::current(),
            ticket: Ticket {
                repo: "/recording-only/repo".into(),
                primary: "/recording-only/repo/.git/objects".into(),
                transaction: "abc123".into(),
                identities: Vec::new(),
                refs: Vec::new(),
                topology: Vec::new(),
            },
            attempted: Arc::new(AtomicBool::new(false)),
        }
    }
    fn output(value: serde_json::Value) -> remote::Output {
        remote::Output {
            code: 0,
            stdout: value.to_string().into_bytes(),
            stderr: Vec::new(),
        }
    }
    #[test]
    fn recording_prepared_attempt_is_single_use_across_clones_and_cancel_does_not_claim()
    -> Result<(), String> {
        connections::Context::default().run(|| {
            let plan = plan();
            let copy = plan.clone();
            assert!(claim(&plan, &|| false).is_err_and(|e| e.contains("no helper request")));
            assert!(!plan.attempted.load(Ordering::Acquire));
            claim(&plan, &|| true)?;
            assert!(claim(&copy, &|| true).is_err_and(|e| e.contains("already used")));
            Ok(())
        })
    }
    #[test]
    fn recording_changed_profile_cannot_claim_a_prepared_attempt() {
        connections::Context::default().run(|| {
            let mut plan = plan();
            plan.profile.error = Some("previous profile".into());
            assert!(claim(&plan, &|| true).is_err_and(|e| e.contains("Profile changed")));
            assert!(!plan.attempted.load(Ordering::Acquire));
        });
    }
    #[test]
    fn recording_only_exact_complete_protocol_and_transaction_prove_success() -> Result<(), String>
    {
        let good = serde_json::json!({"protocol":PROTOCOL,"result":{"Ok":{"kind":"Completed","data":{"transaction":"abc123","message":"verified"}}}});
        assert_eq!(
            completed(parse_reply(output(good.clone()))?, "abc123", "backup")?,
            "verified"
        );
        assert!(completed(parse_reply(output(good.clone()))?, "different", "backup").is_err());
        let mut wrong = good.clone();
        wrong["protocol"] = "gitcraft-owned-objects/999".into();
        assert!(parse_reply(output(wrong)).is_err());
        let mut extra = good.clone();
        extra["result"]["Ok"]["data"]["unexpected"] = true.into();
        assert!(parse_reply(output(extra)).is_err());
        let bytes = good.to_string().into_bytes();
        for cut in 0..bytes.len() {
            assert!(
                parse_reply(remote::Output {
                    code: 0,
                    stdout: bytes[..cut].into(),
                    stderr: Vec::new()
                })
                .is_err()
            );
        }
        assert!(
            parse_reply(remote::Output {
                code: 0,
                stdout: vec![b' '; LIMIT + 1],
                stderr: Vec::new()
            })
            .is_err()
        );
        assert!(
            parse_reply(remote::Output {
                code: 127,
                stdout: Vec::new(),
                stderr: b"helper: not found".to_vec()
            })
            .is_err_and(|e| e.contains("Install the matching"))
        );
        Ok(())
    }
}
fn completed(result: Payload, expected: &str, recovery: &str) -> Result<String, String> {
    match result {
        Payload::Completed {
            transaction,
            message,
        } if transaction == expected => Ok(message),
        _ => Err(format!(
            "Missing/mismatched helper completion proof. Inspect borrowing and {recovery}; no automatic retry was performed."
        )),
    }
}

#[cfg(target_os = "linux")]
fn process(request: Request) -> Result<Payload, String> {
    use super::independent::local;
    if request.protocol != PROTOCOL {
        return Err("Incompatible ownership helper protocol".into());
    }
    let repo = PathBuf::from(&request.repo);
    if !repo.is_absolute() || remote::is_remote(&repo) {
        return Err("The helper accepts only absolute local repository paths".into());
    }
    let primary = PathBuf::from(super::line(
        &repo,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
        &|| true,
    )?);
    let topology = super::topology::read(&repo, &primary, &|| true)?;
    let snapshot = local::prepare(&repo, &primary, &topology, &|| true)?;
    match request.ticket {
        None => Ok(Payload::Prepared(local::export_ticket(
            &snapshot,
            local::fresh_transaction(),
            &|| true,
        )?)),
        Some(expected) => {
            if expected.repo != request.repo {
                return Err("Prepared repository path does not match this request".into());
            }
            let actual = local::export_ticket(&snapshot, expected.transaction.clone(), &|| true)?;
            if actual != expected {
                return Err("Actual ancestor/store/file identities, refs or borrowing changed. Prepare independence again.".into());
            }
            let message = local::make_transaction(
                &snapshot,
                &|| true,
                &mut |_| Ok(()),
                &expected.transaction,
            )?;
            Ok(Payload::Completed {
                transaction: expected.transaction,
                message,
            })
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn process(_request: Request) -> Result<Payload, String> {
    Err("The remote owned-helper capability requires a Linux filesystem with stable birth-time identities. Read-only inspection remains available.".into())
}
pub(crate) fn entrypoint() -> i32 {
    if std::env::args().nth(1).as_deref() == Some("--capabilities") {
        let capabilities = serde_json::json!({"protocol":PROTOCOL,"package_version":env!("CARGO_PKG_VERSION"),"linux_owned_identity":cfg!(target_os="linux"),"requires_filesystem_birth_time":true,"max_protocol_bytes":LIMIT});
        return if writeln!(std::io::stdout(), "{capabilities}").is_ok() {
            0
        } else {
            1
        };
    }
    if std::env::args().nth(1).is_some() {
        eprintln!("Use --capabilities or provide one bounded ownership protocol request on stdin.");
        return 2;
    }
    serve()
}
pub(crate) fn run_cli() -> Option<i32> {
    if std::env::args().nth(1).as_deref() != Some("--gitcraft-object-storage-helper") {
        return None;
    }
    Some(serve())
}
pub(super) fn serve() -> i32 {
    let result = (|| {
        let mut input = Vec::new();
        std::io::stdin()
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > LIMIT {
            return Err("Helper input exceeds 4 MiB".into());
        }
        let request: Request = serde_json::from_slice(&input)
            .map_err(|_| "Invalid ownership helper request".to_owned())?;
        process(request)
    })();
    let reply = Reply {
        protocol: PROTOCOL.into(),
        result,
    };
    let bytes=match serde_json::to_vec(&reply) {Ok(bytes) if bytes.len()<=LIMIT=>bytes,_=>b"{\"protocol\":\"gitcraft-owned-objects/1\",\"result\":{\"Err\":\"Helper reply exceeds 4 MiB\"}}".to_vec()};
    let mut stdout = std::io::stdout();
    if stdout
        .write_all(&bytes)
        .and_then(|_| stdout.write_all(b"\n"))
        .is_ok()
    {
        0
    } else {
        1
    }
}

#[cfg(test)]
pub(super) fn fixture_roundtrip(target: &remote::Target) -> Result<String, String> {
    let Payload::Prepared(ticket) = request_target(target, None, &|| true)? else {
        return Err("no prepared ticket".into());
    };
    let transaction = ticket.transaction.clone();
    match request_target(target, Some(ticket), &|| true)? {
        Payload::Completed {
            transaction: done,
            message,
        } if done == transaction => Ok(message),
        _ => Err("No exact completion proof".into()),
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(super) fn fixture_server_prepare(repo: &Path) -> Result<Ticket, String> {
    match process(Request {
        protocol: PROTOCOL.into(),
        repo: repo.to_string_lossy().into_owned(),
        ticket: None,
    })? {
        Payload::Prepared(ticket) => Ok(ticket),
        _ => Err("Server fixture did not prepare ownership".into()),
    }
}
#[cfg(all(test, target_os = "linux"))]
pub(super) fn fixture_server_execute(repo: &Path, ticket: Ticket) -> Result<String, String> {
    let transaction = ticket.transaction.clone();
    let result = process(Request {
        protocol: PROTOCOL.into(),
        repo: repo.to_string_lossy().into_owned(),
        ticket: Some(ticket),
    })?;
    completed(result, &transaction, "owned fixture recovery")
}
