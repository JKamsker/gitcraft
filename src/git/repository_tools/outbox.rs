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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Consent {
    pub profile: u64,
    pub choices: crate::connections::Choices,
    pub principal: Option<(String, String, String)>, // exact endpoint, account, username; never a password/token
}
impl Consent {
    fn capture(
        profile: &UserProfile,
        context: &crate::connections::Context,
        credentials: Option<&git::Credentials>,
    ) -> Result<Self, String> {
        if context.error.is_some() || context.choices != profile.connections {
            return Err(crate::i18n::text(
                "backend-selected-connection-profile-is-inconsistent-reopen-outbox-in-the-8ec5fa",
            )
            .into());
        }
        let principal = credentials
            .map(|credentials| {
                let scope = credentials.scope.as_ref().ok_or(
                    crate::i18n::text("backend-transient-sign-in-has-no-exact-endpoint-account-scope-sign-in-aga-254b8a"),
                )?;
                Ok::<_, String>((
                    scope.endpoint(),
                    scope.account().into(),
                    credentials.username.clone(),
                ))
            })
            .transpose()?;
        Ok(Self {
            profile: profile.id,
            choices: context.choices.clone(),
            principal,
        })
    }
    fn require(saved: Option<&Self>, selected: &Self) -> Result<(), String> {
        if saved != Some(selected) {
            return Err(crate::i18n::text(
                "backend-selected-profile-account-differs-from-queued-consent-or-its-legac-c5b0c0",
            )
            .into());
        }
        Ok(())
    }
}
fn live_consents() -> &'static std::sync::Mutex<
    std::collections::BTreeMap<(String, String), crate::connections::Context>,
> {
    static LIVE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::BTreeMap<(String, String), crate::connections::Context>>,
    > = std::sync::OnceLock::new();
    LIVE.get_or_init(Default::default)
}
fn live_check(e: &Entry, context: &crate::connections::Context) -> Result<(), String> {
    if live_consents()
        .lock()
        .map_err(|_| crate::i18n::text("backend-outbox-live-consent-unavailable-67e2b9"))?
        .get(&(e.identity.clone(), e.id.clone()))
        .is_some_and(|saved| saved != context)
    {
        return Err(crate::i18n::text(
            "backend-live-queued-connection-config-lease-changed-review-and-approve-ag-d8c29b",
        )
        .into());
    }
    Ok(())
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
    #[serde(default)]
    pub consent: Option<Consent>,
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
    pub context: crate::connections::Context,
    pub consent: Consent,
    pub ui_generation: Option<u64>,
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
        .ok_or(crate::i18n::text(
            "backend-queued-source-branch-was-removed-011ca9",
        ))?;
    let urls = text(
        repo,
        &["remote", "get-url", "--push", "--all", "--", &b.push.remote],
    )?;
    let urls: Vec<_> = urls.lines().map(str::to_owned).collect();
    if urls.is_empty() {
        return Err(crate::i18n::text("backend-push-route-has-no-endpoint-03ad2d").into());
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
        .map_err(|e| {
            crate::i18n::message(
                "backend-invalid-outbox-e-657eca",
                &[("detail", (e).to_string().into())],
            )
        })?
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
        return Err(crate::i18n::text(
            "backend-outbox-exceeds-2-mib-remove-old-receipts-first-22680d",
        )
        .into());
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
pub(crate) fn enqueue(
    repo: &Path,
    expected: &View,
    auto: bool,
    profile: &UserProfile,
    credentials: Option<&git::Credentials>,
) -> Result<String, String> {
    let context = crate::connections::current();
    let consent = Consent::capture(profile, &context, credentials)?;
    if read(repo)?.version != expected.version {
        return Err(crate::i18n::text("backend-outbox-changed-reopen-it-014048").into());
    }
    if expected.queue.entries.len() >= 128 {
        return Err(crate::i18n::text(
            "backend-outbox-has-128-entries-remove-old-receipts-first-ecf056",
        )
        .into());
    }
    let source = &expected.branch;
    if source.is_empty() {
        return Err(
            crate::i18n::text("backend-check-out-a-source-branch-before-queueing-74cb65").into(),
        );
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
        consent: Some(consent),
    };
    check_source(repo, &entry, false)?;
    let mut view = expected.clone();
    view.queue.entries.push(entry.clone());
    save(repo, &view, Some(&entry))?;
    live_consents()
        .lock()
        .map_err(|_| {
            crate::i18n::text(
                "backend-outbox-saved-live-consent-lease-could-not-be-retained-review-agai-1215d1",
            )
        })?
        .insert((entry.identity.clone(), entry.id.clone()), context);
    Ok(crate::i18n::message(
        "backend-queued-at-later-commits-are-excluded-cc2ded",
        &[
            ("source", (entry.source).to_string().into()),
            ("commit", (entry.commit).to_string().into()),
        ],
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
        return Err(crate::i18n::text("backend-repository-identity-changed-2f0e64").into());
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
        return Err(crate::i18n::text(
            "backend-destination-identity-policy-or-routing-changed-review-and-queue-a-9b7d3e",
        )
        .into());
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
    .map_err(|_| {
        crate::i18n::text(
            "backend-source-history-was-rewritten-review-and-queue-a-new-entry-22e255",
        )
        .to_owned()
    })?;
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
        return Err(crate::i18n::text("backend-protected-outbox-commit-changed-13fefd").into());
    }
    Ok(())
}
fn entry<'a>(view: &'a View, id: &str) -> Result<&'a Entry, String> {
    view.queue
        .entries
        .iter()
        .find(|e| e.id == id)
        .ok_or(crate::i18n::text("backend-outbox-entry-disappeared-e177dc").into())
}
pub(crate) fn prepare(
    repo: &Path,
    id: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
) -> Result<Prepared, String> {
    prepare_with(repo, id, profile, credentials, false)
}
pub(crate) fn review_prepare(
    repo: &Path,
    id: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
) -> Result<Prepared, String> {
    prepare_with(repo, id, profile, credentials, true)
}
fn prepare_with(
    repo: &Path,
    id: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
    review: bool,
) -> Result<Prepared, String> {
    let context = crate::connections::current();
    let consent = match Consent::capture(&profile, &context, credentials) {
        Ok(consent) => consent,
        Err(error) => {
            pause(repo, id, &error)?;
            return Err(error);
        }
    };
    let mut view = read(repo)?;
    if review {
        let e = view
            .queue
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(crate::i18n::text("backend-entry-disappeared-c60598"))?;
        if !matches!(e.status, Status::Paused(_) | Status::Sending) {
            return Err(crate::i18n::text(
                "backend-only-paused-or-uncertain-entries-need-renewed-approval-233504",
            )
            .into());
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
        e.consent = Some(consent.clone());
    }
    let e = entry(&view, id)?;
    if !review
        && let Err(error) =
            Consent::require(e.consent.as_ref(), &consent).and_then(|_| live_check(e, &context))
    {
        pause(repo, id, &error)?;
        return Err(error);
    }
    if e.status != Status::Ready {
        return Err(crate::i18n::text(
            "backend-entry-is-paused-in-flight-or-sent-inspect-its-receipt-queue-a-new-d6598c",
        )
        .into());
    }
    if let Err(error) = check(repo, e) {
        pause(repo, id, &error)?;
        return Err(error);
    }
    let ops = vec![vec![
        "push".into(),
        "--porcelain".into(),
        "--no-follow-tags".into(),
        "--recurse-submodules=check".into(),
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
        return Err(crate::i18n::text(
            "backend-preflight-differs-from-the-exact-queued-plain-push-8d6c3d",
        )
        .into());
    }
    Ok(Prepared {
        view,
        id: id.into(),
        plan,
        review,
        context,
        consent,
        ui_generation: None,
    })
}
fn pause(repo: &Path, id: &str, why: &str) -> Result<(), String> {
    let mut view = read(repo)?;
    let e = view
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(crate::i18n::text("backend-entry-disappeared-c60598"))?;
    e.status = Status::Paused(git::redact(why, None));
    save(repo, &view, None)
}
pub(crate) fn send(
    runner: &Runner<'_>,
    p: &Prepared,
    automatic: bool,
    reviewed: &preflight::ReviewedSend,
) -> Result<String, String> {
    reviewed.check_plan(&p.plan)?;
    if crate::network::refused().is_some() {
        return Err(crate::i18n::text("backend-outbox-remains-queued-while-offline-7f7902").into());
    }
    let mut view = read(runner.repo)?;
    if view.version != p.view.version {
        return Err(crate::i18n::text("backend-outbox-changed-review-again-6c3b43").into());
    }
    let actual = entry(&view, &p.id)?;
    if p.review && (!matches!(actual.status, Status::Paused(_) | Status::Sending) || automatic) {
        return Err(crate::i18n::text("backend-renewed-approval-no-longer-applies-544f7d").into());
    }
    let e = if p.review {
        entry(&p.view, &p.id)?.clone()
    } else {
        actual.clone()
    };
    if e.status != Status::Ready {
        return Err(crate::i18n::text("backend-this-entry-is-no-longer-ready-35173b").into());
    }
    let selected = match Consent::capture(
        &p.plan.profile,
        &crate::connections::current(),
        runner.credentials,
    ) {
        Ok(consent) => consent,
        Err(error) => {
            pause(runner.repo, &e.id, &error)?;
            return Err(error);
        }
    };
    if let Err(error) = Consent::require(e.consent.as_ref(), &selected)
        .and_then(|_| {
            if p.context == crate::connections::current() && p.consent == selected {
                Ok(())
            } else {
                Err(crate::i18n::text(
                    "backend-prepared-connection-changed-prepare-and-approve-again-562fe9",
                )
                .into())
            }
        })
        .and_then(|_| {
            if p.review {
                Ok(())
            } else {
                live_check(&e, &p.context)
            }
        })
    {
        pause(runner.repo, &e.id, &error)?;
        return Err(error);
    }
    if automatic && (!e.autosend || !p.plan.findings.is_empty()) {
        pause(
            runner.repo,
            &e.id,
            crate::i18n::text(
                "backend-automatic-plain-send-requires-a-clean-preflight-review-its-findin-85a272",
            ),
        )?;
        return Err(
            crate::i18n::text("backend-automatic-send-paused-for-preflight-review-e64ade").into(),
        );
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
    live_consents()
        .lock()
        .map_err(|_| {
            crate::i18n::text(
                "backend-sending-intent-saved-live-consent-unavailable-inspect-before-retr-cb5284",
            )
        })?
        .insert((e.identity.clone(), e.id.clone()), p.context.clone());
    let result = preflight::execute_bound_controlled(
        runner.repo,
        &p.plan,
        reviewed,
        runner.credentials,
        Some(runner.control),
    );
    let mut receipt = read(runner.repo)?;
    let e = receipt
        .queue
        .entries
        .iter_mut()
        .find(|e| e.id == p.id)
        .ok_or(crate::i18n::text(
            "backend-push-ended-but-entry-changed-inspect-the-remote-before-another-se-875973",
        ))?;
    match &result {
        Ok(output) => {
            e.status = Status::Sent;
            e.receipt = git::redact(output, None);
        }
        Err(error) => {
            e.status=Status::Paused(crate::i18n::text("backend-push-rejected-cancelled-or-outcome-uncertain-inspect-the-destinat-78d1f3").into());
            e.receipt = git::redact(error, None);
        }
    }
    let sent_commit = e.commit.clone();
    save(runner.repo, &receipt, None).map_err(|_| {
        crate::i18n::text(
            "backend-push-attempted-but-receipt-could-not-be-saved-inspect-the-destina-704969",
        )
        .to_owned()
    })?;
    result.map(|_| {
        crate::i18n::message(
            "backend-published-exact-queued-commit-sent-commit-04c0e8",
            &[("sent_commit", (sent_commit).to_string().into())],
        )
    })
}
pub(crate) fn remove(repo: &Path, expected: &View, id: &str) -> Result<String, String> {
    if read(repo)?.version != expected.version {
        return Err(crate::i18n::text("backend-outbox-changed-reopen-it-014048").into());
    }
    let e = entry(expected, id)?;
    if e.status == Status::Sending {
        return Err(crate::i18n::text(
            "backend-outcome-uncertain-inspect-the-remote-and-retain-this-receipt-no-r-36e9d1",
        )
        .into());
    }
    let mut view = expected.clone();
    view.queue.entries.retain(|e| e.id != id);
    save(repo, &view, None)?;
    if let Ok(mut live) = live_consents().lock() {
        live.remove(&(e.identity.clone(), e.id.clone()));
    }
    Ok(crate::i18n::text(
        "backend-removed-queue-entry-protected-source-commit-is-retained-for-recov-91464b",
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn queue(repo: &Path, view: &View, automatic: bool) -> Result<String, String> {
        enqueue(repo, view, automatic, &UserProfile::default(), None)
    }
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
        queue(r.path(), &read(r.path())?, false)?;
        r.commit("later", &[])?;
        let view = read(r.path())?;
        let e = &view.queue.entries[0];
        assert_eq!(e.commit, queued);
        let p = prepare(r.path(), &e.id, UserProfile::default(), None)?;
        let control = git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        send(
            &runner,
            &p,
            false,
            &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None),
        )?;
        let remote = git::run(Some(&bare), &["rev-parse", &p.plan.destination])?;
        assert_eq!(remote.trim(), queued);
        assert!(
            send(
                &runner,
                &p,
                false,
                &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
            )
            .is_err()
        );
        assert_eq!(read(r.path())?.queue.entries[0].status, Status::Sent);
        Ok(())
    }
    #[test]
    fn rewrites_and_changed_destinations_pause_without_any_push() -> Result<(), String> {
        let (r, _host, _bare) = fixture("outbox-pauses")?;
        let base = r.rev("HEAD")?;
        r.commit("queued", &[])?;
        queue(r.path(), &read(r.path())?, true)?;
        let id = read(r.path())?.queue.entries[0].id.clone();
        r.git(&["reset", "--hard", &base])?;
        assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
        assert!(matches!(
            read(r.path())?.queue.entries[0].status,
            Status::Paused(_)
        ));
        queue(r.path(), &read(r.path())?, true)?;
        let id = read(r.path())?.queue.entries[1].id.clone();
        r.git(&[
            "remote",
            "set-url",
            "--push",
            "origin",
            "https://example.invalid/new.git",
        ])?;
        assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
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
        queue(r.path(), &stale, true)?;
        assert!(queue(r.path(), &stale, false).is_err());
        let mut view = read(r.path())?;
        let id = view.queue.entries[0].id.clone();
        view.queue.entries[0].status = Status::Sending;
        save(r.path(), &view, None)?;
        assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
        assert!(remove(r.path(), &read(r.path())?, &id).is_err());
        Ok(())
    }
    #[test]
    fn opted_in_clean_send_and_reviewed_destination_change_keep_the_exact_pin() -> Result<(), String>
    {
        let (r, _host, bare) = fixture("outbox-renewed")?;
        let queued = r.rev("HEAD")?;
        queue(r.path(), &read(r.path())?, true)?;
        let id = read(r.path())?.queue.entries[0].id.clone();
        let p = prepare(r.path(), &id, UserProfile::default(), None)?;
        assert!(
            p.plan.findings.is_empty(),
            "fixture should have a clean plain preflight"
        );
        assert!(preflight::push_args(&p.plan)?.contains(&"--recurse-submodules=check".into()));
        let control = git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        send(
            &runner,
            &p,
            true,
            &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None),
        )?;
        assert_eq!(read(r.path())?.queue.entries[0].status, Status::Sent);
        r.commit("second", &[])?;
        let queued2 = r.rev("HEAD")?;
        queue(r.path(), &read(r.path())?, true)?;
        let id = read(r.path())?.queue.entries[1].id.clone();
        r.commit("later excluded", &[])?;
        let url = format!("{}/.", bare.display());
        r.git(&["remote", "set-url", "--push", "origin", &url])?;
        assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
        let p = review_prepare(r.path(), &id, UserProfile::default(), None)?;
        assert_eq!(p.view.queue.entries[1].commit, queued2);
        assert!(!p.view.queue.entries[1].autosend);
        assert!(
            send(
                &runner,
                &p,
                true,
                &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
            )
            .is_err()
        );
        send(
            &runner,
            &p,
            false,
            &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None),
        )?;
        assert_eq!(
            git::run(Some(&bare), &["rev-parse", &p.plan.destination])?.trim(),
            queued2
        );
        assert_ne!(queued, queued2);
        Ok(())
    }
    #[test]
    fn automatic_findings_pause_before_transport_and_remote_rejections_stay_paused()
    -> Result<(), String> {
        let (r, _host, bare) = fixture("outbox-reject")?;
        queue(r.path(), &read(r.path())?, true)?;
        let id = read(r.path())?.queue.entries[0].id.clone();
        let mut p = prepare(r.path(), &id, UserProfile::default(), None)?;
        p.plan.findings.push(preflight::Finding {
            kind: preflight::Kind::Message,
            commit: p.plan.head.clone(),
            path: String::new(),
            detail: "Review required".into(),
        });
        let control = git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        assert!(
            send(
                &runner,
                &p,
                true,
                &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
            )
            .is_err()
        );
        assert!(matches!(
            read(r.path())?.queue.entries[0].status,
            Status::Paused(_)
        ));
        assert!(git::resolve(&bare, &p.plan.destination).is_err());
        let p = review_prepare(r.path(), &id, UserProfile::default(), None)?;
        // A receiver changes after inspection. Its new ref violates the absent-ref lease.
        let tree = git::run(Some(&bare), &["mktree"])?;
        let remote = git::run(
            Some(&bare),
            &[
                "-c",
                "user.name=Receiver",
                "-c",
                "user.email=receiver@example.com",
                "-c",
                "commit.gpgSign=false",
                "commit-tree",
                tree.trim(),
                "-m",
                "Other work",
            ],
        )?;
        git::run(
            Some(&bare),
            &["update-ref", &p.plan.destination, remote.trim()],
        )?;
        assert!(
            send(
                &runner,
                &p,
                false,
                &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
            )
            .is_err()
        );
        let e = read(r.path())?.queue.entries.remove(0);
        assert!(matches!(e.status, Status::Paused(_)));
        assert!(!e.receipt.is_empty());
        assert_eq!(
            git::run(Some(&bare), &["rev-parse", &p.plan.destination])?.trim(),
            remote.trim()
        );
        assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod consent_tests {
    use super::*;
    #[test]
    fn merge_pure_outbox_consent_retains_all_added_provider_choices() {
        let profile = UserProfile::default();
        let context = crate::connections::Context {
            choices: profile.connections.clone(),
            ..Default::default()
        };
        let saved = Consent::capture(&profile, &context, None).unwrap();
        let encoded = serde_json::to_value(&saved).unwrap();
        for key in ["azure", "bitbucket", "jira", "linear", "github", "gitlab"] {
            assert!(encoded["choices"].get(key).is_some(), "missing {key}");
            let mut changed = profile.clone();
            match key {
                "azure" => changed.connections.azure.user = "other-account".into(),
                "bitbucket" => changed.connections.bitbucket.user = "other-account".into(),
                "jira" => changed.connections.jira.user = "other-account".into(),
                "linear" => changed.connections.linear.user = "other-account".into(),
                "github" => changed.connections.github.user = "other-account".into(),
                "gitlab" => changed.connections.gitlab.user = "other-account".into(),
                _ => unreachable!(),
            }
            let selected = crate::connections::Context {
                choices: changed.connections.clone(),
                ..Default::default()
            };
            let fresh = Consent::capture(&changed, &selected, None).unwrap();
            assert!(
                Consent::require(Some(&saved), &fresh).is_err(),
                "changed {key} remained authorized"
            );
        }
        assert_eq!(serde_json::from_value::<Consent>(encoded).unwrap(), saved);
    }

    #[test]
    fn pure_outbox_consent_distinguishes_profiles_selected_accounts_scopes_and_legacy_without_secrets()
     {
        let mut a = UserProfile::default();
        a.connections.github.user = "alice".into();
        let ca = crate::connections::Context {
            choices: a.connections.clone(),
            ..Default::default()
        };
        let credentials = git::Credentials {
            username: "alice".into(),
            password: "test-secret-must-never-be-persisted".into(),
            scope: Some(
                git::CredentialScope::for_url("https://github.com/team/repo.git", "alice").unwrap(),
            ),
        };
        let saved = Consent::capture(&a, &ca, Some(&credentials)).unwrap();
        let encoded = serde_json::to_string(&saved).unwrap();
        assert!(!encoded.contains(&credentials.password));
        assert!(!encoded.contains("password"));
        assert_eq!(serde_json::from_str::<Consent>(&encoded).unwrap(), saved);
        assert!(Consent::require(None, &saved).is_err());
        let mut b = a.clone();
        b.id += 1;
        let other_profile = Consent::capture(&b, &ca, Some(&credentials)).unwrap();
        assert!(Consent::require(Some(&saved), &other_profile).is_err());
        b = a.clone();
        b.connections.github.user = "bob".into();
        let cb = crate::connections::Context {
            choices: b.connections.clone(),
            ..Default::default()
        };
        assert!(
            Consent::require(
                Some(&saved),
                &Consent::capture(&b, &cb, Some(&credentials)).unwrap()
            )
            .is_err()
        );
        assert!(Consent::capture(&b, &ca, Some(&credentials)).is_err());
        let mut changed = credentials.clone();
        changed.scope = Some(
            git::CredentialScope::for_url("https://github.com/other/repo.git", "alice").unwrap(),
        );
        assert!(
            Consent::require(
                Some(&saved),
                &Consent::capture(&a, &ca, Some(&changed)).unwrap()
            )
            .is_err()
        );
        changed = credentials.clone();
        changed.username = "other".into();
        assert!(
            Consent::require(
                Some(&saved),
                &Consent::capture(&a, &ca, Some(&changed)).unwrap()
            )
            .is_err()
        );
        changed = credentials.clone();
        changed.password = "rotated-secret-not-an-identity".into();
        assert!(
            Consent::require(
                Some(&saved),
                &Consent::capture(&a, &ca, Some(&changed)).unwrap()
            )
            .is_ok()
        );
        changed.scope = None;
        assert!(Consent::capture(&a, &ca, Some(&changed)).is_err());
    }
}
