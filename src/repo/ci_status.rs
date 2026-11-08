//! The build status of the branch tips, pull requests and latest commits, read in the
//! background, and the coloured dot that shows it.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use egui::{Color32, Id, Painter, Pos2, Rect, Response, Stroke, Ui, vec2};

use super::{Action, Msg, RepoTab, WIP_HASH};
use crate::ci::route::{self, Answers, Plan};
use crate::ci::{self, CheckState, Project, Status};
use crate::theme::Tokens;

/// So many of the latest commits get a status, besides the tips of branches.
const RECENT: usize = 30;
/// At most so many commits are asked about at a time.
const MAX_SHAS: usize = 120;
/// How soon statuses are read again while a build runs, or after one may have started.
const RUNNING_POLL: Duration = Duration::from_secs(15);
/// Statuses older than this are read again with the next reload, like the pull requests.
const STALE: Duration = Duration::from_secs(120);
pub(super) const DOT_R: f32 = 3.5;

/// The statuses of a tab's commits.
#[derive(Default)]
pub(super) struct CiStatuses {
    /// The projects they are read from (a fork and its upstream, say).
    projects: HashSet<Project>,
    pub(super) by_sha: HashMap<String, Status>,
    /// The project each commit's status came from, whose builds its dot opens.
    source: HashMap<String, Project>,
    /// Unmerged statuses, retained independently when one project cannot be read.
    cached: route::Cached,
    /// The commits last asked about, each with the project it was asked of.
    asked: HashSet<(Project, String)>,
    loading: bool,
    loaded: Option<Instant>,
    /// When to read them again: a build runs, or one may only just have started.
    next: Option<Instant>,
    /// Why they couldn't be read (no tool, not logged in…).
    pub(super) error: Option<String>,
    /// The answer on its way was asked of the project before this one: it is dropped.
    superseded: bool,
}

impl CiStatuses {
    pub(super) fn account_changed(&mut self) {
        let in_flight = self.loading;
        *self = CiStatuses {
            projects: self.projects.clone(),
            loading: in_flight,
            superseded: in_flight,
            ..Default::default()
        };
    }
    /// The repository's builds are now read from `projects` (its remotes or upstreams
    /// changed): what was read from a project no longer among them goes, along with the answer
    /// still on its way. That one keeps the slot until it comes, so the two can't land in the
    /// wrong order.
    fn moved_to(&mut self, projects: HashSet<Project>) {
        let in_flight = self.loading;
        let answers = std::mem::take(&mut self.cached)
            .into_iter()
            .filter(|(project, _)| projects.contains(project))
            .map(|(project, statuses)| (project, Ok(statuses)))
            .collect();
        let merged = route::merge(answers, &[]).unwrap_or_default();
        *self = CiStatuses {
            projects,
            by_sha: merged.by_sha,
            source: merged.source,
            cached: merged.cached,
            loading: in_flight,
            superseded: in_flight,
            ..CiStatuses::default()
        };
    }

    /// Applies a refresh while preserving each failed project's last contribution.
    fn arrived(&mut self, fresh: bool, answers: Answers) {
        self.loading = false;
        if std::mem::take(&mut self.superseded) {
            // Asked of the projects before: read them again, from these, right away.
            if !self.projects.is_empty() {
                self.next = Some(Instant::now());
            }
            return;
        }
        match route::merge(answers, &self.cached) {
            Ok(merged) => {
                let running = merged
                    .by_sha
                    .values()
                    .any(|s| s.state == CheckState::Running);
                self.by_sha = merged.by_sha;
                self.source = merged.source;
                self.cached = merged.cached;
                self.error = merged.error;
                if running || fresh {
                    self.next = Some(Instant::now() + RUNNING_POLL);
                }
            }
            // What was read before still holds as well as anything.
            Err(e) => self.error = Some(e),
        }
    }
}

impl RepoTab {
    /// The repository's main project on its forge (that of `origin`): whether builds can be
    /// shown at all, and where those of a commit no remote has are asked.
    pub(super) fn ci_project(&self) -> Option<Project> {
        Project::of(self.forge()?, &self.remotes)
    }

    /// Every candidate for a commit's builds, preferring its last status source.
    pub(super) fn ci_projects_for(&self, sha: &str) -> Vec<Project> {
        let Some(main) = self.ci_project() else {
            return Vec::new();
        };
        let mut projects = route::projects_for(sha, &main, &self.remotes, &self.refs);
        if let Some(source) = self.ci.source.get(sha)
            && let Some(index) = projects.iter().position(|p| p == source)
        {
            projects[..=index].rotate_right(1);
        }
        projects
    }

    /// The build status of commit `sha`, if it has one.
    pub(super) fn ci_status(&self, sha: &str) -> Option<&Status> {
        self.ci.by_sha.get(sha)
    }

    /// The commits whose status is worth knowing (HEAD, the branches, the pull requests and
    /// the latest commits), grouped by the project each is asked of.
    fn ci_plan(&self) -> Plan {
        let Some(main) = self.ci_project() else {
            return Vec::new();
        };
        let sources = route::Sources {
            main: &main,
            remotes: &self.remotes,
            refs: &self.refs,
            prs: self.prs.iter().map(|pr| pr.sha.as_str()).collect(),
            recent: self
                .history
                .iter()
                .map(|c| c.hash.as_str())
                .filter(|hash| *hash != WIP_HASH)
                .take(RECENT)
                .collect(),
        };
        route::plan(&sources, MAX_SHAS)
    }

    /// Reads the statuses again if they are old, or commits came that weren't asked about
    /// (after a push, a fetch, a new pull request).
    pub(super) fn refresh_ci(&mut self, ctx: &egui::Context) {
        let plan = self.ci_plan();
        let projects: HashSet<Project> = plan.iter().map(|(p, _)| p.clone()).collect();
        if projects != self.ci.projects {
            self.ci.moved_to(projects);
        }
        if self.ci.projects.is_empty() {
            return;
        }
        let stale = self.ci.loaded.is_none_or(|t| t.elapsed() > STALE);
        let unasked = route::pairs(&plan)
            .iter()
            .any(|pair| !self.ci.asked.contains(pair));
        if stale || unasked {
            self.load_ci(ctx);
        }
    }

    pub(super) fn load_ci(&mut self, ctx: &egui::Context) {
        let plan = self.ci_plan();
        if self.ci.loading || plan.is_empty() {
            return;
        }
        let pairs = route::pairs(&plan);
        // New commits after others were asked about: maybe a push, whose build starts soon.
        let fresh = !self.ci.asked.is_empty() && pairs.iter().any(|p| !self.ci.asked.contains(p));
        self.ci.loading = true;
        self.ci.loaded = Some(Instant::now());
        self.ci.next = None;
        self.ci.asked = pairs;
        let path = self.path.clone();
        let first = plan[0].0.clone();
        // Answered all the same if it panics: no statuses are read again until it is.
        let failed = move |e| Msg::CiStatuses(fresh, vec![(first, Err(e))]);
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            let answers = plan
                .into_iter()
                .map(|(project, shas)| {
                    let answer = ci::statuses(&path, &project, &shas);
                    (project, answer)
                })
                .collect();
            Msg::CiStatuses(fresh, answers)
        });
    }

    pub(super) fn ci_arrived(&mut self, fresh: bool, answers: Answers) {
        self.ci.arrived(fresh, answers);
    }

    /// Reads the statuses again when it's time to; called every frame.
    pub(super) fn poll_ci(&mut self, ctx: &egui::Context) {
        let Some(next) = self.ci.next else { return };
        let now = Instant::now();
        // Offline or metered: asked again once that is over.
        if !crate::network::background_may_run() {
            ctx.request_repaint_after(std::time::Duration::from_secs(30));
            return;
        }
        if now >= next {
            self.load_ci(ctx);
        } else {
            ctx.request_repaint_after(next - now);
        }
    }
}

/// The colour of a build state.
pub(crate) fn state_color(t: &Tokens, state: CheckState) -> Color32 {
    match state {
        CheckState::Passed => t.success,
        CheckState::Failed => t.danger,
        CheckState::Running => t.warning,
        CheckState::Skipped => t.text_faint,
    }
}

/// Paints the dot of `state` around `center`: filled, or a ring while it runs.
pub(super) fn paint_dot(
    painter: &Painter,
    t: &Tokens,
    center: Pos2,
    radius: f32,
    state: CheckState,
) {
    let color = state_color(t, state);
    if state == CheckState::Running {
        painter.circle_stroke(center, radius - 0.5, Stroke::new(1.6, color));
    } else {
        painter.circle_filled(center, radius, color);
    }
}

/// What a screen reader says of a dot: the tally, then what clicking does.
fn spoken(status: &Status) -> String {
    let tally = ci::tally_label(&status.checks);
    let tally = if tally.is_empty() {
        state_word(status.state).to_owned()
    } else {
        tally
    };
    crate::a11y::labels::join([format!("builds: {tally}").as_str(), "opens the builds"])
}

/// What a dot's tooltip says: the tally, then the checks that need attention.
pub(super) fn tooltip(status: &Status) -> String {
    let mut text = match ci::tally_label(&status.checks) {
        tally if tally.is_empty() => format!("CI: {}", state_word(status.state)),
        tally => format!("CI: {tally}"),
    };
    let mut worst: Vec<_> = status
        .checks
        .iter()
        .filter(|c| matches!(c.state, CheckState::Failed | CheckState::Running))
        .collect();
    worst.sort_by_key(|c| c.state != CheckState::Failed);
    for check in worst.iter().take(8) {
        text.push_str(&format!("\n{}  {}", state_word(check.state), check.name));
    }
    if worst.len() > 8 {
        text.push_str(&format!("\n… and {} more", worst.len() - 8));
    }
    text.push_str("\nClick for details");
    text
}

pub(crate) fn state_word(state: CheckState) -> &'static str {
    match state {
        CheckState::Passed => "passed",
        CheckState::Failed => "failed",
        CheckState::Running => "running",
        CheckState::Skipped => "skipped",
    }
}

/// A clickable dot for `status` of commit `sha` around `center`; clicked, it asks for the
/// commit's builds in the big area.
pub(super) fn dot(ui: &Ui, id: Id, center: Pos2, sha: &str, status: &Status) -> Option<Action> {
    dot_in(ui, id, center, sha, status, true)
}

/// [`dot`] on a row of a list that is one Tab stop (the graph): only the stop row's dot is
/// `focusable`, so Tab doesn't stop on every row's dot.
pub(super) fn dot_in(
    ui: &Ui,
    id: Id,
    center: Pos2,
    sha: &str,
    status: &Status,
    focusable: bool,
) -> Option<Action> {
    let t = Tokens::get(ui.ctx());
    let rect = Rect::from_center_size(center, vec2(DOT_R * 2.0 + 6.0, DOT_R * 2.0 + 6.0));
    let resp: Response = ui.interact(rect, id, crate::a11y::list_row_sense(focusable));
    super::widgets::a11y(&resp, egui::WidgetType::Button, || spoken(status));
    let radius = if resp.hovered() { DOT_R + 1.0 } else { DOT_R };
    paint_dot(ui.painter(), &t, center, radius, status.state);
    let clicked = resp.on_hover_text(tooltip(status)).clicked();
    clicked.then(|| Action::OpenCi {
        sha: sha.to_owned(),
        run: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ci::Check;

    fn check(name: &str, state: CheckState) -> Check {
        Check {
            name: name.to_owned(),
            state,
            detail: String::new(),
            url: format!("https://ci/{name}"),
        }
    }

    #[test]
    fn the_tooltip_tallies_and_names_what_failed_first() {
        let status = Status {
            state: CheckState::Failed,
            checks: vec![
                check("test", CheckState::Running),
                check("lint", CheckState::Failed),
                check("docs", CheckState::Passed),
            ],
        };
        assert_eq!(
            tooltip(&status),
            "CI: 1 failed, 1 running, 1 passed\nfailed  lint\nrunning  test\nClick for details"
        );
    }

    fn project(path: &str) -> Project {
        Project {
            forge: crate::forge::Forge::GitHub,
            host: "github.com".to_owned(),
            path: path.to_owned(),
        }
    }

    fn passed() -> Status {
        Status {
            state: CheckState::Passed,
            checks: Vec::new(),
        }
    }

    fn only(path: &str) -> HashSet<Project> {
        HashSet::from([project(path)])
    }

    fn answer(path: &str, sha: &str) -> Answers {
        vec![(
            project(path),
            Ok(HashMap::from([(sha.to_owned(), passed())])),
        )]
    }

    #[test]
    fn statuses_asked_of_the_project_before_are_dropped() {
        let mut ci = CiStatuses {
            projects: only("me/fork"),
            loading: true,
            ..CiStatuses::default()
        };
        // The remote is pointed elsewhere while the fork's statuses are being read.
        ci.moved_to(only("org/upstream"));
        assert!(ci.loading, "no second read while the first is on its way");
        ci.arrived(false, answer("me/fork", "abc"));
        assert!(
            ci.by_sha.is_empty(),
            "the fork's statuses aren't upstream's"
        );
        assert!(!ci.loading);
        assert!(ci.next.is_some(), "read again from upstream");
        // Upstream's own answer is taken.
        ci.arrived(false, answer("org/upstream", "abc"));
        assert_eq!(ci.by_sha.len(), 1);
        assert_eq!(ci.source["abc"].path, "org/upstream");
    }

    #[test]
    fn a_project_change_with_nothing_on_its_way_drops_what_the_old_one_said() {
        let mut ci = CiStatuses {
            projects: only("me/fork"),
            by_sha: HashMap::from([("abc".to_owned(), passed())]),
            source: HashMap::from([("abc".to_owned(), project("me/fork"))]),
            cached: vec![(
                project("me/fork"),
                HashMap::from([("abc".to_owned(), passed())]),
            )],
            ..CiStatuses::default()
        };
        ci.moved_to(only("org/upstream"));
        assert!(ci.by_sha.is_empty());
        assert!(!ci.loading);
        ci.arrived(false, answer("org/upstream", "abc"));
        assert_eq!(ci.by_sha.len(), 1);
    }

    /// A remote added (a fork checked out to push to) doesn't blank the dots meanwhile.
    #[test]
    fn a_project_added_keeps_what_the_others_said() {
        let mut ci = CiStatuses {
            projects: only("org/upstream"),
            by_sha: HashMap::from([("abc".to_owned(), passed())]),
            source: HashMap::from([("abc".to_owned(), project("org/upstream"))]),
            cached: vec![(
                project("org/upstream"),
                HashMap::from([("abc".to_owned(), passed())]),
            )],
            ..CiStatuses::default()
        };
        ci.moved_to(HashSet::from([project("org/upstream"), project("me/fork")]));
        assert_eq!(ci.by_sha.len(), 1);
        assert_eq!(ci.source.len(), 1);
    }

    /// Removing a secondary source removes only its checks, even if the next read fails.
    #[test]
    fn removing_a_project_rebuilds_shared_statuses_from_retained_contributions() {
        let mut ci = CiStatuses::default();
        let mut failed = passed();
        failed.state = CheckState::Failed;
        ci.arrived(
            false,
            vec![
                (
                    project("me/fork"),
                    Ok(HashMap::from([("abc".to_owned(), passed())])),
                ),
                (
                    project("org/upstream"),
                    Ok(HashMap::from([("abc".to_owned(), failed)])),
                ),
            ],
        );
        assert_eq!(ci.by_sha["abc"].state, CheckState::Failed);
        ci.moved_to(only("me/fork"));
        assert_eq!(ci.by_sha["abc"].state, CheckState::Passed);
        assert_eq!(ci.cached.len(), 1);
        ci.arrived(false, vec![(project("me/fork"), Err("offline".to_owned()))]);
        assert_eq!(ci.by_sha["abc"].state, CheckState::Passed);
        assert_eq!(ci.source["abc"], project("me/fork"));
    }

    #[test]
    fn a_status_without_checks_says_its_state() {
        let status = Status {
            state: CheckState::Passed,
            checks: Vec::new(),
        };
        assert_eq!(tooltip(&status), "CI: passed\nClick for details");
    }
}
