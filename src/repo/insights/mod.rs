//! Insights in the big area (issue #175): interactive views of what changes affect, how CI
//! spends its time, what the worktrees and agents are doing, how a review stands and what is
//! deployed. Each is read in the background, says where its data came from and how complete it
//! is, has a table beside its picture, and opens the commit, diff, file, pull request, run or
//! worktree behind what is clicked. None changes anything.

pub(crate) mod common;
pub(crate) mod graph;
pub(crate) mod heat_grid;
mod page;
#[cfg(test)]
mod tests;

mod churn;
mod ci_path;
mod conflict_radar;
mod constellation;
mod coupling;
mod coverage;
mod deploy;
mod evolution;
mod impact;
mod review_heat;
mod ribbons;
mod transform;

use std::collections::HashSet;

use super::RepoTab;

/// The views, in the order they are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Impact,
    CiPath,
    Constellation,
    Evolution,
    Coupling,
    ConflictRadar,
    ReviewHeat,
    Transform,
    Deploy,
    Ribbons,
    Coverage,
    Churn,
}

impl Kind {
    pub(crate) const ALL: [Kind; 12] = [
        Kind::Impact,
        Kind::CiPath,
        Kind::Constellation,
        Kind::Evolution,
        Kind::Coupling,
        Kind::ConflictRadar,
        Kind::ReviewHeat,
        Kind::Transform,
        Kind::Deploy,
        Kind::Ribbons,
        Kind::Coverage,
        Kind::Churn,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Kind::Impact => "Change impact",
            Kind::CiPath => "CI critical path",
            Kind::Constellation => "Worktrees and agents",
            Kind::Evolution => "Code evolution",
            Kind::Coupling => "Change coupling",
            Kind::ConflictRadar => "Conflict radar",
            Kind::ReviewHeat => "Review heatmap",
            Kind::Transform => "Repeated edits",
            Kind::Deploy => "Deployments",
            Kind::Ribbons => "Rebase ribbons",
            Kind::Coverage => "Test coverage",
            Kind::Churn => "Churn and complexity",
        }
    }

    /// What it shows, in a line.
    pub(crate) fn blurb(self) -> &'static str {
        match self {
            Kind::Impact => {
                "What refers to the symbols a change touched, at the revisions compared"
            }
            Kind::CiPath => {
                "A run's jobs on a time line: the critical path, waiting, and what got slower"
            }
            Kind::Constellation => {
                "Branches, worktrees, agent tasks, pull requests and their builds, joined"
            }
            Kind::Evolution => "Which files and folders changed in which commits, and how much",
            Kind::Coupling => "Files that change together, and those left out of a change",
            Kind::ConflictRadar => "Which active branches overlap or conflict when merged",
            Kind::ReviewHeat => {
                "What of a pull request was reviewed, changed since, or has open threads"
            }
            Kind::Transform => "The same edit made across files, and where it was made otherwise",
            Kind::Deploy => "Which environments are known to run a commit, and how freshly known",
            Kind::Ribbons => "How each commit of a rewritten branch maps onto its earlier version",
            Kind::Coverage => {
                "What coverage reports say of the lines a change touched, for that revision"
            }
            Kind::Churn => "Size, churn and complexity of files over time, and rising hotspots",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Kind::Impact => "code",
            Kind::CiPath => "rocket",
            Kind::Constellation => "folder-tree",
            Kind::Evolution => "history",
            Kind::Coupling => "git-merge",
            Kind::ConflictRadar => "circle-alert",
            Kind::ReviewHeat => "eye",
            Kind::Transform => "copy",
            Kind::Deploy => "server",
            Kind::Ribbons => "arrow-down-up",
            Kind::Coverage => "flask-conical",
            Kind::Churn => "chart-column",
        }
    }

    /// The group it is listed under.
    pub(crate) fn group(self) -> &'static str {
        match self {
            Kind::Impact | Kind::ReviewHeat | Kind::Transform | Kind::Coverage => "Review",
            Kind::CiPath | Kind::Deploy => "CI and delivery",
            Kind::Constellation | Kind::ConflictRadar | Kind::Ribbons => "Branches and agents",
            Kind::Evolution | Kind::Coupling | Kind::Churn => "History",
        }
    }
}

/// What the insights page keeps, while it is open.
pub(crate) struct InsightsView {
    pub(crate) kind: Kind,
    /// Tables in place of the pictures (for reading, copying, a screen reader).
    pub(crate) table: bool,
    /// The views read once already: showing one again doesn't read it again.
    loaded: HashSet<Kind>,
    /// The pull request the big area showed when the page was opened over it, which the
    /// views about a change take for theirs.
    pub(crate) pr: Option<u64>,
    pub(super) impact: impact::State,
    pub(super) ci_path: ci_path::State,
    pub(super) constellation: constellation::State,
    pub(super) evolution: evolution::State,
    pub(super) coupling: coupling::State,
    pub(super) conflict_radar: conflict_radar::State,
    pub(super) review_heat: review_heat::State,
    pub(super) transform: transform::State,
    pub(super) deploy: deploy::State,
    pub(super) ribbons: ribbons::State,
    pub(super) coverage: coverage::State,
    pub(super) churn: churn::State,
}

impl InsightsView {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            table: false,
            loaded: HashSet::new(),
            pr: None,
            impact: Default::default(),
            ci_path: Default::default(),
            constellation: Default::default(),
            evolution: Default::default(),
            coupling: Default::default(),
            conflict_radar: Default::default(),
            review_heat: Default::default(),
            transform: Default::default(),
            deploy: Default::default(),
            ribbons: Default::default(),
            coverage: Default::default(),
            churn: Default::default(),
        }
    }
}

/// What the insights page asks for.
pub(crate) enum InsightsAct {
    /// Open the page at a view (or show that view, when it is open).
    Open(Kind),
    /// Read the view shown again.
    Reload,
    /// Tables in place of the pictures, or the pictures again.
    ToggleTable,
    /// Esc: what the view shows selected first, then the page.
    Back,
    Close,
    Impact(impact::Act),
    CiPath(ci_path::Act),
    Constellation(constellation::Act),
    Evolution(evolution::Act),
    Coupling(coupling::Act),
    ConflictRadar(conflict_radar::Act),
    ReviewHeat(review_heat::Act),
    Transform(transform::Act),
    Deploy(deploy::Act),
    Ribbons(ribbons::Act),
    Coverage(coverage::Act),
    Churn(churn::Act),
}

impl RepoTab {
    pub(super) fn insights_act(&mut self, act: InsightsAct, ctx: &egui::Context) {
        match act {
            InsightsAct::Open(kind) => self.open_insights(kind, ctx),
            InsightsAct::Reload => {
                if let Some(kind) = self.insights.as_ref().map(|v| v.kind) {
                    self.load_insight(kind, ctx);
                }
            }
            InsightsAct::ToggleTable => {
                if let Some(view) = &mut self.insights {
                    view.table = !view.table;
                }
            }
            InsightsAct::Back => {
                if !self.insight_back() {
                    self.insights = None;
                }
            }
            InsightsAct::Close => self.insights = None,
            InsightsAct::Impact(act) => self.impact_act(act, ctx),
            InsightsAct::CiPath(act) => self.ci_path_act(act, ctx),
            InsightsAct::Constellation(act) => self.constellation_act(act, ctx),
            InsightsAct::Evolution(act) => self.evolution_act(act, ctx),
            InsightsAct::Coupling(act) => self.coupling_act(act, ctx),
            InsightsAct::ConflictRadar(act) => self.conflict_radar_act(act, ctx),
            InsightsAct::ReviewHeat(act) => self.review_heat_act(act, ctx),
            InsightsAct::Transform(act) => self.transform_act(act, ctx),
            InsightsAct::Deploy(act) => self.deploy_act(act, ctx),
            InsightsAct::Ribbons(act) => self.ribbons_act(act, ctx),
            InsightsAct::Coverage(act) => self.coverage_act(act, ctx),
            InsightsAct::Churn(act) => self.churn_act(act, ctx),
        }
    }

    /// Shows the insights page in the big area at `kind`, read the first time it is shown.
    pub(super) fn open_insights(&mut self, kind: Kind, ctx: &egui::Context) {
        // Those editors hold choices written nowhere yet and are drawn over the page.
        let editor = if self.conflict.is_some() {
            Some("conflict editor")
        } else if self.rebase.is_some() {
            Some("interactive rebase")
        } else if self.split.is_some() {
            Some("commit split")
        } else if self.absorb.is_some() {
            Some("absorb editor")
        } else {
            None
        };
        if let Some(editor) = editor {
            self.status = Some(crate::repo::Notice::plain((
                format!("Finish or close the {editor} first"),
                true,
            )));
            return;
        }
        let pr = self.pr_view.as_ref().map(|v| v.number);
        // The page takes the big area: what was open over it closes.
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.close_pr();
        self.versions.view = None;
        self.timeline_view = None;
        self.doctor = None;
        self.bakeoff.view = None;
        let view = self
            .insights
            .get_or_insert_with(|| Box::new(InsightsView::new(kind)));
        view.kind = kind;
        if pr.is_some() {
            view.pr = pr;
        }
        if view.loaded.insert(kind) {
            self.load_insight(kind, ctx);
        }
    }

    /// Reads view `kind` (again).
    fn load_insight(&mut self, kind: Kind, ctx: &egui::Context) {
        match kind {
            Kind::Impact => self.impact_load(ctx),
            Kind::CiPath => self.ci_path_load(ctx),
            Kind::Constellation => self.constellation_load(ctx),
            Kind::Evolution => self.evolution_load(ctx),
            Kind::Coupling => self.coupling_load(ctx),
            Kind::ConflictRadar => self.conflict_radar_load(ctx),
            Kind::ReviewHeat => self.review_heat_load(ctx),
            Kind::Transform => self.transform_load(ctx),
            Kind::Deploy => self.deploy_load(ctx),
            Kind::Ribbons => self.ribbons_load(ctx),
            Kind::Coverage => self.coverage_load(ctx),
            Kind::Churn => self.churn_load(ctx),
        }
    }

    /// What Esc takes back in the view shown (a selection), if anything.
    fn insight_back(&mut self) -> bool {
        let Some(kind) = self.insights.as_ref().map(|v| v.kind) else {
            return false;
        };
        match kind {
            Kind::Impact => self.impact_back(),
            Kind::CiPath => self.ci_path_back(),
            Kind::Constellation => self.constellation_back(),
            Kind::Evolution => self.evolution_back(),
            Kind::Coupling => self.coupling_back(),
            Kind::ConflictRadar => self.conflict_radar_back(),
            Kind::ReviewHeat => self.review_heat_back(),
            Kind::Transform => self.transform_back(),
            Kind::Deploy => self.deploy_back(),
            Kind::Ribbons => self.ribbons_back(),
            Kind::Coverage => self.coverage_back(),
            Kind::Churn => self.churn_back(),
        }
    }

    /// The view shown draws itself below the page's header.
    fn insight_ui(&mut self, kind: Kind, ui: &mut egui::Ui, acts: &mut Vec<super::Action>) {
        match kind {
            Kind::Impact => self.impact_ui(ui, acts),
            Kind::CiPath => self.ci_path_ui(ui, acts),
            Kind::Constellation => self.constellation_ui(ui, acts),
            Kind::Evolution => self.evolution_ui(ui, acts),
            Kind::Coupling => self.coupling_ui(ui, acts),
            Kind::ConflictRadar => self.conflict_radar_ui(ui, acts),
            Kind::ReviewHeat => self.review_heat_ui(ui, acts),
            Kind::Transform => self.transform_ui(ui, acts),
            Kind::Deploy => self.deploy_ui(ui, acts),
            Kind::Ribbons => self.ribbons_ui(ui, acts),
            Kind::Coverage => self.coverage_ui(ui, acts),
            Kind::Churn => self.churn_ui(ui, acts),
        }
    }

    /// [`RepoTab::then`] for a view's read: what it runs fetches nothing (see
    /// [`crate::git::offline`]).
    pub(super) fn insight_then<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        self.then(ctx, move || crate::git::offline(work), then);
    }

    /// [`RepoTab::forge_then`] for a view's read of the forge and the repository: the git it
    /// runs fetches nothing.
    pub(super) fn insight_forge_then<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        self.forge_then(ctx, move || crate::git::offline(work), then);
    }

    /// The palette's entries: one per view.
    pub(super) fn insights_palette_items(&self) -> Vec<(String, String, super::Action)> {
        Kind::ALL
            .iter()
            .map(|&kind| {
                (
                    format!("Insights: {}", kind.title()),
                    format!("insights {}", kind.group().to_lowercase()),
                    super::Action::Insights(InsightsAct::Open(kind)),
                )
            })
            .collect()
    }
}
