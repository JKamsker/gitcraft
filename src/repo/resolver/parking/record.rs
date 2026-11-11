use super::*;
mod persistence;

/// A run as kept with its merge, to show it again in another session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct RunRecord {
    pub(super) engine: String,
    pub(super) files: Vec<String>,
    /// When it started and ended, in seconds since 1970.
    pub(super) started: i64,
    pub(super) ended: i64,
    pub(super) plan: Vec<PlannedStep>,
    /// Each step's id, how it ended and its note.
    pub(super) steps: Vec<(String, Option<StepStatus>, Option<String>)>,
    pub(super) finished: Option<(bool, String)>,
    pub(super) error: Option<String>,
    pub(super) last_words: Option<String>,
    pub(super) cost_usd: Option<f64>,
}

impl RunRecord {
    pub(in crate::repo) fn of(run: &Run) -> RunRecord {
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
    pub(in crate::repo) fn progress(&self) -> Progress {
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
pub(in crate::repo) fn kept_dir(repo: &Path, id: &str) -> Result<PathBuf, String> {
    crate::agent::repo_file(repo, &format!("resolver/{id}"))
}

/// Keeps `ws`'s work and the run `record` in `dir`: the workspace of what is kept, and how
/// each file stands in it.
pub(in crate::repo) fn keep(
    ws: &Workspace,
    dir: &Path,
    record: &RunRecord,
    done: &[String],
) -> Result<(Workspace, Vec<(String, FileResult)>), String> {
    let kept = persistence::keep_record(ws, dir, record)?;
    let results = kept.check(None, done);
    Ok((kept, results))
}

/// Save diagnostics without reading/copying files that may still be written by an agent.
pub(in crate::repo) fn preserve_uncertain(run: &Run, ws: &Workspace) {
    // The process service itself may be unavailable: preserve a record without invoking Git.
    if let Ok(record) = serde_json::to_vec_pretty(&RunRecord::of(run)) {
        let _ = std::fs::write(ws.dir().with_extension("uncertain-run.json"), record);
    }
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
pub(in crate::repo) fn kept_run(repo: &Path, parked: &Parked) -> Option<Run> {
    let dir = kept_dir(repo, &parked.id).ok()?;
    if dir.join("cleanup-unconfirmed").exists() {
        return None;
    }
    let record = persistence::read_record(&dir)?;
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
