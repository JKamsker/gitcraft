//! The bake-off in the big area: 2–4 attempts at the same task compared in a grid, any file
//! of theirs side by side, and _Assemble_ to make a branch of the best parts. The logic is in
//! [`crate::agent::bakeoff`].

mod picker;
mod view;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static COMPARISONS: AtomicU64 = AtomicU64::new(1);
fn next_request() -> u64 {
    COMPARISONS.fetch_add(1, Ordering::Relaxed)
}
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
    /// Numbers the comparisons asked for: an older answer doesn't replace a newer one.
    request: u64,
    /// The attempts' trees the picks were made for.
    picked_trees: Vec<String>,
    /// Every file an attempt touched, and which touched it: worked out once per comparison.
    rows: Vec<(String, Vec<bool>)>,
    /// Where each build leaves its status, once one was started.
    statuses: Vec<Option<PathBuf>>,
    launches: Vec<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// The commit _Assemble_ made last, to show in the graph.
    made: Option<String>,
    /// Close was asked for once with picks made: a second time drops them.
    confirm_close: bool,
}

/// Each attempt's take on the file shown, or why it couldn't be read.
type Sides = Result<Vec<Side>, String>;

/// An attempt's take on the file shown: its patch from the base, and its hunks to pick.
pub(crate) struct Side {
    patch: String,
    hunks: Vec<Hunk>,
}

impl Side {
    /// What the attempt did to the file, as its patch says.
    fn kind(&self) -> &'static str {
        if self.patch.trim().is_empty() {
            "unchanged"
        } else if self.patch.contains("\ndeleted file mode") {
            "deleted"
        } else if self.patch.contains("\nnew file mode") {
            "added"
        } else {
            "changed"
        }
    }
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
    /// Shows the commit _Assemble_ made in the graph.
    ShowMade,
    Output(usize),
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
            request: next_request(),
            picked_trees: Vec::new(),
            rows: Vec::new(),
            statuses: vec![None; n],
            launches: vec![None; n],
            made: None,
            confirm_close: false,
        }
    }
}

impl RepoTab {
    pub(crate) fn bake_act(&mut self, act: BakeAct, ctx: &egui::Context) {
        match act {
            BakeAct::Output(n) => {
                if let Some(path) = self
                    .bakeoff
                    .view
                    .as_ref()
                    .and_then(|v| v.statuses.get(n))
                    .and_then(|s| s.as_ref())
                {
                    let log = crate::remote::resolve(
                        path.parent().unwrap_or(&self.path),
                        &format!(
                            "{}.log",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                    );
                    self.apply(super::Action::OpenInEditor(log), ctx);
                }
            }
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
                self.file_view = None;
                self.bakeoff.view = Some(Box::new(Bakeoff::new(attempts, command)));
                self.load_bakeoff(ctx);
            }
            BakeAct::Close => {
                if let Some(view) = &mut self.bakeoff.view
                    && !view.picks.is_empty()
                    && !view.confirm_close
                {
                    view.confirm_close = true;
                    let n = view.picks.len();
                    view.note = Some((
                        crate::i18n::message(
                            "repo-workflows-bakeoff-unassembled",
                            &[("count", (n).into())],
                        ),
                        true,
                    ));
                } else {
                    self.bakeoff.view = None;
                }
            }
            BakeAct::ShowMade => {
                if let Some(commit) = self.bakeoff.view.take().and_then(|v| v.made) {
                    self.goto(&commit, ctx);
                }
            }
            BakeAct::Refresh => self.load_bakeoff(ctx),
            BakeAct::OpenFile(path) => self.open_bakeoff_file(path, ctx),
            BakeAct::CloseFile => {
                if let Some(view) = &mut self.bakeoff.view {
                    view.file = None;
                }
            }
            // A second click takes it back.
            BakeAct::TakeFile(path, n) => self.pick(path, |take| {
                (take != Some(Take::File(n))).then_some(Take::File(n))
            }),
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
                let idle: Vec<usize> = self.bakeoff.view.as_ref().map_or_else(Vec::new, |v| {
                    (0..v.attempts.len())
                        .filter(|i| {
                            !matches!(
                                v.builds.get(*i),
                                Some(Some(Built::Preparing | Built::Running))
                            )
                        })
                        .collect()
                });
                for i in idle {
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
            view.confirm_close = false;
        }
    }

    fn load_bakeoff(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        view.comparison = None;
        view.builds.fill(None);
        view.statuses.fill(None);
        view.launches.fill(None);
        view.file = None;
        view.request = next_request();
        let request = view.request;
        let attempts = view.attempts.clone();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let command: String =
                    crate::agent::read_json(&repo, build::COMMAND_FILE).unwrap_or_default();
                (logic::compare(&repo, &attempts), command)
            },
            move |tab, (result, command), _| {
                let Some(view) = &mut tab.bakeoff.view else {
                    return;
                };
                if view.request != request {
                    return;
                }
                if view.build_command.is_empty() {
                    view.build_command = command;
                }
                // Picks made for other trees no longer fit.
                if let Ok(new) = &result {
                    let trees: Vec<String> = new.attempts.iter().map(|a| a.tree.clone()).collect();
                    if trees != view.picked_trees {
                        if !view.picks.is_empty() {
                            view.note = Some((
                                crate::i18n::text("repo-workflows-bakeoff-load-bakeoff-c6f043db")
                                    .to_owned(),
                                true,
                            ));
                        }
                        view.picks.clear();
                        view.picked_trees = trees;
                    }
                    view.rows = new.files();
                }
                view.comparison = Some(result);
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
        let request = view.request;
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
                    && view.request == request
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
        let request = view.request;
        view.assembling = true;
        view.note = None;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || assemble::assemble(&repo, &plan).map(|commit| (plan.branch, commit)),
            move |tab, result, ctx| {
                if let Some(view) = &mut tab.bakeoff.view
                    && view.request == request
                {
                    view.assembling = false;
                    view.note = Some(match &result {
                        Ok((branch, commit)) => {
                            view.made = Some(commit.clone());
                            // Another go at it needs a name of its own.
                            view.branch = next_name(branch);
                            (
                                crate::i18n::message(
                                    "repo-workflows-bakeoff-assemble-c7f55e42",
                                    &[
                                        ("branch", (branch).clone().into()),
                                        (
                                            "crate-repo-short-commit-1",
                                            (crate::repo::short(commit)).clone().into(),
                                        ),
                                    ],
                                ),
                                false,
                            )
                        }
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
        let Some(Ok(comparison)) = &view.comparison else {
            return;
        };
        let Some(compared) = comparison.attempts.get(n) else {
            return;
        };
        if view.builds.get(n) == Some(&Some(Built::Preparing)) {
            return;
        }
        let tree = compared.tree.clone();
        let base = comparison.base.clone();
        let request = view.request;
        if command.is_empty() {
            view.note = Some((
                crate::i18n::text("repo-workflows-bakeoff-build-c18d3706").to_owned(),
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
            *slot = Some(Built::Preparing);
        }
        if let Some(status) = view.statuses.get_mut(n) {
            *status = None;
        }
        if let Some(launch) = view.launches.get_mut(n) {
            *launch = None;
        }
        let repo = self.path.clone();
        let remembered = command.clone();
        self.then(
            ctx,
            move || -> Result<(PathBuf, PathBuf), String> {
                let _ = crate::agent::write_json(&repo, build::COMMAND_FILE, &remembered);
                let worktree = build::workspace(&repo, &tree, &base)?;
                let status = build::status_file(&repo, &worktree)?;
                // An earlier build's status isn't this one's.
                if let Some(dir) = status.parent() {
                    let _ = crate::remote::create_dir_all(dir);
                }
                crate::remote::write(&status, b"").map_err(|e| {
                    crate::i18n::message(
                        "repo-workflows-bakeoff-build-06d160c3",
                        &[("e", (e).to_string().into())],
                    )
                })?;
                Ok((worktree, status))
            },
            move |tab, status, ctx| match status {
                Ok((worktree, status)) => {
                    if tab
                        .bakeoff
                        .view
                        .as_ref()
                        .is_none_or(|v| v.request != request)
                    {
                        return;
                    }
                    let there = build::for_sh(&status);
                    if let Some(view) = &mut tab.bakeoff.view
                        && let Some(slot) = view.statuses.get_mut(n)
                    {
                        *slot = Some(status.clone());
                    }
                    let launched = tab.start_launch_tracked(
                        Launch {
                            label: format!("build · {name}"),
                            worktree,
                            run: Some(build::script(&command, &there)),
                            ..Launch::default()
                        },
                        ctx,
                    );
                    if let Some(view) = &mut tab.bakeoff.view {
                        view.accept_launch(request, n, &status, launched);
                    }
                }
                Err(e) => {
                    if let Some(view) = &mut tab.bakeoff.view
                        && view.request == request
                    {
                        view.note = Some((e, true));
                        if let Some(slot) = view.builds.get_mut(n) {
                            *slot = Some(Built::Unavailable);
                        }
                    }
                }
            },
        );
    }

    /// Every frame the bake-off is shown: the builds still running are looked at now and then.
    fn poll_builds(&mut self, ctx: &egui::Context) {
        // Atomic observation only: no try_wait, process search or signals on UI.
        let terminals = self
            .terminals
            .iter()
            .map(|terminal| terminal.output_lifetime())
            .collect::<Vec<_>>();
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let running: Vec<(usize, PathBuf, bool)> = view
            .builds
            .iter()
            .zip(&view.statuses)
            .enumerate()
            .filter(|(_, (b, _))| **b == Some(Built::Running))
            .filter_map(|(i, (_, file))| {
                file.clone().map(|f| {
                    let open = view
                        .launches
                        .get(i)
                        .and_then(|launch| launch.as_ref())
                        .is_some_and(|witness| {
                            !witness.load(std::sync::atomic::Ordering::Relaxed)
                                && terminals
                                    .iter()
                                    .any(|terminal| std::sync::Arc::ptr_eq(terminal, witness))
                        });
                    (i, f, open)
                })
            })
            .collect();
        if running.is_empty() || view.polled.is_some_and(|t| t.elapsed() < BUILD_POLL) {
            if !running.is_empty() {
                ctx.request_repaint_after(BUILD_POLL);
            }
            return;
        }
        view.polled = Some(Instant::now());
        let request = view.request;
        self.then(
            ctx,
            move || {
                running
                    .into_iter()
                    .map(|(i, file, open)| {
                        let text = crate::remote::read_to_string(&file).ok();
                        (i, file, build::observed_status(text.as_deref(), open))
                    })
                    .collect::<Vec<_>>()
            },
            move |tab, found, _| {
                if let Some(view) = &mut tab.bakeoff.view {
                    view.accept_builds(request, found);
                }
            },
        );
    }
}

/// The name for another go after `branch` was made: `x` → `x-2`, `x-2` → `x-3`.
fn next_name(branch: &str) -> String {
    match branch.rsplit_once('-') {
        Some((stem, n)) if n.parse::<u32>().is_ok_and(|n| n >= 2) => {
            format!("{stem}-{}", n.parse::<u32>().unwrap_or(1) + 1)
        }
        _ => format!("{branch}-2"),
    }
}

#[cfg(test)]
mod tests;

impl Bakeoff {
    fn accept_launch(
        &mut self,
        request: u64,
        n: usize,
        status: &Path,
        launched: Result<std::sync::Arc<std::sync::atomic::AtomicBool>, String>,
    ) {
        if self.request != request
            || self.statuses.get(n) != Some(&Some(status.to_path_buf()))
            || self.builds.get(n) != Some(&Some(Built::Preparing))
        {
            return;
        }
        match launched {
            Ok(witness) => {
                self.launches[n] = Some(witness);
                self.builds[n] = Some(Built::Running);
            }
            Err(error) => {
                self.builds[n] = Some(Built::Unavailable);
                self.note = Some((error, true));
            }
        }
    }
    fn accept_builds(&mut self, request: u64, found: Vec<(usize, PathBuf, Built)>) {
        if self.request != request {
            return;
        }
        for (i, path, built) in found {
            if self.statuses.get(i) == Some(&Some(path))
                && let Some(slot) = self.builds.get_mut(i)
                && *slot == Some(Built::Running)
            {
                *slot = Some(built);
            }
        }
    }
}
#[cfg(test)]
mod generation_tests {
    use super::*;
    #[test]
    fn pure_bakeoff_recorded_launch_failure_is_retryable_and_stale_launch_receipts_are_rejected()
    -> Result<(), String> {
        let mut view = Bakeoff::new(
            vec![Attempt {
                name: "a".into(),
                rev: "a".into(),
                worktree: None,
            }],
            String::new(),
        );
        let status = PathBuf::from("owned-unique-receipt");
        view.statuses[0] = Some(status.clone());
        view.builds[0] = Some(Built::Preparing);
        view.accept_launch(view.request + 1, 0, &status, Err("old launch".into()));
        assert_eq!(view.builds[0], Some(Built::Preparing));
        view.accept_launch(
            view.request,
            0,
            &PathBuf::from("previous-launch"),
            Err("stale path".into()),
        );
        assert_eq!(view.builds[0], Some(Built::Preparing));
        view.accept_launch(
            view.request,
            0,
            &status,
            Err("recorded PTY spawn failure".into()),
        );
        assert_eq!(view.builds[0], Some(Built::Unavailable));
        assert!(
            view.note
                .as_ref()
                .ok_or("expected Some")?
                .0
                .contains("spawn failure")
        );
        // Retry owns a different status path; no receipt from the old run wins.
        view.statuses[0] = Some(PathBuf::from("fresh-owned-receipt"));
        view.builds[0] = Some(Built::Preparing);
        view.accept_builds(view.request, vec![(0, status, Built::Passed)]);
        assert_eq!(view.builds[0], Some(Built::Preparing));
        Ok(())
    }
    #[test]
    fn pure_bakeoff_closed_or_reloaded_comparisons_reject_old_build_statuses() {
        let attempts = vec![
            Attempt {
                name: "a".into(),
                rev: "a".into(),
                worktree: None,
            },
            Attempt {
                name: "b".into(),
                rev: "b".into(),
                worktree: None,
            },
        ];
        let old = Bakeoff::new(attempts.clone(), String::new());
        let mut current = Bakeoff::new(attempts, String::new());
        assert_ne!(old.request, current.request);
        let path = PathBuf::from("new-owned-status");
        current.builds[0] = Some(Built::Running);
        current.statuses[0] = Some(path.clone());
        current.accept_builds(old.request, vec![(0, path.clone(), Built::Passed)]);
        assert_eq!(current.builds[0], Some(Built::Running));
        current.accept_builds(
            current.request,
            vec![(0, PathBuf::from("old-owned-status"), Built::Passed)],
        );
        assert_eq!(current.builds[0], Some(Built::Running));
        current.accept_builds(current.request, vec![(0, path, Built::Passed)]);
        assert_eq!(current.builds[0], Some(Built::Passed));
    }
}

impl Side {
    fn display_kind(&self) -> &'static str {
        let id = match self.kind() {
            "unchanged" => "repo-workflows-bakeoff-kind-unchanged",
            "deleted" => "repo-workflows-bakeoff-kind-deleted",
            "added" => "repo-workflows-bakeoff-kind-added",
            "changed" => "repo-workflows-bakeoff-kind-changed",
            other => return other,
        };
        crate::i18n::text(id)
    }
}
#[cfg(test)]
mod fluent_kind_tests {
    use super::*;
    #[test]
    fn fluent_partition_bakeoff_actual_kind_label_keeps_raw_classification() {
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            for (patch, raw, german) in [
                ("", "unchanged", "unverändert"),
                (
                    "diff --git a/x b/x\ndeleted file mode 100644",
                    "deleted",
                    "gelöscht",
                ),
                (
                    "diff --git a/x b/x\nnew file mode 100644",
                    "added",
                    "hinzugefügt",
                ),
                ("@@ -1 +1 @@\n-before\n+after", "changed", "geändert"),
            ] {
                let side = Side {
                    patch: patch.into(),
                    hunks: Vec::new(),
                };
                assert_eq!(side.kind(), raw);
                assert_eq!(
                    side.display_kind(),
                    if language == "de" { german } else { raw }
                );
                assert_eq!(side.kind() != "unchanged", !patch.is_empty());
                assert_eq!(side.patch, patch);
            }
        }
        crate::i18n::set_language("en-US");
    }
}
