//! The bake-off in the big area: 2–4 attempts at the same task compared in a grid, any file
//! of theirs side by side, and _Assemble_ to make a branch of the best parts. The logic is in
//! [`crate::agent::bakeoff`].

mod picker;
mod view;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::agent::Launch;
use crate::agent::bakeoff::assemble::{self, Plan, Take};
use crate::agent::bakeoff::build::{self, Built};
use crate::agent::bakeoff::hunks::Hunk;
use crate::agent::bakeoff::{self as logic, Attempt, Comparison};
use crate::repo::RepoTab;

pub(super) use picker::Picker;

/// How often the builds still running are looked at.
const BUILD_POLL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct BakeoffState {
    pub(super) picker: Option<Picker>,
    pub(super) view: Option<Box<Bakeoff>>,
}

/// The bake-off shown.
pub(crate) struct Bakeoff {
    attempts: Vec<Attempt>,
    comparison: Option<Result<Comparison, String>>,
    /// The file opened side by side, and each attempt's take on it (`None` while read).
    file: Option<(String, Option<Sides>)>,
    picks: BTreeMap<String, Take>,
    branch: String,
    build_command: String,
    builds: Vec<Option<Built>>,
    polled: Option<Instant>,
    assembling: bool,
    note: Option<(String, bool)>,
}

/// Each attempt's take on the file shown, or why it couldn't be read.
type Sides = Result<Vec<Side>, String>;

/// An attempt's take on the file shown: its patch from the base, and its hunks to pick.
pub(crate) struct Side {
    patch: String,
    hunks: Vec<Hunk>,
}

/// What the bake-off views ask for.
pub(crate) enum BakeAct {
    OpenPicker,
    ClosePicker,
    Compare(Vec<Attempt>),
    Close,
    Refresh,
    OpenFile(String),
    CloseFile,
    TakeFile(String, usize),
    ToggleHunk(String, usize, usize),
    ClearPick(String),
    Assemble,
    Build(usize),
    BuildAll,
}

impl Bakeoff {
    fn new(attempts: Vec<Attempt>, build_command: String) -> Self {
        let n = attempts.len();
        Self {
            branch: format!(
                "bakeoff/{}",
                attempts
                    .first()
                    .map_or("best", |a| a.name.as_str())
                    .replace(['/', ' '], "-")
            ),
            attempts,
            comparison: None,
            file: None,
            picks: BTreeMap::new(),
            build_command,
            builds: vec![None; n],
            polled: None,
            assembling: false,
            note: None,
        }
    }
}

impl RepoTab {
    pub(crate) fn bake_act(&mut self, act: BakeAct, ctx: &egui::Context) {
        match act {
            BakeAct::OpenPicker => self.bakeoff.picker = Some(Picker::new(self)),
            BakeAct::ClosePicker => self.bakeoff.picker = None,
            BakeAct::Compare(attempts) => {
                self.bakeoff.picker = None;
                let command = self
                    .bakeoff
                    .view
                    .as_ref()
                    .map(|v| v.build_command.clone())
                    .unwrap_or_default();
                self.close_pr();
                self.ci_view = None;
                self.diff = None;
                self.bakeoff.view = Some(Box::new(Bakeoff::new(attempts, command)));
                self.load_bakeoff(ctx);
            }
            BakeAct::Close => self.bakeoff.view = None,
            BakeAct::Refresh => self.load_bakeoff(ctx),
            BakeAct::OpenFile(path) => self.open_bakeoff_file(path, ctx),
            BakeAct::CloseFile => {
                if let Some(view) = &mut self.bakeoff.view {
                    view.file = None;
                }
            }
            BakeAct::TakeFile(path, n) => self.pick(path, |_| Some(Take::File(n))),
            BakeAct::ToggleHunk(path, n, i) => self.pick(path, |take| {
                let mut list = match take {
                    Some(Take::Hunks(list)) => list,
                    _ => Vec::new(),
                };
                if let Some(at) = list.iter().position(|h| *h == (n, i)) {
                    list.remove(at);
                } else {
                    list.push((n, i));
                }
                (!list.is_empty()).then_some(Take::Hunks(list))
            }),
            BakeAct::ClearPick(path) => self.pick(path, |_| None),
            BakeAct::Assemble => self.assemble(ctx),
            BakeAct::Build(n) => self.build(n, ctx),
            BakeAct::BuildAll => {
                let n = self.bakeoff.view.as_ref().map_or(0, |v| v.attempts.len());
                for i in 0..n {
                    self.build(i, ctx);
                }
            }
        }
    }

    fn pick(&mut self, path: String, change: impl FnOnce(Option<Take>) -> Option<Take>) {
        if let Some(view) = &mut self.bakeoff.view {
            let now = view.picks.remove(&path);
            if let Some(take) = change(now) {
                view.picks.insert(path, take);
            }
            view.note = None;
        }
    }

    fn load_bakeoff(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        view.comparison = None;
        view.file = None;
        let attempts = view.attempts.clone();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let command: String =
                    crate::agent::read_json(&repo, build::COMMAND_FILE).unwrap_or_default();
                (logic::compare(&repo, &attempts), command)
            },
            |tab, (result, command), _| {
                if let Some(view) = &mut tab.bakeoff.view {
                    if view.build_command.is_empty() {
                        view.build_command = command;
                    }
                    // Picks made for other trees no longer fit.
                    let moved = match (&view.comparison, &result) {
                        (Some(Ok(old)), Ok(new)) => old
                            .attempts
                            .iter()
                            .map(|a| &a.tree)
                            .ne(new.attempts.iter().map(|a| &a.tree)),
                        _ => true,
                    };
                    if moved {
                        view.picks.clear();
                    }
                    view.comparison = Some(result);
                }
            },
        );
    }

    fn open_bakeoff_file(&mut self, path: String, ctx: &egui::Context) {
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let Some(Ok(cmp)) = &view.comparison else {
            return;
        };
        let base = cmp.base.clone();
        let trees: Vec<String> = cmp.attempts.iter().map(|a| a.tree.clone()).collect();
        view.file = Some((path.clone(), None));
        let repo = self.path.clone();
        let wanted = path.clone();
        self.then(
            ctx,
            move || {
                trees
                    .iter()
                    .map(|tree| {
                        Ok(Side {
                            patch: logic::file_patch(&repo, &base, tree, &path)?,
                            hunks: assemble::file_hunks(&repo, &base, tree, &path)?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            },
            move |tab, sides, _| {
                if let Some(view) = &mut tab.bakeoff.view
                    && let Some((shown, slot)) = &mut view.file
                    && *shown == wanted
                {
                    *slot = Some(sides);
                }
            },
        );
    }

    fn assemble(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let Some(Ok(cmp)) = &view.comparison else {
            return;
        };
        let plan = Plan {
            base: cmp.base.clone(),
            trees: cmp.attempts.iter().map(|a| a.tree.clone()).collect(),
            names: view.attempts.iter().map(|a| a.name.clone()).collect(),
            picks: view
                .picks
                .iter()
                .map(|(p, t)| (p.clone(), t.clone()))
                .collect(),
            branch: view.branch.trim().to_owned(),
        };
        view.assembling = true;
        view.note = None;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || assemble::assemble(&repo, &plan).map(|commit| (plan.branch, commit)),
            |tab, result, ctx| {
                if let Some(view) = &mut tab.bakeoff.view {
                    view.assembling = false;
                    view.note = Some(match &result {
                        Ok((branch, commit)) => (
                            format!(
                                "Made {branch} at {}: check it out or compare it from the graph",
                                crate::repo::short(commit)
                            ),
                            false,
                        ),
                        Err(e) => (e.clone(), true),
                    });
                }
                if result.is_ok() {
                    tab.reload(ctx);
                }
            },
        );
    }

    /// Builds attempt `n` in a terminal of its worktree.
    fn build(&mut self, n: usize, ctx: &egui::Context) {
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let command = view.build_command.trim().to_owned();
        let Some(worktree) = view.attempts.get(n).and_then(|a| a.worktree.clone()) else {
            return;
        };
        if command.is_empty() {
            view.note = Some((
                "Type the command that builds the project first".to_owned(),
                true,
            ));
            return;
        }
        let name = view
            .attempts
            .get(n)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        if let Some(slot) = view.builds.get_mut(n) {
            *slot = Some(Built::Running);
        }
        let repo = self.path.clone();
        let remembered = command.clone();
        let wt = worktree.clone();
        self.then(
            ctx,
            move || -> Result<PathBuf, String> {
                let _ = crate::agent::write_json(&repo, build::COMMAND_FILE, &remembered);
                let status = build::status_file(&repo, &wt)?;
                // An earlier build's status isn't this one's.
                if let Some(dir) = status.parent() {
                    let _ = crate::remote::create_dir_all(dir);
                }
                crate::remote::write(&status, b"")
                    .map_err(|e| format!("could not prepare the build's status file: {e}"))?;
                Ok(status)
            },
            move |tab, status, ctx| match status {
                Ok(status) => {
                    let there = build::for_sh(&status);
                    tab.start_launch(
                        Launch {
                            label: format!("build · {name}"),
                            worktree,
                            run: Some(build::script(&command, &there)),
                            ..Launch::default()
                        },
                        ctx,
                    );
                }
                Err(e) => {
                    if let Some(view) = &mut tab.bakeoff.view {
                        view.note = Some((e, true));
                        if let Some(slot) = view.builds.get_mut(n) {
                            *slot = None;
                        }
                    }
                }
            },
        );
    }

    /// Every frame the bake-off is shown: the builds still running are looked at now and then.
    fn poll_builds(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let running: Vec<(usize, PathBuf)> = view
            .builds
            .iter()
            .zip(&view.attempts)
            .enumerate()
            .filter(|(_, (b, _))| **b == Some(Built::Running))
            .filter_map(|(i, (_, a))| a.worktree.clone().map(|w| (i, w)))
            .collect();
        if running.is_empty() || view.polled.is_some_and(|t| t.elapsed() < BUILD_POLL) {
            if !running.is_empty() {
                ctx.request_repaint_after(BUILD_POLL);
            }
            return;
        }
        view.polled = Some(Instant::now());
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                running
                    .into_iter()
                    .map(|(i, wt)| {
                        let text = build::status_file(&repo, &wt)
                            .ok()
                            .and_then(|f| crate::remote::read_to_string(&f).ok());
                        (i, build::read_status(text.as_deref()))
                    })
                    .collect::<Vec<_>>()
            },
            |tab, found, _| {
                if let Some(view) = &mut tab.bakeoff.view {
                    for (i, built) in found {
                        if let Some(slot) = view.builds.get_mut(i)
                            && *slot == Some(Built::Running)
                        {
                            *slot = Some(built);
                        }
                    }
                }
            },
        );
    }
}

#[cfg(test)]
mod tests;
