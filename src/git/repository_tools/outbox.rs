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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod consent_tests;

mod delivery;
pub(crate) use delivery::prepare;
pub(crate) use delivery::remove;
pub(crate) use delivery::review_prepare;
pub(crate) use delivery::send;
