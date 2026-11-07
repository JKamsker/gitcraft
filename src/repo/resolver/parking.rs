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
                    file: s.file.clone(),
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

/// The run kept for the parked merge `parked`, with its work, if one was.
fn kept_run(repo: &Path, parked: &Parked) -> Option<Run> {
    let dir = kept_dir(repo, &parked.id).ok()?;
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
        error: record.error,
        last_words: record.last_words,
        cost_usd: record.cost_usd,
        review: None,
        parked: Some(parked.clone()),
    })
}

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
        if crate::remote::is_remote(&self.path) || self.resolver.reading_parked {
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
        if parked != self.resolver.parked {
            self.resolver.parked = parked;
            self.rebuild(ctx);
        }
        // One of them without its run here: gitcraft was closed meanwhile. Its run comes back
        // from what was kept (else it waits, in the graph, to be brought back).
        if self.resolver.run.is_some() {
            return;
        }
        let Some(first) = self.resolver.parked.first().cloned() else {
            return;
        };
        let repo = self.path.clone();
        self.then(
            ctx,
            move || kept_run(&repo, &first),
            |tab, run, _| {
                if tab.resolver.run.is_none() {
                    tab.resolver.run = run;
                    tab.resolver.folded = true;
                }
            },
        );
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
        self.finished.push((text, ready == 0));
    }

    /// Brings the parked merge `id` back into the worktree, then shows the agent's work on it.
    pub(super) fn bring_back(&mut self, id: &str, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some(("Wait for the running operation to finish".to_owned(), true));
            return;
        }
        let Some(parked) = self.resolver.parked.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        // Its agent still at work: stopped first, so what it did is kept and shown.
        let at_work = self.resolver.run.as_ref().is_some_and(|r| {
            r.id == id && matches!(r.phase, Phase::Preparing | Phase::Working | Phase::Checking)
        });
        if at_work {
            self.status = Some((
                "The agent is still at it: stop it first, or wait for it".to_owned(),
                true,
            ));
            return;
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            move || park::bring_back(&repo, &parked).map(|()| parked),
            |tab, back, ctx| tab.brought_back(back, ctx),
        );
    }

    fn brought_back(&mut self, back: Result<Parked, String>, ctx: &egui::Context) {
        let parked = match back {
            Ok(p) => p,
            Err(e) => {
                self.status = Some((format!("Not brought back: {e}"), true));
                return;
            }
        };
        self.resolver.parked.retain(|p| p.id != parked.id);
        self.status = Some((format!("The {} is back, as it was", parked.what()), false));
        self.reload(ctx);
        let Some(run) = self.resolver.run.as_mut().filter(|r| r.id == parked.id) else {
            return;
        };
        run.parked = None;
        if run.phase == Phase::Ready && !run.results.is_empty() {
            // What it did, to review and apply now that the merge is here.
            self.resolver.folded = false;
            self.resolver_act(super::ResolverAct::Review(0), ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
