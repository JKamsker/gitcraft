//! A run on a merge put aside (see [`crate::resolver::park`]): the parked merges of the
//! worktree read with the rest of the tab, the agent's work kept with the merge when it is done
//! (its files and its run, in `.git/gitcraft/resolver/<id>/`, so that it outlasts gitcraft),
//! the user told, and the merge brought back to review and apply that work. A parked merge
//! whose run isn't in this tab (gitcraft was closed meanwhile) gets its run back from what was
//! kept, or waits to be brought back.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{Phase, Run, done_files};
use crate::repo::RepoTab;
use crate::resolver::agent::Engine;
use crate::resolver::park::{self, Parked};
use crate::resolver::report::{PlannedStep, Progress, Report, StepStatus};
use crate::resolver::workspace::{FileResult, Workspace};

/// A run as kept with its merge, to show it again in another session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct RunRecord {
    engine: String,
    files: Vec<String>,
    /// When it started and ended, in seconds since 1970.
    started: i64,
    ended: i64,
    plan: Vec<PlannedStep>,
    /// Each step's id, how it ended and its note.
    steps: Vec<(String, Option<StepStatus>, Option<String>)>,
    finished: Option<(bool, String)>,
    error: Option<String>,
    last_words: Option<String>,
    cost_usd: Option<f64>,
}

impl RunRecord {
    fn of(run: &Run) -> RunRecord {
        let now = crate::git::now();
        let ago = |at: Instant| i64::try_from(at.elapsed().as_secs()).unwrap_or(0);
        RunRecord {
            engine: run.engine.label().to_owned(),
            files: run.files.clone(),
            started: now - ago(run.started),
            ended: now - run.ended.map_or(0, ago),
            plan: run
                .progress
                .steps
                .iter()
                .map(|s| PlannedStep {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    file: s.path.clone(),
                })
                .collect(),
            steps: run
                .progress
                .steps
                .iter()
                .map(|s| (s.id.clone(), Some(s.status), s.note.clone()))
                .collect(),
            finished: run
                .progress
                .finished
                .as_ref()
                .map(|f| (f.ok, f.summary.clone())),
            error: run.error.clone(),
            last_words: run.last_words.clone(),
            cost_usd: run.cost_usd,
        }
    }

    /// The progress it records, as the card shows it.
    fn progress(&self) -> Progress {
        let now = Instant::now();
        let mut p = Progress::default();
        if !self.plan.is_empty() {
            p.apply(
                Report::Plan {
                    steps: self.plan.clone(),
                    estimate_minutes: None,
                },
                now,
            );
        }
        for (id, status, note) in &self.steps {
            if let Some(status) = status.filter(|s| *s != StepStatus::Pending) {
                p.apply(
                    Report::Step {
                        id: id.clone(),
                        status,
                        note: note.clone(),
                    },
                    now,
                );
            }
        }
        if let Some((ok, summary)) = &self.finished {
            p.apply(
                Report::Finish {
                    ok: *ok,
                    summary: summary.clone(),
                },
                now,
            );
        }
        p
    }
}

/// Where the agent's work on the merge `id` is kept.
fn kept_dir(repo: &Path, id: &str) -> Result<PathBuf, String> {
    crate::agent::repo_file(repo, &format!("resolver/{id}"))
}

/// Keeps `ws`'s work and the run `record` in `dir`: the workspace of what is kept, and how
/// each file stands in it.
fn keep(
    ws: &Workspace,
    dir: &Path,
    record: &RunRecord,
    done: &[String],
) -> Result<(Workspace, Vec<(String, FileResult)>), String> {
    let kept = ws.keep(dir)?;
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("run.json"), text)
        .map_err(|e| format!("couldn't keep the agent's run: {e}"))?;
    let results = kept.check(None, done);
    Ok((kept, results))
}

/// Save diagnostics without reading/copying files that may still be written by an agent.
pub(super) fn preserve_uncertain(run: &Run, ws: &Workspace) {
    if let Ok(dir) = kept_dir(ws.dir(), &run.id) {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(record) = serde_json::to_vec_pretty(&RunRecord::of(run)) {
            let _ = std::fs::write(dir.join("uncertain-run.json"), record);
        }
        let _ = std::fs::write(
            dir.join("cleanup-unconfirmed"),
            ws.dir().to_string_lossy().as_bytes(),
        );
    }
}

/// The run kept for the parked merge `parked`, with its work, if one was.
fn kept_run(repo: &Path, parked: &Parked) -> Option<Run> {
    let dir = kept_dir(repo, &parked.id).ok()?;
    if dir.join("cleanup-unconfirmed").exists() {
        return None;
    }
    let text = std::fs::read_to_string(dir.join("run.json")).ok()?;
    let record: RunRecord = serde_json::from_str(&text).ok()?;
    let ws = Workspace::load(&dir).ok()?;
    let progress = record.progress();
    let results = ws.check(None, &done_files(&progress));
    let picked: BTreeSet<String> = results
        .iter()
        .filter(|(_, r)| r.applies())
        .map(|(p, _)| p.clone())
        .collect();
    let now = crate::git::now();
    let back = |at: i64| {
        let ago = Duration::from_secs(u64::try_from(now - at).unwrap_or(0));
        Instant::now().checked_sub(ago).unwrap_or_else(Instant::now)
    };
    Some(Run {
        id: parked.id.clone(),
        engine: if record.engine == Engine::Codex.label() {
            Engine::Codex
        } else {
            Engine::Claude
        },
        files: record.files,
        started: back(record.started),
        ended: Some(back(record.ended)),
        phase: Phase::Ready,
        progress,
        workspace: Some(Arc::new(ws)),
        agent: None,
        mcp_file: None,
        stopping: false,
        results,
        picked,
        applied: 0,
        unstaged: false,
        error: record.error,
        last_words: record.last_words,
        cost_usd: record.cost_usd,
        review: None,
        parked: Some(parked.clone()),
    })
}

/// What the tab says it is doing while a merge is brought back, or dropped.
const BRINGING_BACK: &str = "Bring the merge back";
const DROPPING: &str = "Drop the merge put aside";

/// Drops `ws` away from the UI thread: dropping a scratch worktree runs git.
fn let_go(ws: Option<Arc<Workspace>>) {
    if let Some(ws) = ws {
        std::thread::spawn(move || {
            if let Ok(ws) = Arc::try_unwrap(ws) {
                let _ = ws.remove_patiently();
            }
        });
    }
}

impl RepoTab {
    /// Reads the merges put aside from this worktree (with the rest of the tab, on a reload).
    pub(in crate::repo) fn read_parked(&mut self, ctx: &egui::Context) {
        if crate::remote::is_remote(&self.path) {
            return;
        }
        // One at a time: the one under way is read again once it is in.
        if self.resolver.reading_parked {
            self.resolver.read_parked_again = true;
            return;
        }
        self.resolver.reading_parked = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || park::list(&repo).unwrap_or_default(),
            |tab, parked, ctx| tab.parked_read(parked, ctx),
        );
    }

    fn parked_read(&mut self, parked: Vec<Parked>, ctx: &egui::Context) {
        self.resolver.reading_parked = false;
        // Read before a change made here (or asked for since): read again, this one dropped.
        if std::mem::take(&mut self.resolver.read_parked_again) {
            self.read_parked(ctx);
            return;
        }
        if parked != self.resolver.parked {
            self.resolver.parked = parked;
            self.rebuild(ctx);
        }
        // One of them without its run here: gitcraft was closed meanwhile. Its run comes back
        // from what was kept (else it waits, in the graph, to be brought back).
        if self.resolver.run.is_some() {
            return;
        }
        let parked = self.resolver.parked.clone();
        if parked.is_empty() {
            return;
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            // The first of them whose agent's work was kept.
            move || parked.iter().find_map(|p| kept_run(&repo, p)),
            |tab, run, _| tab.kept_run_read(run),
        );
    }

    /// The run of a parked merge, read back from what was kept: the tab's, unless another run
    /// came meanwhile or the merge isn't parked any more (brought back, which reads its run
    /// itself, or dropped).
    fn kept_run_read(&mut self, run: Option<Run>) {
        let Some(run) = run else {
            return;
        };
        let parked = self.resolver.parked.iter().any(|p| p.id == run.id);
        if self.resolver.run.is_none() && parked {
            self.resolver.run = Some(run);
            self.resolver.folded = true;
        }
    }

    /// Unfolds the agent's card (a parked merge's row clicked).
    pub(in crate::repo) fn resolver_show(&mut self) {
        self.resolver.folded = false;
    }

    /// The agent of a parked merge ended: its work is kept with the merge, the scratch worktree
    /// goes, and the user is told it is there to bring back.
    pub(super) fn keep_parked_work(&mut self, ctx: &egui::Context) {
        let Some(run) = self.resolver.run.as_mut() else {
            return;
        };
        let Some(ws) = run.workspace.clone() else {
            run.phase = Phase::Failed;
            return;
        };
        run.phase = Phase::Checking;
        let record = RunRecord::of(run);
        let done = done_files(&run.progress);
        let (repo, id) = (self.path.clone(), run.id.clone());
        self.then(
            ctx,
            move || {
                let dir = kept_dir(&repo, &id)?;
                keep(&ws, &dir, &record, &done)
            },
            |tab, kept, _| tab.parked_work_kept(kept),
        );
    }

    fn parked_work_kept(&mut self, kept: Result<(Workspace, Vec<(String, FileResult)>), String>) {
        let name = self.name.clone();
        let Some(run) = self.resolver.run.as_mut() else {
            return;
        };
        let (ws, results) = match kept {
            Ok(kept) => kept,
            Err(e) => {
                run.phase = Phase::Failed;
                run.error = Some(format!("The agent's work couldn't be kept: {e}"));
                return;
            }
        };
        let_go(run.workspace.replace(Arc::new(ws)));
        if let Some(file) = run.mcp_file.take() {
            let _ = std::fs::remove_file(file);
        }
        run.picked = results
            .iter()
            .filter(|(_, r)| r.applies())
            .map(|(p, _)| p.clone())
            .collect();
        let ready = run.picked.len();
        run.results = results;
        run.phase = Phase::Ready;
        let what = run.parked.as_ref().map(Parked::what).unwrap_or_default();
        let text = match ready {
            0 => format!("{name} · The agent is done with the {what}: nothing to apply"),
            1 => format!("{name} · The agent resolved 1 file of the {what}: bring it back"),
            n => format!("{name} · The agent resolved {n} files of the {what}: bring it back"),
        };
        self.finished
            .push(crate::repo::notice::Notice::plain((text, ready == 0)));
    }

    /// Brings the parked merge `id` back into the worktree, then shows the agent's work on it.
    pub(super) fn bring_back(&mut self, id: &str, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Wait for the running operation to finish".to_owned(),
                true,
            )));
            return;
        }
        let Some(parked) = self.resolver.parked.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        // Its agent may go on working (in its own worktree): its work is then checked against
        // the merge back in the worktree, as in place. Only while the merge is being put aside,
        // or the agent's work kept with it, does it wait a moment.
        let settling =
            self.resolver.run.as_ref().is_some_and(|r| {
                r.id == id && matches!(r.phase, Phase::Preparing | Phase::Checking)
            });
        if settling {
            self.status = Some(crate::repo::notice::Notice::plain((
                "A moment: try again in a second".to_owned(),
                true,
            )));
            return;
        }
        // Switching cards is safe only after the other parked run is durably kept.
        if self.resolver.run.as_ref().is_some_and(|r| {
            r.id != id
                && (r.parked.is_none()
                    || !matches!(r.phase, Phase::Ready | Phase::Failed)
                    || !r.workspace.as_ref().is_some_and(|w| w.is_kept()))
        }) {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Finish or dismiss the current agent run first".into(),
                true,
            )));
            return;
        }
        let repo = self.path.clone();
        self.busy_op = Some(BRINGING_BACK.to_owned());
        self.then(
            ctx,
            move || park::bring_back(&repo, &parked).map(|()| parked),
            |tab, back, ctx| tab.brought_back(back, ctx),
        );
    }

    fn brought_back(&mut self, back: Result<Parked, String>, ctx: &egui::Context) {
        if self.busy_op.as_deref() == Some(BRINGING_BACK) {
            self.busy_op = None;
        }
        let parked = match back {
            Ok(p) => p,
            Err(e) => {
                self.status = Some(crate::repo::notice::Notice::plain((
                    format!("Not brought back: {e}"),
                    true,
                )));
                return;
            }
        };
        self.resolver.parked.retain(|p| p.id != parked.id);
        self.resolver.parked_changed();
        self.status = Some(crate::repo::notice::Notice::plain((
            format!("The {} is back, as it was", parked.what()),
            false,
        )));
        // Its changes shown, as they were: conflicted and staged files.
        self.resolver.reveal = Some((super::Reveal::Changes, Instant::now()));
        self.reload(ctx);
        if self.resolver_owns(&parked.id) {
            self.show_brought_back(ctx);
            return;
        }
        let repo = self.path.clone();
        let id = parked.id.clone();
        self.then(
            ctx,
            move || kept_run(&repo, &parked),
            move |tab, run, ctx| {
                tab.select_restored_run(&id, run, ctx);
            },
        );
    }

    fn select_restored_run(&mut self, id: &str, run: Option<Run>, ctx: &egui::Context) {
        let Some(run) = run.filter(|r| r.id == id) else {
            return;
        };
        let replaceable = self.resolver.run.as_ref().is_none_or(|r| {
            r.id == id
                || (r.parked.is_some()
                    && matches!(r.phase, Phase::Ready | Phase::Failed)
                    && r.workspace.as_ref().is_some_and(|w| w.is_kept()))
        });
        if !replaceable {
            return;
        }
        // The previous run's workspace and run.json remain with its parked merge.
        // In particular, do not call let_go, which deletes the durable workspace.
        self.resolver.run = Some(run);
        self.show_brought_back(ctx);
    }

    /// The run of the merge just brought back: its card in sight, as for a run in place, and
    /// once it is done what it did, to review and apply now that the merge is here.
    fn show_brought_back(&mut self, ctx: &egui::Context) {
        let Some(run) = self.resolver.run.as_mut() else {
            return;
        };
        run.parked = None;
        self.resolver.folded = false;
        if run.phase == Phase::Ready && !run.results.is_empty() {
            self.resolver_act(super::ResolverAct::Review(0), ctx);
        }
    }
}

impl RepoTab {
    /// Drops the merge put aside as `id` without bringing it back: its files go on a branch of
    /// their own, so nothing of it is lost; its run (and the agent's work) goes.
    pub(super) fn drop_parked(&mut self, id: &str, ctx: &egui::Context) {
        // Never while it is brought back (or anything else runs on the worktree).
        if self.busy_op.is_some() {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Wait for the running operation to finish".to_owned(),
                true,
            )));
            return;
        }
        let Some(parked) = self.resolver.parked.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        let working = self.resolver.run.as_ref().is_some_and(|r| {
            r.id == id && matches!(r.phase, Phase::Preparing | Phase::Working | Phase::Checking)
        });
        if working {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Stop the agent first".to_owned(),
                true,
            )));
            return;
        }
        let repo = self.path.clone();
        self.busy_op = Some(DROPPING.to_owned());
        self.then(
            ctx,
            move || park::drop_to_branch(&repo, &parked).map(|b| (parked, b)),
            |tab, dropped, ctx| tab.parked_dropped(dropped, ctx),
        );
    }

    fn parked_dropped(&mut self, dropped: Result<(Parked, String), String>, ctx: &egui::Context) {
        if self.busy_op.as_deref() == Some(DROPPING) {
            self.busy_op = None;
        }
        let (parked, branch) = match dropped {
            Ok(dropped) => dropped,
            Err(e) => {
                self.status = Some(crate::repo::notice::Notice::plain((
                    format!("Not dropped: {e}"),
                    true,
                )));
                return;
            }
        };
        self.resolver.parked.retain(|p| p.id != parked.id);
        self.resolver.parked_changed();
        if let Some(run) = self.resolver.run.as_mut().filter(|r| r.id == parked.id) {
            run.parked = None;
        }
        if self.resolver_owns(&parked.id) {
            self.resolver_act(super::ResolverAct::Dismiss, ctx);
        }
        self.status = Some(crate::repo::notice::Notice::plain((
            format!(
                "The {} was dropped: its files are on {branch}",
                parked.what()
            ),
            false,
        )));
        self.reload(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_of_the_parked_list_from_before_a_change_here_is_dropped() {
        let ctx = egui::Context::default();
        let dir = std::env::temp_dir().join("gitcraft-parked-stale-read");
        let mut tab = RepoTab::new_deferred(dir, &ctx);
        let parked = Parked {
            id: "p1".into(),
            ..Parked::default()
        };
        // Put aside while the list was being read: that read doesn't have it.
        tab.resolver.reading_parked = true;
        tab.resolver.parked.push(parked.clone());
        tab.resolver.parked_changed();
        tab.parked_read(Vec::new(), &ctx);
        assert_eq!(tab.resolver.parked, vec![parked]);
        assert!(tab.resolver.reading_parked, "read again");
        assert!(!tab.resolver.read_parked_again);
    }

    #[test]
    fn a_run_read_back_for_a_merge_no_longer_parked_is_left_out() {
        let ctx = egui::Context::default();
        let dir = std::env::temp_dir().join("gitcraft-parked-gone-run");
        let mut tab = RepoTab::new_deferred(dir, &ctx);
        let mut run = super::super::look_tests::run(Phase::Ready, Instant::now());
        run.parked = Some(Parked {
            id: run.id.clone(),
            ..Parked::default()
        });
        // Brought back (or dropped) while its run was read.
        tab.kept_run_read(Some(run));
        assert!(tab.resolver.run.is_none());
    }

    #[test]
    fn a_run_in_a_worktree_parked_behind_the_one_shown_is_found() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(std::env::temp_dir().join("gitcraft-owner-a"), &ctx);
        let mut behind = RepoTab::new_deferred(std::env::temp_dir().join("gitcraft-owner-b"), &ctx);
        behind.resolver.run = Some(super::super::look_tests::run(Phase::Ready, Instant::now()));
        tab.parked.push(Box::new(behind));
        assert!(!tab.resolver_owns("look"));
        assert!(
            tab.resolver_owner("look")
                .is_some_and(|t| t.path.ends_with("gitcraft-owner-b"))
        );
        assert!(tab.resolver_owner("other").is_none());
    }

    #[test]
    fn a_run_record_gives_its_progress_back() -> Result<(), String> {
        let record = RunRecord {
            engine: "Codex".into(),
            plan: vec![PlannedStep {
                id: "a.rs".into(),
                title: "Resolve a.rs".into(),
                file: Some("a.rs".into()),
            }],
            steps: vec![(
                "a.rs".into(),
                Some(StepStatus::Done),
                Some("kept both".into()),
            )],
            finished: Some((true, "all done".into())),
            ..RunRecord::default()
        };
        let text = serde_json::to_string(&record).map_err(|e| e.to_string())?;
        let back: RunRecord = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let p = back.progress();
        assert_eq!(p.steps.len(), 1);
        assert_eq!(p.steps[0].status, StepStatus::Done);
        assert_eq!(p.steps[0].note.as_deref(), Some("kept both"));
        assert!(
            p.finished
                .as_ref()
                .is_some_and(|f| f.ok && f.summary == "all done")
        );
        Ok(())
    }
}

#[cfg(test)]
mod selection_regression_tests {
    use super::*;
    #[test]
    fn done_step_file_overrides_an_id_that_names_another_file() {
        let now = Instant::now();
        let mut progress = Progress::default();
        progress.apply(
            Report::Plan {
                steps: vec![
                    PlannedStep {
                        id: "untouched.txt".into(),
                        title: "Resolve actual".into(),
                        file: Some("actual.txt".into()),
                    },
                    PlannedStep {
                        id: "fallback.txt".into(),
                        title: "Fallback".into(),
                        file: None,
                    },
                ],
                estimate_minutes: None,
            },
            now,
        );
        for id in ["untouched.txt", "fallback.txt"] {
            progress.apply(
                Report::Step {
                    id: id.into(),
                    status: StepStatus::Done,
                    note: None,
                },
                now,
            );
        }
        assert_eq!(done_files(&progress), vec!["actual.txt", "fallback.txt"]);
    }
}

#[cfg(test)]
mod restored_run_tests {
    use super::*;
    #[test]
    fn selecting_restored_id_preserves_the_other_parked_run() -> Result<(), String> {
        let repo = crate::git::fixture::Repo::new("resolver-switch-parked")?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
        let directory = repo.path().join("saved-agent");
        std::fs::create_dir_all(directory.join("resolved")).map_err(|e| e.to_string())?;
        std::fs::write(
            directory.join("workspace.json"),
            r#"{"files":[],"sides":{}}"#,
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(directory.join("run.json"), "original record").map_err(|e| e.to_string())?;
        let mut previous = super::super::look_tests::run(Phase::Ready, Instant::now());
        previous.id = "previous".into();
        previous.parked = Some(Parked {
            id: "previous".into(),
            ..Default::default()
        });
        previous.workspace = Some(Arc::new(Workspace::load(&directory)?));
        tab.resolver.run = Some(previous);
        let restored = || {
            let mut run = super::super::look_tests::run(Phase::Ready, Instant::now());
            run.id = "restored".into();
            run.results.clear();
            run
        };
        tab.select_restored_run("wrong-id", Some(restored()), &ctx);
        assert_eq!(
            tab.resolver.run.as_ref().map(|r| r.id.as_str()),
            Some("previous")
        );
        tab.select_restored_run("restored", None, &ctx);
        assert_eq!(
            tab.resolver.run.as_ref().map(|r| r.id.as_str()),
            Some("previous")
        );
        tab.select_restored_run("restored", Some(restored()), &ctx);
        assert_eq!(
            tab.resolver.run.as_ref().map(|r| r.id.as_str()),
            Some("restored")
        );
        assert!(Workspace::load(&directory).is_ok());
        assert_eq!(
            std::fs::read_to_string(directory.join("run.json")).map_err(|e| e.to_string())?,
            "original record"
        );
        Ok(())
    }
}
