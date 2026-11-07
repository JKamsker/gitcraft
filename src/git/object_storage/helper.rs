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
#[cfg(target_os = "linux")]
mod receipt;
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
    #[serde(default)]
    status: Option<Ticket>,
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
    Terminal {
        transaction: String,
        result: Result<String, String>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(any(target_os = "linux", windows))]
struct Receipt {
    protocol: String,
    ticket: Ticket,
    result: Result<String, String>,
}
#[cfg(any(target_os = "linux", windows))]
pub(super) fn receipt_bytes(
    ticket: &Ticket,
    result: &Result<String, String>,
) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(&Receipt {
        protocol: PROTOCOL.into(),
        ticket: ticket.clone(),
        result: result.clone(),
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Terminal ownership receipt exceeds 4 MiB".into());
    }
    Ok(bytes)
}
#[derive(Clone, Debug)]
pub(super) struct RemotePlan {
    pub repo: PathBuf,
    profile: connections::Context,
    ticket: Ticket,
    attempted: Arc<AtomicBool>,
}
#[derive(Clone)]
struct Pending {
    owner: remote::OwnedDispatch,
    ticket: Ticket,
}
fn pending() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, Pending>> {
    static VALUE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, Pending>>,
    > = std::sync::OnceLock::new();
    VALUE.get_or_init(Default::default)
}
fn retire(repo: &Path, transaction: &str) {
    let mut values = pending()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if values
        .get(repo)
        .is_some_and(|value| value.ticket.transaction == transaction)
    {
        values.remove(repo);
    }
}
#[derive(Debug)]
enum ExchangeError {
    NotDispatched(String),
    Unknown(String),
}
impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotDispatched(s) | Self::Unknown(s) => f.write_str(s),
        }
    }
}
fn exchange(
    target: &remote::Target,
    body: Request,
    owner: &remote::OwnedDispatch,
    active: &dyn Fn() -> bool,
) -> Result<Result<Payload, String>, ExchangeError> {
    exchange_with(target, body, owner, active, remote::exec_owned)
}
fn exchange_with(
    target: &remote::Target,
    body: Request,
    owner: &remote::OwnedDispatch,
    active: &dyn Fn() -> bool,
    send: impl FnOnce(
        &remote::Host,
        &remote::Request,
        &remote::OwnedDispatch,
        &dyn Fn() -> bool,
    ) -> Result<remote::Output, remote::OwnedFailure>,
) -> Result<Result<Payload, String>, ExchangeError> {
    if !active() {
        owner.cancel();
        return Err(ExchangeError::NotDispatched(
            "Object independence was cancelled before dispatch; no helper request was sent.".into(),
        ));
    }
    let payload =
        serde_json::to_vec(&body).map_err(|e| ExchangeError::NotDispatched(e.to_string()))?;
    if payload.len() > LIMIT {
        return Err(ExchangeError::NotDispatched(
            "Owned-helper snapshot exceeds 4 MiB; use Git on the repository machine.".into(),
        ));
    }
    // Constant shell text only. Paths and ownership metadata travel as bounded JSON stdin.
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
    owner.run(|| {
        // Keep the Context/completion scope across actual queue/connect/send/result.
        // exec_owned checks active again at its actual pre-send boundary; it never
        // invokes the legacy timeout controller or retries a possibly partial send.
        if !active() {
            owner.cancel();
            return Err(ExchangeError::NotDispatched("Object independence was cancelled before dispatch; no helper request was sent.".into()));
        }
        let out = send(&target.host, &req, owner, active).map_err(|e| match e {
            remote::OwnedFailure::NotDispatched(message) => ExchangeError::NotDispatched(message),
            remote::OwnedFailure::Unknown => ExchangeError::Unknown("The actual helper transport ended without a known outcome; no stop or retry was dispatched.".into()),
        })?;
        parse_protocol(out).map_err(ExchangeError::Unknown)
    })
}
fn parse_protocol(out: remote::Output) -> Result<Result<Payload, String>, String> {
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
    Ok(reply.result)
}
#[cfg(test)]
fn parse_reply(out: remote::Output) -> Result<Payload, String> {
    parse_protocol(out)?
}
fn request_target(
    target: &remote::Target,
    ticket: Option<Ticket>,
    active: &dyn Fn() -> bool,
) -> Result<Payload, String> {
    let owner = remote::OwnedDispatch::default();
    exchange(
        target,
        Request {
            protocol: PROTOCOL.into(),
            repo: target.path.clone(),
            ticket,
            status: None,
        },
        &owner,
        active,
    )
    .map_err(|e| e.to_string())?
}
fn resolve_pending(repo: &Path, active: &dyn Fn() -> bool) -> Result<(), String> {
    let held = pending()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(repo)
        .cloned();
    let Some(held) = held else {
        return Ok(());
    };
    let target = remote::parse(repo).ok_or("Pending helper target is unavailable")?;
    let result = held.owner.run(|| {
        let reader = remote::OwnedDispatch::default();
        exchange(
            &target,
            Request {
                protocol: PROTOCOL.into(),
                repo: target.path.clone(),
                ticket: None,
                status: Some(held.ticket.clone()),
            },
            &reader,
            active,
        )
    });
    if let Ok(Ok(Payload::Terminal {
        transaction,
        result,
    })) = result
        && transaction == held.ticket.transaction
        && !held.owner.pending()
    {
        retire(repo, &transaction);
        return Err(format!(
            "The previous independence attempt has a verified terminal receipt: {result:?}. Inspect this repository again before another action."
        ));
    }
    Err(format!(
        "A previous independence attempt still has no verified terminal outcome. No replacement was dispatched. Reinspect after the helper finishes. Recovery: {}/info/alternates.gitcraft-backup-{}; private stage: {}/gitcraft-independent-stage-{}. A missing receipt does not prove it stopped. If no receipt can be recovered, finish deliberate recovery on that machine, ensure the helper has ended, then reopen GitCraft.",
        held.ticket.primary, held.ticket.transaction, held.ticket.primary, held.ticket.transaction
    ))
}
pub(super) fn prepare(repo: &Path, active: &dyn Fn() -> bool) -> Result<RemotePlan, String> {
    resolve_pending(repo, active)?;
    let target = remote::parse(repo).ok_or("Owned helper requires an SSH/WSL repository")?;
    let Payload::Prepared(ticket) = request_target(&target, None, active)? else {
        return Err("The owned helper did not prepare an identity snapshot".into());
    };
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
    plan.attempted.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire)
        .map_err(|_| "This prepared independence attempt was already used. Inspect borrowing and recovery evidence again before another deliberate action.".to_owned())?;
    Ok(())
}
pub(super) fn execute(plan: &RemotePlan, active: &dyn Fn() -> bool) -> Result<String, String> {
    claim(plan, active)?;
    let target = remote::parse(&plan.repo).ok_or("Owned helper requires an SSH/WSL repository")?;
    let owner = remote::OwnedDispatch::default();
    reserve(plan, &owner)?;
    let recovery = format!(
        "{}/info/alternates.gitcraft-backup-{}",
        plan.ticket.primary, plan.ticket.transaction
    );
    let result = exchange(
        &target,
        Request {
            protocol: PROTOCOL.into(),
            repo: target.path.clone(),
            ticket: Some(plan.ticket.clone()),
            status: None,
        },
        &owner,
        active,
    );
    finish_attempt(plan, result, &recovery)
}
fn reserve(plan: &RemotePlan, owner: &remote::OwnedDispatch) -> Result<(), String> {
    {
        let mut values = pending()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if values.contains_key(&plan.repo) || values.len() >= 64 {
            return Err("Resolve the previous unknown helper outcome before another independence action. Inspect borrowing and retained recovery evidence.".into());
        }
        values.insert(
            plan.repo.clone(),
            Pending {
                owner: owner.clone(),
                ticket: plan.ticket.clone(),
            },
        );
    }
    Ok(())
}
fn finish_attempt(
    plan: &RemotePlan,
    result: Result<Result<Payload, String>, ExchangeError>,
    recovery: &str,
) -> Result<String, String> {
    match result {
        Err(ExchangeError::NotDispatched(message)) => {
            retire(&plan.repo, &plan.ticket.transaction);
            Err(format!("The helper mutation was not dispatched: {message}"))
        }
        Ok(Err(message)) => {
            retire(&plan.repo, &plan.ticket.transaction);
            Err(format!(
                "The helper finished with an error: {message}. Inspect retained recovery: {recovery}"
            ))
        }
        Ok(Ok(payload)) => match completed(payload, &plan.ticket.transaction, &recovery) {
            Ok(message) => {
                retire(&plan.repo, &plan.ticket.transaction);
                Ok(message)
            }
            Err(error) => Err(format!(
                "{error} The captured operation barrier remains retained; inspect borrowing again to query its terminal receipt."
            )),
        },
        Err(error @ ExchangeError::Unknown(_)) => Err(format!(
            "No exact completion proof was received ({detail}); no automatic retry or cleanup was performed. The captured connection and operation barrier remain retained. Inspect borrowing again to query its terminal receipt, or recover deliberately on that machine. Backup (if created): {recovery}",
            detail = error.to_string()
        )),
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

#[cfg(test)]
mod protocol_tests {
    use super::*;
    fn target() -> remote::Target {
        remote::Target {
            host: remote::Host::Ssh("recording-only.invalid".into()),
            path: "/recording-only/repo ' $(touch ESCAPED)".into(),
        }
    }
    fn body(target: &remote::Target) -> Request {
        Request {
            protocol: PROTOCOL.into(),
            repo: target.path.clone(),
            ticket: None,
            status: None,
        }
    }
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
    fn recording_owned_adapter_checks_cancel_before_send_and_holds_actual_completion_scope()
    -> Result<(), String> {
        connections::Context::default().run(|| {
            use std::sync::atomic::AtomicUsize;
            let target = target();
            let owner = remote::OwnedDispatch::default();
            let calls = AtomicUsize::new(0);
            let cancelled = exchange_with(&target,body(&target),&owner,&|| false,|_,_,_,_| {
                calls.fetch_add(1,Ordering::Relaxed); Err(remote::OwnedFailure::Unknown)
            });
            assert!(matches!(cancelled,Err(ExchangeError::NotDispatched(_))));
            assert_eq!(calls.load(Ordering::Relaxed),0);
            assert!(!owner.pending());
            let owner = remote::OwnedDispatch::default();
            let good = serde_json::json!({"protocol":PROTOCOL,"result":{"Ok":{"kind":"Completed","data":{"transaction":"abc123","message":"verified"}}}});
            let result = exchange_with(&target,body(&target),&owner,&|| true,|_,request,actual,active| {
                assert!(actual.pending());
                assert!(active());
                assert_eq!(connections::current(),connections::Context::default());
                assert_eq!(request.cwd.as_deref(),Some(target.path.as_str()));
                assert!(!request.script.contains(&target.path));
                let payload: Request = serde_json::from_slice(request.stdin.as_deref().ok_or_else(||remote::OwnedFailure::NotDispatched("no payload".into()))?)
                    .map_err(|_|remote::OwnedFailure::NotDispatched("invalid payload".into()))?;
                assert_eq!(payload.repo,target.path);
                calls.fetch_add(1,Ordering::Relaxed);
                Ok(output(good))
            }).map_err(|e|e.to_string())??;
            assert_eq!(completed(result,"abc123","backup")?,"verified");
            assert_eq!(calls.load(Ordering::Relaxed),1);
            assert!(!owner.pending());
            Ok(())
        })
    }
    #[test]
    fn recording_unknown_or_mismatched_outcome_keeps_owner_and_blocks_replacement()
    -> Result<(), String> {
        connections::Context::default().run(|| {
            let mut plan = plan();
            plan.repo = PathBuf::from("ssh://recording-only.invalid/unique-unknown-object-attempt");
            let owner = remote::OwnedDispatch::default();
            reserve(&plan, &owner)?;
            let result = finish_attempt(
                &plan,
                Err(ExchangeError::Unknown("recording lost response".into())),
                "backup",
            );
            assert!(result.is_err_and(|e| e.contains("barrier remain retained")));
            assert!(reserve(&plan, &remote::OwnedDispatch::default()).is_err());
            retire(&plan.repo, "wrong-transaction");
            assert!(
                pending()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains_key(&plan.repo)
            );
            let wrong = Payload::Completed {
                transaction: "different".into(),
                message: "not this attempt".into(),
            };
            assert!(finish_attempt(&plan, Ok(Ok(wrong)), "backup").is_err());
            assert!(
                pending()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains_key(&plan.repo)
            );
            assert_eq!(
                finish_attempt(
                    &plan,
                    Ok(Ok(Payload::Completed {
                        transaction: plan.ticket.transaction.clone(),
                        message: "verified".into()
                    })),
                    "backup"
                )?,
                "verified"
            );
            assert!(
                !pending()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains_key(&plan.repo)
            );
            Ok(())
        })
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
    if let Some(ticket) = request.status {
        if request.ticket.is_some() || ticket.repo != request.repo {
            return Err("Status request does not match the prepared repository/ticket".into());
        }
        let result = receipt::read(&ticket)?;
        return Ok(Payload::Terminal {
            transaction: ticket.transaction,
            result,
        });
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
            let message = local::make_receipted_transaction(&snapshot, &|| true, &expected)?;
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
        let capabilities = serde_json::json!({"protocol":PROTOCOL,"package_version":env!("CARGO_PKG_VERSION"),"linux_owned_identity":cfg!(target_os="linux"),"requires_filesystem_birth_time":true,"terminal_status":true,"max_protocol_bytes":LIMIT});
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
        status: None,
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
        status: None,
    })?;
    completed(result, &transaction, "owned fixture recovery")
}
#[cfg(all(test, target_os = "linux"))]
pub(super) fn fixture_server_status(
    repo: &Path,
    ticket: Ticket,
) -> Result<Result<String, String>, String> {
    let transaction = ticket.transaction.clone();
    match process(Request {
        protocol: PROTOCOL.into(),
        repo: repo.to_string_lossy().into_owned(),
        ticket: None,
        status: Some(ticket),
    })? {
        Payload::Terminal {
            transaction: actual,
            result,
        } if actual == transaction => Ok(result),
        _ => Err("No exact terminal fixture receipt".into()),
    }
}
