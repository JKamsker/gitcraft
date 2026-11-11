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

/// The same submission boundary is used by live observation and inert scheduling tests.
fn submit_fetch(
    pool: &tasks::Pool,
    path: &Path,
    cancel: tasks::Cancellation,
    job: Box<dyn FnOnce() + Send>,
) -> Result<(), &'static str> {
    pool.submit(
        cancel,
        tasks::Priority::Background,
        Some(crate::repo::mutation_key(path)),
        job,
    )
}

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
    paths_by(open, recent, crate::repo::fetch_key)
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
mod observation;
use observation::*;

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
    status_language: u64,
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
        // Status is transient presentation, not observation/notification ownership. Refresh
        // the current count/pause caption after a language change without restarting work.
        let language = crate::i18n::generation();
        if self.status_language != language {
            self.status_language = language;
            self.status.clear();
        }
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
            .map(|path| crate::repo::fetch_key(path))
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
                Err(crate::i18n::text("interaction-watch-label-5").into()),
                now,
                Duration::from_secs(u64::from(minutes.max(1)) * 60),
            );
            self.status = crate::i18n::text("interaction-watch-label-6").into();
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
                            Ok(_) => crate::i18n::message(
                                "interaction-watch-count",
                                &[("count", self.model.subscriptions.len().into())],
                            ),
                            Err(why) => crate::i18n::message(
                                "interaction-watch-retry",
                                &[
                                    ("path", job.path.display().to_string().into()),
                                    ("detail", why.as_str().into()),
                                ],
                            ),
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
                            self.status = crate::i18n::message(
                                "interaction-watch-repo-notice",
                                &[
                                    (
                                        "repo",
                                        job.path
                                            .file_name()
                                            .unwrap_or(job.path.as_os_str())
                                            .to_string_lossy()
                                            .as_ref()
                                            .into(),
                                    ),
                                    ("text", notice.text.as_str().into()),
                                ],
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
                            Err(crate::i18n::text("interaction-watch-label-7").into()),
                            now,
                            Duration::from_secs(60),
                        );
                    }
                }
            }
        }
        if !active {
            self.status = if enabled {
                crate::i18n::text("interaction-watch-label-8")
            } else {
                crate::i18n::text("interaction-watch-label-9")
            }
            .into();
            return;
        }
        if self.status.is_empty() {
            self.status = crate::i18n::message(
                "interaction-watch-count",
                &[("count", self.model.subscriptions.len().into())],
            );
        }
        let allowed = crate::network::background_may_run();
        if !allowed {
            self.status = crate::i18n::text("interaction-watch-label-1").into();
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
        let body = crate::app::resident_notification_body(&crate::i18n::message(
            "interaction-watch-repo-notice",
            &[
                (
                    "repo",
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                        .as_ref()
                        .into(),
                ),
                ("text", text.into()),
            ],
        ));
        #[cfg(not(test))]
        {
            let job = notification_job(cancel.clone(), body, |body| {
                let _ = notify_rust::Notification::new()
                    .appname("gitcraft")
                    .summary(crate::i18n::text("interaction-watch-label-11"))
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
            submit_fetch(
                pool,
                &path,
                cancel.clone(),
                Box::new(move || {
                    let result = context.run(|| {
                        crate::repo::discover_fetch_key(&work_path);
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
        self.status = crate::i18n::message(
            "interaction-watch-checking",
            &[("path", path.display().to_string().into())],
        );
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
