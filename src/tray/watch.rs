//! One bounded subscription per path, one active job in all. No RepoTab is created for a
//! recent repository. Open tabs defer their automatic fetch to this owner; explicit operations
//! still use the same task mutation key. Model decisions accept a clock and recording probe.
use crate::forge::inbox::{Checks, InboxPr};
use crate::git;
use crate::{connections, forge, tasks};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::sync::mpsc::channel;
use std::sync::{
    Mutex,
    mpsc::{Receiver, TryRecvError},
};
use std::time::{Duration, Instant};

const MAX_REPOS: usize = 12;
const JOB_LIMIT: Duration = Duration::from_secs(120);
#[cfg(not(test))]
const STEP_LIMIT: Duration = Duration::from_secs(30);
static OWNED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
pub(crate) fn owns_fetch(path: &Path) -> bool {
    OWNED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|p| p == path)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Notice {
    key: String,
    text: String,
}

#[derive(Clone, Debug)]
struct BranchChecks {
    name: String,
    sha: String,
    failed: bool,
}

#[derive(Clone, Default, Debug)]
struct Observation {
    default: Option<(String, String)>, // full remote ref and validated object id
    advanced: Option<u64>,
    prs: Option<Vec<InboxPr>>,
    ci_failed: Option<bool>,
    branch_checks: Option<BranchChecks>,
    warnings: Vec<String>,
}
#[derive(Default)]
struct Subscription {
    baseline: Option<Observation>,
    due: Option<Instant>,
    failures: u32,
}
#[derive(Default)]
struct Model {
    subscriptions: BTreeMap<PathBuf, Subscription>,
    context: Option<(u64, connections::Context)>,
}
impl Model {
    fn configure(
        &mut self,
        profile: u64,
        context: connections::Context,
        paths: Vec<PathBuf>,
    ) -> bool {
        let key = (profile, context);
        let changed = self.context.as_ref() != Some(&key);
        if changed {
            self.subscriptions.clear();
            self.context = Some(key);
        }
        self.subscriptions.retain(|path, _| paths.contains(path));
        for path in paths {
            self.subscriptions.entry(path).or_default();
        }
        changed
    }
    fn next(&self, now: Instant, busy: &HashSet<PathBuf>, allowed: bool) -> Option<PathBuf> {
        if !allowed {
            return None;
        }
        self.subscriptions
            .iter()
            .filter(|(p, s)| !busy.contains(*p) && s.due.is_none_or(|t| now >= t))
            .min_by_key(|(_, s)| s.due)
            .map(|(p, _)| p.clone())
    }
    fn finish(
        &mut self,
        path: &Path,
        result: Result<Observation, String>,
        now: Instant,
        every: Duration,
    ) -> Vec<Notice> {
        let Some(subscription) = self.subscriptions.get_mut(path) else {
            return Vec::new();
        };
        match result {
            Ok(mut observation) => {
                subscription.failures = 0;
                subscription.due = Some(now + every);
                let notifications = subscription
                    .baseline
                    .as_ref()
                    .map_or_else(Vec::new, |before| transitions(before, &observation));
                // A failed optional provider call must not fabricate removals/reappearances.
                if let Some(before) = &subscription.baseline {
                    if observation.prs.is_none() {
                        observation.prs.clone_from(&before.prs);
                    }
                    if observation.ci_failed.is_none() {
                        observation.ci_failed = before.ci_failed;
                    }
                    if observation.branch_checks.is_none() {
                        observation.branch_checks.clone_from(&before.branch_checks);
                    }
                    if observation.default.is_none() {
                        observation.default.clone_from(&before.default);
                    }
                }
                subscription.baseline = Some(observation);
                notifications
            }
            Err(_) => {
                subscription.failures = subscription.failures.saturating_add(1);
                let backoff = Duration::from_secs(30 * 2_u64.pow(subscription.failures.min(6)));
                subscription.due = Some(now + backoff.min(Duration::from_secs(3600)).max(every));
                Vec::new()
            }
        }
    }
}
fn paths(open: &[PathBuf], recent: &[PathBuf]) -> Vec<PathBuf> {
    paths_by(open, recent, crate::repo::mutation_key)
}
fn paths_by(open: &[PathBuf], recent: &[PathBuf], key: impl Fn(&Path) -> PathBuf) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut owners = HashSet::new();
    for path in open.iter().chain(recent) {
        if paths.len() < MAX_REPOS && owners.insert(key(path)) {
            paths.push(path.clone());
        }
    }
    paths
}
fn transitions(before: &Observation, after: &Observation) -> Vec<Notice> {
    let mut out = Vec::new();
    if let (Some((old_ref, old)), Some((new_ref, new)), Some(n)) =
        (&before.default, &after.default, after.advanced)
        && old_ref == new_ref
        && old != new
        && n > 0
    {
        let branch = new_ref.splitn(4, '/').nth(3).unwrap_or(new_ref);
        out.push(Notice {
            key: format!("repo:advance:{new_ref}:{new}"),
            text: format!("{branch} moved {n} commit{}", if n == 1 { "" } else { "s" }),
        });
    }
    if let (Some(old), Some(new)) = (&before.prs, &after.prs) {
        for pr in new {
            let prior = old.iter().find(|p| p.url == pr.url);
            if pr.review_requested && prior.is_none_or(|p| !p.review_requested) {
                out.push(Notice {
                    key: format!("review:{}:{}", pr.url, pr.updated),
                    text: format!("Review requested: #{} {}", pr.number, pr.title),
                });
            }
            if pr.mine
                && pr.checks == Checks::Failing
                && prior.is_none_or(|p| p.checks != Checks::Failing)
            {
                out.push(Notice {
                    key: format!("checks:{}:{}", pr.url, pr.updated),
                    text: format!("Checks failed: #{} {}", pr.number, pr.title),
                });
            }
        }
    }
    if after.ci_failed == Some(true) && before.ci_failed == Some(false) {
        out.push(Notice {
            key: format!("repo:ci:{:?}", after.default),
            text: "Default-branch checks failed".into(),
        });
    }
    if let (Some(old), Some(new)) = (&before.branch_checks, &after.branch_checks)
        && old.name == new.name
        && new.failed
        && (!old.failed || old.sha != new.sha)
    {
        out.push(Notice {
            key: format!("repo:branch:{}:{}", new.name, new.sha),
            text: format!("Checks failed on {}", new.name),
        });
    }
    out
}

struct BranchPlan {
    name: String,
    sha: String,
    projects: Vec<crate::ci::Project>,
}
fn branch_plan(
    forge: forge::Forge,
    remotes: &[git::Remote],
    refs: &git::Refs,
) -> Option<BranchPlan> {
    let name = refs.current.clone()?;
    if !valid_oid(&refs.head) {
        return None;
    }
    let main = crate::ci::Project::of(forge, remotes)?;
    let mut current = refs.clone();
    current.local.retain(|branch| branch.name == name);
    current.remotes.clear(); // ask this branch's destination(s), not all repository tips
    let plan = crate::ci::route::plan(
        &crate::ci::route::Sources {
            main: &main,
            remotes,
            refs: &current,
            prs: Vec::new(),
            recent: Vec::new(),
        },
        4,
    );
    let projects = plan
        .into_iter()
        .filter(|(_, shas)| shas.contains(&refs.head))
        .map(|(project, _)| project)
        .collect();
    Some(BranchPlan {
        name,
        sha: refs.head.clone(),
        projects,
    })
}

/// All process/provider work is confined to this adapter, invoked by a task, never by a UI
/// callback. The recording adapter exercises the exact orchestration below without runtime.
trait Probe {
    fn git(&mut self, args: &[&str]) -> Result<String, String>;
    fn hosting(&mut self) -> Option<forge::Hosting>;
    fn inbox(&mut self, hosting: &forge::Hosting) -> Result<Vec<InboxPr>, String>;
    fn ci_failed(&mut self, hosting: &forge::Hosting, sha: &str) -> Result<Option<bool>, String>;
    fn branch_checks(&mut self, _: &forge::Hosting) -> Result<Option<BranchChecks>, String> {
        Ok(None)
    }
    fn may_run(&self) -> bool;
}
fn observe(probe: &mut impl Probe, before: Option<&Observation>) -> Result<Observation, String> {
    if !probe.may_run() {
        return Err("Background network work is paused".into());
    }
    // No transient Credentials object, shell interpolation, askpass or UI authentication.
    probe.git(&["fetch", "--all", "--prune", "--quiet"])?;
    let mut snapshot = Observation::default();
    if !probe.may_run() {
        return Err("Background network work was paused".into());
    }
    let remotes = probe.git(&["remote"])?;
    let remote = remotes
        .lines()
        .find(|r| *r == "origin")
        .or_else(|| remotes.lines().next());
    if let Some(remote) = remote.filter(|r| !r.is_empty() && !r.starts_with('-')) {
        let sym = format!("refs/remotes/{remote}/HEAD");
        let reference = probe.git(&["symbolic-ref", "--quiet", &sym]).or_else(|_| {
            // Repositories created with `git init` + `remote add` need not have a local
            // remote HEAD. Ask the remote's symbolic HEAD without changing checkout/config.
            let answer = probe.git(&["ls-remote", "--symref", "--", remote, "HEAD"])?;
            let branch = answer.lines().find_map(|line| {
                line.strip_prefix("ref: refs/heads/")
                    .and_then(|line| line.strip_suffix("\tHEAD"))
            });
            branch
                .filter(|branch| !branch.is_empty())
                .map(|branch| format!("refs/remotes/{remote}/{branch}"))
                .ok_or_else(|| "Remote did not report a symbolic default branch".to_owned())
        });
        match reference {
            Ok(reference) => {
                let reference = reference.trim().to_owned();
                let prefix = format!("refs/remotes/{remote}/");
                if reference.starts_with(&prefix) && reference != sym {
                    let object = format!("{reference}^{{commit}}");
                    let sha = probe
                        .git(&["rev-parse", "--verify", "--end-of-options", &object])?
                        .trim()
                        .to_owned();
                    if valid_oid(&sha) {
                        if let Some((old_ref, old_sha)) = before.and_then(|b| b.default.as_ref())
                            && old_ref == &reference
                            && old_sha != &sha
                            && valid_oid(old_sha)
                        {
                            let range = format!("{old_sha}..{sha}");
                            snapshot.advanced = probe
                                .git(&["rev-list", "--count", &range, "--"])
                                .ok()
                                .and_then(|n| n.trim().parse().ok());
                        }
                        snapshot.default = Some((reference, sha));
                    }
                }
            }
            Err(why) => snapshot
                .warnings
                .push(format!("Default branch is unknown: {why}")),
        }
    }
    if probe.may_run()
        && let Some(hosting) = probe.hosting()
    {
        match probe.inbox(&hosting) {
            Ok(prs) => snapshot.prs = Some(prs),
            Err(why) => snapshot.warnings.push(why),
        }
        match probe.branch_checks(&hosting) {
            Ok(checks) => snapshot.branch_checks = checks,
            Err(why) => snapshot.warnings.push(why),
        }
        if let Some((_, sha)) = &snapshot.default {
            match probe.ci_failed(&hosting, sha) {
                Ok(state) => snapshot.ci_failed = state,
                Err(why) => snapshot.warnings.push(why),
            }
        }
    }
    Ok(snapshot)
}
fn valid_oid(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64) && sha.bytes().all(|c| c.is_ascii_hexdigit())
}
#[cfg(not(test))]
struct LiveProbe<'a> {
    path: &'a Path,
    cancel: tasks::Cancellation,
}
#[cfg(not(test))]
impl Probe for LiveProbe<'_> {
    fn git(&mut self, args: &[&str]) -> Result<String, String> {
        if !self.may_run() {
            return Err("Background work was stopped or paused".into());
        }
        git::run_unattended_within(self.path, args, STEP_LIMIT)
    }
    fn hosting(&mut self) -> Option<forge::Hosting> {
        let remotes = git::remotes(self.path).ok()?;
        forge::detect(self.path, &remotes)
    }
    fn inbox(&mut self, hosting: &forge::Hosting) -> Result<Vec<InboxPr>, String> {
        if !self.may_run() {
            return Err("Background work was stopped or paused".into());
        }
        hosting
            .forge
            .provider()
            .inbox(self.path, hosting)
            .map(|answer| {
                answer
                    .prs
                    .into_iter()
                    .filter(|pr| {
                        !hosting.forge.provider().inbox_per_host()
                            || pr.project.eq_ignore_ascii_case(&hosting.path)
                    })
                    .collect()
            })
    }
    fn ci_failed(&mut self, hosting: &forge::Hosting, sha: &str) -> Result<Option<bool>, String> {
        if !self.may_run() {
            return Err("Background work was stopped or paused".into());
        }
        let project = crate::ci::Project::of(hosting.forge, &git::remotes(self.path)?);
        let Some(project) = project else {
            return Ok(None);
        };
        crate::ci::statuses(self.path, &project, &[sha.into()]).map(|statuses| {
            statuses
                .get(sha)
                .map(|s| s.state == crate::ci::CheckState::Failed)
        })
    }
    fn branch_checks(&mut self, hosting: &forge::Hosting) -> Result<Option<BranchChecks>, String> {
        if !self.may_run() {
            return Err("Background work was stopped or paused".into());
        }
        let remotes = git::remotes(self.path)?;
        let refs = git::refs(self.path)?;
        let Some(plan) = branch_plan(hosting.forge, &remotes, &refs) else {
            return Ok(None);
        };
        let mut known = false;
        let mut failed = false;
        let mut errors = Vec::new();
        for project in plan.projects {
            if !self.may_run() {
                return Err("Background work was stopped or paused".into());
            }
            match crate::ci::statuses(self.path, &project, std::slice::from_ref(&plan.sha)) {
                Ok(statuses) => {
                    if let Some(status) = statuses.get(&plan.sha) {
                        known = true;
                        failed |= status.state == crate::ci::CheckState::Failed;
                    }
                }
                Err(why) => errors.push(why),
            }
        }
        if !known && !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(known.then_some(BranchChecks {
            name: plan.name,
            sha: plan.sha,
            failed,
        }))
    }
    fn may_run(&self) -> bool {
        !self.cancel.is_cancelled() && crate::network::background_may_run()
    }
}
struct Running {
    path: PathBuf,
    rx: Receiver<Result<Observation, String>>,
    cancel: tasks::Cancellation,
    started: Instant,
    stale: bool,
}
#[derive(Default)]
pub(crate) struct Watcher {
    model: Model,
    running: Option<Running>,
    pub status: String,
    notified: BTreeMap<String, Instant>,
    // The watcher retains each delivery lifetime separately from the observation job.
    // Removing one path retires only its pending notifications; profile/opt-in/preference
    // changes retire every old owner. Queued tasks hold tokens, never these Scope guards.
    deliveries: BTreeMap<PathBuf, tasks::Scope>,
    #[cfg(test)]
    queued_notifications: Vec<Box<dyn FnOnce() + Send>>,
    #[cfg(test)]
    recorded_notifications: std::sync::Arc<Mutex<Vec<String>>>,
}
impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(job) = &self.running {
            job.cancel.cancel();
        }
    }
}
pub(crate) struct Inputs<'a> {
    pub enabled: bool,
    pub profile: u64,
    pub open: &'a [PathBuf],
    pub busy: &'a HashSet<PathBuf>,
    pub recent: &'a [PathBuf],
    pub minutes: u32,
    pub notifications_enabled: bool,
}
impl Watcher {
    pub fn tick(&mut self, input: Inputs<'_>, ctx: &egui::Context) {
        let Inputs {
            enabled,
            profile,
            open,
            busy,
            recent,
            minutes,
            notifications_enabled,
        } = input;
        let now = Instant::now();
        let active = enabled && minutes > 0;
        let paths = if active {
            paths(open, recent)
        } else {
            Vec::new()
        };
        *OWNED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = paths
            .iter()
            .map(|path| crate::repo::mutation_key(path))
            .collect();
        let changed = self.model.configure(profile, connections::current(), paths);
        if changed {
            self.notified.clear();
            self.status.clear();
        }
        if changed || !active || !notifications_enabled {
            self.deliveries.clear(); // Scope::drop cancels both queued and already-starting tasks
        }
        self.deliveries
            .retain(|path, _| self.model.subscriptions.contains_key(path));
        if active && notifications_enabled {
            for path in self.model.subscriptions.keys() {
                self.deliveries.entry(path.clone()).or_default();
            }
        }
        if let Some(job) = &self.running
            && !job.stale
            && now.duration_since(job.started) > JOB_LIMIT
        {
            self.model.finish(
                &job.path,
                Err("Watcher deadline exceeded".into()),
                now,
                Duration::from_secs(u64::from(minutes.max(1)) * 60),
            );
            self.status = "Watcher deadline exceeded; waiting for cancellation before retry".into();
        }
        if let Some(job) = &mut self.running
            && (changed
                || !active
                || !self.model.subscriptions.contains_key(&job.path)
                || !crate::network::background_may_run()
                || now.duration_since(job.started) > JOB_LIMIT)
        {
            job.cancel.cancel();
            job.stale = true;
        }
        if let Some(job) = self.running.take() {
            match job.rx.try_recv() {
                Ok(result) => {
                    if !job.stale {
                        self.status = match &result {
                            Ok(s) if !s.warnings.is_empty() => s.warnings.join("; "),
                            Ok(_) => {
                                format!("Watching {} repositories", self.model.subscriptions.len())
                            }
                            Err(why) => format!("{}: {why} (will retry)", job.path.display()),
                        };
                        let notifications = self.model.finish(
                            &job.path,
                            result,
                            now,
                            Duration::from_secs(u64::from(minutes.max(1)) * 60),
                        );
                        self.notified
                            .retain(|_, sent| now.duration_since(*sent) < Duration::from_secs(300));
                        for notice in notifications {
                            self.status = format!(
                                "{}: {}",
                                job.path
                                    .file_name()
                                    .unwrap_or(job.path.as_os_str())
                                    .to_string_lossy(),
                                notice.text
                            );
                            let key = if notice.key.starts_with("repo:") {
                                format!("{}:{}", job.path.display(), notice.key)
                            } else {
                                notice.key
                            };
                            if notifications_enabled && !self.notified.contains_key(&key) {
                                if self.notified.len() >= 1024 {
                                    self.notified.clear();
                                }
                                self.notified.insert(key, now);
                                self.queue_notification(&job.path, &notice.text);
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty) => self.running = Some(job),
                Err(TryRecvError::Disconnected) => {
                    if !job.stale {
                        self.model.finish(
                            &job.path,
                            Err("Background job was cancelled".into()),
                            now,
                            Duration::from_secs(60),
                        );
                    }
                }
            }
        }
        if !active {
            self.status = if enabled {
                "Watcher paused: automatic fetch interval is zero"
            } else {
                "Watcher disabled"
            }
            .into();
            return;
        }
        if self.status.is_empty() {
            self.status = format!("Watching {} repositories", self.model.subscriptions.len());
        }
        let allowed = crate::network::background_may_run();
        if !allowed {
            self.status = "Background network work is paused".into();
        }
        // Keep old work until its cancellation/completion acknowledges; never overlap a new
        // profile's job with an old one. Queue capacity and mutation ownership remain shared.
        if self.running.is_none()
            && let Some(path) = self.model.next(now, busy, allowed)
        {
            self.start(path, ctx);
        }
    }
    fn queue_notification(&mut self, path: &Path, text: &str) {
        let Some(owner) = self.deliveries.get(path) else {
            return;
        };
        let cancel = owner.0.clone();
        let body = crate::app::resident_notification_body(&format!(
            "{}: {text}",
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
        ));
        #[cfg(not(test))]
        {
            let job = notification_job(cancel.clone(), body, |body| {
                let _ = notify_rust::Notification::new()
                    .appname("gitcraft")
                    .summary("gitcraft repository update")
                    .body(body)
                    .show();
            });
            if let Ok(pool) = tasks::pool() {
                let _ = pool.submit(cancel, tasks::Priority::Background, None, job);
            }
        }
        #[cfg(test)]
        {
            let sink = std::sync::Arc::clone(&self.recorded_notifications);
            self.queued_notifications
                .push(notification_job(cancel, body, move |body| {
                    sink.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(body.to_owned());
                }));
        }
    }
    #[cfg(not(test))]
    fn start(&mut self, path: PathBuf, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let cancel = tasks::Cancellation::default();
        let stopped = cancel.clone();
        let work_path = path.clone();
        let before = self
            .model
            .subscriptions
            .get(&path)
            .and_then(|s| s.baseline.clone());
        let context = connections::current();
        let wake = ctx.clone();
        let submitted = tasks::pool().and_then(|pool| {
            pool.submit(
                cancel.clone(),
                tasks::Priority::Background,
                Some(crate::repo::mutation_key(&path)),
                Box::new(move || {
                    let result = context.run(|| {
                        observe(
                            &mut LiveProbe {
                                path: &work_path,
                                cancel: stopped,
                            },
                            before.as_ref(),
                        )
                    });
                    let _ = tx.send(result);
                    wake.request_repaint_of(egui::ViewportId::ROOT);
                }),
            )
            .map_err(str::to_owned)
        });
        if let Err(why) = submitted {
            self.status = why;
            self.model.finish(
                &path,
                Err(self.status.clone()),
                Instant::now(),
                Duration::from_secs(60),
            );
            return;
        }
        self.status = format!("Checking {}", path.display());
        self.running = Some(Running {
            path,
            rx,
            cancel,
            started: Instant::now(),
            stale: false,
        });
    }
    #[cfg(test)]
    fn start(&mut self, _: PathBuf, _: &egui::Context) {} // pure tests never submit live jobs
}
/// The exact final delivery boundary used by the OS notifier and recording notifier. The
/// owner token is also the task's cancellation token; check again immediately before show.
fn notification_job(
    cancel: tasks::Cancellation,
    body: String,
    show: impl FnOnce(&str) + Send + 'static,
) -> Box<dyn FnOnce() + Send> {
    Box::new(move || {
        if !cancel.is_cancelled() {
            show(&body);
        }
    })
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
