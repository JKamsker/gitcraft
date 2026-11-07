//! Durable exact-OID pushes. The queue and protected source objects move in one ref
//! transaction. An in-flight/ambiguous write never becomes eligible for automatic retry.
use super::{
    guard::{Runner, text},
    remote_config::validate_url,
    routes,
};
use crate::{git, push_preflight as preflight, settings::profiles::UserProfile};
use serde::{Deserialize, Serialize};
use std::path::Path;
const REF: &str = "refs/gitcraft/push-outbox/state";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Status {
    Ready,
    Sending,
    Sent,
    Paused(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub id: String,
    pub commit: String,
    pub source: String,
    pub reviewed_tip: String,
    pub remote: String,
    pub destination: String,
    pub endpoints: Vec<String>,
    pub autosend: bool,
    pub status: Status,
    pub receipt: String,
    identity: String,
    routing: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Queue {
    pub entries: Vec<Entry>,
}
#[derive(Clone)]
pub(crate) struct View {
    pub version: String,
    pub queue: Queue,
    pub branch: String,
}
#[derive(Clone)]
pub(crate) struct Prepared {
    pub view: View,
    pub id: String,
    pub plan: preflight::Plan,
    pub review: bool,
}
fn digest(s: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn routing(repo: &Path, source: &str) -> Result<(String, String, String, Vec<String>), String> {
    let browser = routes::read(repo, source)?;
    let b = browser
        .branches
        .iter()
        .find(|b| b.name == source)
        .ok_or("Queued source branch was removed")?;
    let urls = text(
        repo,
        &["remote", "get-url", "--push", "--all", "--", &b.push.remote],
    )?;
    let urls: Vec<_> = urls.lines().map(str::to_owned).collect();
    if urls.is_empty() {
        return Err("Push route has no endpoint".into());
    }
    for url in &urls {
        validate_url(url)?;
    }
    let stamp = preflight::outbox_config_stamp(repo)?;
    Ok((
        b.push.remote.clone(),
        format!("refs/heads/{}", b.push.branch),
        digest(&stamp),
        urls.iter().map(|s| digest(s)).collect(),
    ))
}
pub(crate) fn read(repo: &Path) -> Result<View, String> {
    let version = git::run_ok_codes(
        Some(repo),
        &["rev-parse", "--verify", "--quiet", REF],
        &[0, 1],
    )?
    .trim()
    .to_owned();
    let queue = if version.is_empty() {
        Queue::default()
    } else {
        serde_json::from_slice(&super::read(
            repo,
            &["cat-file", "blob", &version],
            vec![],
            2 << 20,
        )?)
        .map_err(|e| format!("Invalid outbox: {e}"))?
    };
    let branch = git::run_ok_codes(
        Some(repo),
        &["symbolic-ref", "--short", "-q", "HEAD"],
        &[0, 1],
    )?
    .trim()
    .to_owned();
    Ok(View {
        version,
        queue,
        branch,
    })
}
fn save(repo: &Path, view: &View, pin: Option<&Entry>) -> Result<(), String> {
    let bytes = serde_json::to_vec(&view.queue).map_err(|e| e.to_string())?;
    if bytes.len() > 2 << 20 {
        return Err("Outbox exceeds 2 MiB; remove old receipts first".into());
    }
    let oid = super::read(repo, &["hash-object", "-w", "--stdin"], bytes, 1024)?;
    let oid = std::str::from_utf8(&oid).map_err(|e| e.to_string())?.trim();
    let mut txn = format!(
        "start\nupdate {REF} {oid} {}\n",
        if view.version.is_empty() {
            "0".repeat(oid.len())
        } else {
            view.version.clone()
        }
    );
    if let Some(e) = pin {
        txn.push_str(&format!(
            "create refs/gitcraft/push-outbox/pins/{} {}\n",
            e.id, e.commit
        ));
    }
    txn.push_str("prepare\ncommit\n");
    super::read(repo, &["update-ref", "--stdin"], txn.into_bytes(), 65536)?;
    Ok(())
}
pub(crate) fn enqueue(repo: &Path, expected: &View, auto: bool) -> Result<String, String> {
    if read(repo)?.version != expected.version {
        return Err("Outbox changed; reopen it".into());
    }
    if expected.queue.entries.len() >= 128 {
        return Err("Outbox has 128 entries; remove old receipts first".into());
    }
    let source = &expected.branch;
    if source.is_empty() {
        return Err("Check out a source branch before queueing".into());
    }
    let commit = text(
        repo,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{source}^{{commit}}"),
        ],
    )?
    .trim()
    .to_owned();
    let (remote, destination, stamp, endpoints) = routing(repo, source)?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = format!(
        "{}-{}-{}",
        git::now(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let entry = Entry {
        id,
        reviewed_tip: commit.clone(),
        commit,
        source: source.clone(),
        remote,
        destination,
        endpoints,
        autosend: auto,
        status: Status::Ready,
        receipt: String::new(),
        identity: text(
            repo,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?,
        routing: stamp,
    };
    check_source(repo, &entry, false)?;
    let mut view = expected.clone();
    view.queue.entries.push(entry.clone());
    save(repo, &view, Some(&entry))?;
    Ok(format!(
        "Queued {} at {}. Later commits are excluded.",
        entry.source, entry.commit
    ))
}
fn check(repo: &Path, e: &Entry) -> Result<(), String> {
    check_source(repo, e, true)
}
fn check_source(repo: &Path, e: &Entry, pinned: bool) -> Result<(), String> {
    if text(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )? != e.identity
    {
        return Err("Repository identity changed".into());
    }
    let (remote, destination, stamp, endpoints) = routing(repo, &e.source)?;
    if (remote, destination, stamp, endpoints)
        != (
            e.remote.clone(),
            e.destination.clone(),
            e.routing.clone(),
            e.endpoints.clone(),
        )
    {
        return Err(
            "Destination, identity policy or routing changed; review and queue a new entry".into(),
        );
    }
    let source = text(
        repo,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}^{{commit}}", e.source),
        ],
    )?;
    git::run(
        Some(repo),
        &[
            "merge-base",
            "--is-ancestor",
            &e.reviewed_tip,
            source.trim(),
        ],
    )
    .map_err(|_| "Source history was rewritten; review and queue a new entry".to_owned())?;
    if pinned
        && text(
            repo,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/gitcraft/push-outbox/pins/{}", e.id),
            ],
        )?
        .trim()
            != e.commit
    {
        return Err("Protected outbox commit changed".into());
    }
    Ok(())
}
fn entry<'a>(view: &'a View, id: &str) -> Result<&'a Entry, String> {
    view.queue
        .entries
        .iter()
        .find(|e| e.id == id)
        .ok_or("Outbox entry disappeared".into())
}
pub(crate) fn prepare(repo: &Path, id: &str, profile: UserProfile) -> Result<Prepared, String> {
    prepare_with(repo, id, profile, false)
}
pub(crate) fn review_prepare(
    repo: &Path,
    id: &str,
    profile: UserProfile,
) -> Result<Prepared, String> {
    prepare_with(repo, id, profile, true)
}
fn prepare_with(
    repo: &Path,
    id: &str,
    profile: UserProfile,
    review: bool,
) -> Result<Prepared, String> {
    let mut view = read(repo)?;
    if review {
        let e = view
            .queue
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or("Entry disappeared")?;
        if !matches!(e.status, Status::Paused(_) | Status::Sending) {
            return Err("Only paused or uncertain entries need renewed approval".into());
        }
        let (remote, destination, stamp, endpoints) = routing(repo, &e.source)?;
        e.reviewed_tip = text(
            repo,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{}^{{commit}}", e.source),
            ],
        )?
        .trim()
        .into();
        e.remote = remote;
        e.destination = destination;
        e.routing = stamp;
        e.endpoints = endpoints;
        e.status = Status::Ready;
        e.autosend = false;
    }
    let e = entry(&view, id)?;
    if e.status != Status::Ready {
        return Err("Entry is paused, in-flight or sent. Inspect its receipt; queue a new push explicitly if appropriate.".into());
    }
    if let Err(error) = check(repo, e) {
        pause(repo, id, &error)?;
        return Err(error);
    }
    let ops = vec![vec![
        "push".into(),
        "--porcelain".into(),
        "--no-follow-tags".into(),
        "--".into(),
        e.remote.clone(),
        format!("{}:{}", e.commit, e.destination),
    ]];
    let plan = match preflight::scan(repo, ops, profile, &|| {
        !crate::tasks::cancellation().is_cancelled()
    }) {
        Ok(p) => p,
        Err(error) => {
            pause(repo, id, &error)?;
            return Err(error);
        }
    };
    if plan.head != e.commit
        || plan.remote != e.remote
        || plan.destination != e.destination
        || plan.force
        || plan.deleting
    {
        return Err("Preflight differs from the exact queued plain push".into());
    }
    Ok(Prepared {
        view,
        id: id.into(),
        plan,
        review,
    })
}
fn pause(repo: &Path, id: &str, why: &str) -> Result<(), String> {
    let mut view = read(repo)?;
    let e = view
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or("Entry disappeared")?;
    e.status = Status::Paused(git::redact(why, None));
    save(repo, &view, None)
}
pub(crate) fn send(runner: &Runner<'_>, p: &Prepared, automatic: bool) -> Result<String, String> {
    if crate::network::refused().is_some() {
        return Err("Outbox remains queued while offline".into());
    }
    let mut view = read(runner.repo)?;
    if view.version != p.view.version {
        return Err("Outbox changed; review again".into());
    }
    let actual = entry(&view, &p.id)?;
    if p.review && (!matches!(actual.status, Status::Paused(_) | Status::Sending) || automatic) {
        return Err("Renewed approval no longer applies".into());
    }
    let e = if p.review {
        entry(&p.view, &p.id)?.clone()
    } else {
        actual.clone()
    };
    if e.status != Status::Ready {
        return Err("This entry is no longer ready".into());
    }
    if automatic && (!e.autosend || !p.plan.findings.is_empty()) {
        pause(
            runner.repo,
            &e.id,
            "Automatic plain send requires a clean preflight; review its findings and queue explicitly",
        )?;
        return Err("Automatic send paused for preflight review".into());
    }
    if let Err(error) = check(runner.repo, &e) {
        pause(runner.repo, &e.id, &error)?;
        return Err(error);
    }
    preflight::check(runner.repo, &p.plan)?;
    // Durable before network I/O. A crash/cancel/lost acknowledgement stays Sending and is
    // explicitly ineligible for retry, including after application restart.
    let slot = view
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == p.id)
        .unwrap();
    *slot = e.clone();
    slot.status = Status::Sending;
    save(runner.repo, &view, None)?;
    let result = preflight::execute_controlled(
        runner.repo,
        &p.plan,
        runner.credentials,
        Some(runner.control),
    );
    let mut receipt = read(runner.repo)?;
    let e = receipt
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == p.id)
        .ok_or("Push ended but entry changed; inspect the remote before another send")?;
    match &result {
        Ok(output) => {
            e.status = Status::Sent;
            e.receipt = git::redact(output, None);
        }
        Err(error) => {
            e.status=Status::Paused("Push rejected, cancelled or outcome uncertain. Inspect the destination before queueing again.".into());
            e.receipt = git::redact(error, None);
        }
    }
    let sent_commit = e.commit.clone();
    save(runner.repo,&receipt,None).map_err(|_|"Push attempted, but receipt could not be saved. Inspect the destination; do not retry automatically".to_owned())?;
    result.map(|_| format!("Published exact queued commit {sent_commit}"))
}
pub(crate) fn remove(repo: &Path, expected: &View, id: &str) -> Result<String, String> {
    if read(repo)?.version != expected.version {
        return Err("Outbox changed; reopen it".into());
    }
    let e = entry(expected, id)?;
    if e.status == Status::Sending {
        return Err("Outcome uncertain: inspect the remote and retain this receipt; no retry or deletion is automatic".into());
    }
    let mut view = expected.clone();
    view.queue.entries.retain(|e| e.id != id);
    save(repo, &view, None)?;
    Ok("Removed queue entry; protected source commit is retained for recovery".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(
        name: &str,
    ) -> Result<(git::fixture::Repo, git::fixture::Repo, std::path::PathBuf), String> {
        let r = git::fixture::Repo::new(name)?;
        r.commit("base", &[])?;
        let host = git::fixture::Repo::new(&format!("{name}-host"))?;
        let bare = host.path().join("host.git");
        std::fs::create_dir(&bare).map_err(|e| e.to_string())?;
        git::run(Some(&bare), &["init", "--bare"])?;
        r.git(&["remote", "add", "origin", &bare.to_string_lossy()])?;
        Ok((r, host, bare))
    }
    #[test]
    fn durable_exact_oid_excludes_later_commits_and_refuses_duplicate_send() -> Result<(), String> {
        let (r, _host, bare) = fixture("outbox-exact")?;
        let queued = r.rev("HEAD")?;
        enqueue(r.path(), &read(r.path())?, false)?;
        r.commit("later", &[])?;
        let view = read(r.path())?;
        let e = &view.queue.entries[0];
        assert_eq!(e.commit, queued);
        let p = prepare(r.path(), &e.id, UserProfile::default())?;
        let control = git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        send(&runner, &p, false)?;
        let remote = git::run(Some(&bare), &["rev-parse", &p.plan.destination])?;
        assert_eq!(remote.trim(), queued);
        assert!(send(&runner, &p, false).is_err());
        assert_eq!(read(r.path())?.queue.entries[0].status, Status::Sent);
        Ok(())
    }
    #[test]
    fn rewrites_and_changed_destinations_pause_without_any_push() -> Result<(), String> {
        let (r, _host, _bare) = fixture("outbox-pauses")?;
        let base = r.rev("HEAD")?;
        r.commit("queued", &[])?;
        enqueue(r.path(), &read(r.path())?, true)?;
        let id = read(r.path())?.queue.entries[0].id.clone();
        r.git(&["reset", "--hard", &base])?;
        assert!(prepare(r.path(), &id, UserProfile::default()).is_err());
        assert!(matches!(
            read(r.path())?.queue.entries[0].status,
            Status::Paused(_)
        ));
        enqueue(r.path(), &read(r.path())?, true)?;
        let id = read(r.path())?.queue.entries[1].id.clone();
        r.git(&[
            "remote",
            "set-url",
            "--push",
            "origin",
            "https://example.invalid/new.git",
        ])?;
        assert!(prepare(r.path(), &id, UserProfile::default()).is_err());
        assert!(matches!(
            read(r.path())?.queue.entries[1].status,
            Status::Paused(_)
        ));
        Ok(())
    }
    #[test]
    fn stale_queue_writers_and_uncertain_sends_are_not_retried() -> Result<(), String> {
        let (r, _host, _bare) = fixture("outbox-cas")?;
        let stale = read(r.path())?;
        enqueue(r.path(), &stale, true)?;
        assert!(enqueue(r.path(), &stale, false).is_err());
        let mut view = read(r.path())?;
        let id = view.queue.entries[0].id.clone();
        view.queue.entries[0].status = Status::Sending;
        save(r.path(), &view, None)?;
        assert!(prepare(r.path(), &id, UserProfile::default()).is_err());
        assert!(remove(r.path(), &read(r.path())?, &id).is_err());
        Ok(())
    }
}
