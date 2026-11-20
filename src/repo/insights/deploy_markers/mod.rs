//! Deployment markers on the commit graph (#173 item 9): a pill on each commit an environment
//! was last deployed with, as the forge's deployment records say (GitHub, GitLab), read in the
//! background with the CI statuses' triggers and kept per tab. A pill selects its commit; its
//! menu compares two environments and asks whether the selected commit reached one.

mod model;
mod pills;
#[cfg(test)]
mod tests;
mod tooltip;
#[cfg(test)]
mod ui_tests;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::ci::Project;
use crate::forge::Forge;
use crate::repo::{Action, RepoTab};
use model::{Marker, Read};

/// Markers older than this are read again with the next reload. Longer than the CI statuses'
/// two minutes: one read asks the forge up to some 35 times.
const STALE: Duration = Duration::from_secs(300);
/// How soon they are read again while a deployment is queued or under way.
const IN_FLIGHT_POLL: Duration = Duration::from_secs(60);
/// A read that hasn't answered in so long is taken for lost (its tab was suspended, its
/// worker ended): the next trigger reads again.
const LOST: Duration = Duration::from_secs(120);
/// Markers from a read older than this (seconds) are drawn as stale.
const SHOWN_FRESH: i64 = 30 * 60;

/// What the markers were read for: once any of it changes, they go. The account (connection)
/// is compared by value: switching away and back again leaves them as valid as they were.
#[derive(PartialEq, Eq)]
struct Key {
    project: Option<Project>,
    epoch: u64,
    path: std::path::PathBuf,
    connection: crate::connections::Context,
}

impl Key {
    /// Whether the tab is still in the repository and account this was read for.
    fn holds(&self, tab: &RepoTab) -> bool {
        self.path == tab.path && self.connection == crate::connections::current()
    }
}

/// The tab's markers.
#[derive(Default)]
pub(in crate::repo) struct State {
    /// The graph shows them (the setting), as of the last frame.
    visible: bool,
    markers: Vec<Marker>,
    partial: Vec<String>,
    /// Why the last read failed: what an earlier one found is still shown, as stale.
    error: Option<String>,
    forge: Option<Forge>,
    /// When the markers shown were read (seconds since the epoch).
    observed: Option<i64>,
    /// When a read was last started (or found impossible).
    loaded: Option<Instant>,
    /// The read on its way: its number and when it started.
    pending: Option<(u64, Instant)>,
    requests: u64,
    /// When to read again: a deployment is queued or under way.
    next: Option<Instant>,
    key: Option<Key>,
}

impl State {
    fn retire(&mut self) {
        let (visible, requests) = (self.visible, self.requests);
        *self = Self {
            visible,
            requests,
            ..Self::default()
        };
    }

    /// The markers shown came from an old read, or the latest failed.
    fn read_stale(&self, now: i64) -> bool {
        self.error.is_some() || self.observed.is_none_or(|at| now - at > SHOWN_FRESH)
    }

    fn reading(&self, now: Instant) -> bool {
        self.pending
            .is_some_and(|(_, at)| now.duration_since(at) < LOST)
    }
}

/// What the markers ask for (in a module of its own, so the action stays inside the domain).
pub(super) mod act {
    #[derive(Clone, Debug)]
    pub(crate) enum Act {
        /// Show or hide them (the setting).
        Toggle,
        /// Whether commit `commit` is in what `environment` runs, by the repository's history.
        Reached { commit: String, environment: String },
    }
}
pub(super) use act::Act;

impl RepoTab {
    fn deploy_markers_key(&self, ctx: &egui::Context) -> Key {
        Key {
            project: self.ci_project(),
            epoch: super::super::forge_selection::lifetime(ctx, self.repo_key()).1,
            path: self.path.clone(),
            connection: crate::connections::current(),
        }
    }

    /// Each frame the graph is drawn: whether the markers show (the setting). They are read
    /// the first time they show for a repository, forge and account, and again when due.
    pub(in crate::repo) fn deploy_markers_frame(&mut self, visible: bool, ctx: &egui::Context) {
        self.deploy_markers.visible = visible;
        if !visible {
            return;
        }
        let key = self.deploy_markers_key(ctx);
        if self.deploy_markers.key.as_ref() != Some(&key) {
            self.deploy_markers.retire();
        }
        let now = Instant::now();
        let state = &self.deploy_markers;
        let due = state.loaded.is_none() || state.next.is_some_and(|next| now >= next);
        if due {
            self.load_deploy_markers(key, ctx);
        } else if let Some(next) = state.next {
            ctx.request_repaint_after(next - now);
        }
    }

    /// With the CI statuses (a reload, a forge or account change, back online): the markers
    /// are read again when they are old.
    pub(in crate::repo) fn refresh_deploy_markers(&mut self, ctx: &egui::Context) {
        let state = &self.deploy_markers;
        if state.visible && state.loaded.is_none_or(|at| at.elapsed() > STALE) {
            let key = self.deploy_markers_key(ctx);
            self.load_deploy_markers(key, ctx);
        }
    }

    fn load_deploy_markers(&mut self, key: Key, ctx: &egui::Context) {
        let now = Instant::now();
        // Offline or metered: read once that is over (the next trigger, or the next frame).
        if self.deploy_markers.reading(now) || !crate::network::background_may_run() {
            return;
        }
        if self.deploy_markers.key.as_ref() != Some(&key) {
            self.deploy_markers.retire();
        }
        let available = crate::insights::deploy::availability(self.forge());
        let state = &mut self.deploy_markers;
        state.loaded = Some(now);
        state.next = None;
        let project = key.project.clone();
        let epoch = key.epoch;
        state.key = Some(key);
        let Some(project) = project.filter(|_| available.is_ok()) else {
            // Nothing to read here: no markers, and nothing asked until the context changes.
            state.markers.clear();
            state.error = available.err();
            return;
        };
        state.requests += 1;
        let request = state.requests;
        state.pending = Some((request, now));
        state.forge = Some(project.forge);
        let (clock, _) = super::super::forge_selection::lifetime(ctx, self.repo_key());
        let path = self.path.clone();
        self.forge_then(
            ctx,
            move || crate::git::offline(|| model::read(&path, &project)),
            move |tab, result, _| {
                if clock.load(Ordering::SeqCst) == epoch {
                    tab.deploy_markers_arrived(request, result);
                }
            },
        );
    }

    fn deploy_markers_arrived(&mut self, request: u64, result: Result<Read, String>) {
        let current = self
            .deploy_markers
            .key
            .as_ref()
            .is_some_and(|key| key.holds(self));
        let state = &mut self.deploy_markers;
        if !current || state.pending.map(|(r, _)| r) != Some(request) {
            return;
        }
        state.pending = None;
        match result {
            Ok(read) => {
                state.next = read.in_flight.then(|| Instant::now() + IN_FLIGHT_POLL);
                state.markers = read.markers;
                state.partial = read.partial;
                state.error = None;
                state.observed = Some(crate::git::now());
            }
            // What was read before is kept, and drawn as stale.
            Err(error) => state.error = Some(error),
        }
    }

    pub(super) fn deploy_markers_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Toggle => self.toggle_deploy_markers(ctx),
            Act::Reached {
                commit,
                environment,
            } => self.deploy_reached(commit, environment, ctx),
        }
    }

    /// Asks Git, in the background, whether `commit` is in what `environment` last deployed.
    fn deploy_reached(&mut self, commit: String, environment: String, ctx: &egui::Context) {
        let Some(deployed) = self
            .deploy_markers
            .markers
            .iter()
            .find(|m| m.environment == environment)
            .map(|m| m.sha().to_owned())
        else {
            return;
        };
        let path = self.path.clone();
        let asked = deployed.clone();
        self.then(
            ctx,
            move || Answer {
                contained: crate::git::offline(|| {
                    crate::insights::deploy::contains(&path, &commit, &asked)
                }),
                commit,
            },
            move |tab, answer: Answer, _| {
                let notice = tooltip::reached_notice(&answer, &environment, &deployed);
                tab.status = Some(crate::repo::Notice::plain((notice, false)));
            },
        );
    }

    /// The palette's entry: show or hide the markers.
    pub(in crate::repo) fn deploy_markers_palette_item(&self) -> (String, String, Action) {
        (
            crate::i18n::text("graph-deployments-palette").to_owned(),
            crate::i18n::text("graph-deployments-palette-hint").to_owned(),
            Action::Insights(super::InsightsAct::DeployMarkers(Act::Toggle)),
        )
    }
}

/// What Git said of a commit and an environment's deployed commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Answer {
    /// Contained (`Some(true)`), not (`Some(false)`) or unknown: either isn't here.
    contained: Option<bool>,
    commit: String,
}
