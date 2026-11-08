//! What the card looks like in each of its states, drawn over a real tab with a merge in
//! progress, and saved as pictures to look at. Not run by default (it needs a GPU adapter):
//!
//! ```text
//! GITCRAFT_LOOK_DIR=<folder> cargo test --lib -- --ignored repo::resolver::look_tests
//! ```

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;

use super::{Phase, Run};
use crate::repo::RepoTab;
use crate::resolver::agent::{Activity, Engine};
use crate::resolver::process::Running;
use crate::resolver::report::{PlannedStep, Progress, Report, StepStatus};
use crate::resolver::workspace::FileResult;
use crate::settings::Settings;

const FILES: [&str; 4] = [
    "src/repo/operations.rs",
    "src/repo/layout.rs",
    "src/git/command.rs",
    "Cargo.lock",
];

/// A plan of a step per file and a check, `done` of them over, the next one running.
fn progress(done: usize, now: Instant) -> Progress {
    let mut p = Progress::default();
    let mut steps: Vec<PlannedStep> = FILES
        .iter()
        .map(|f| PlannedStep {
            id: (*f).to_owned(),
            title: format!("Resolve {}", f.rsplit('/').next().unwrap_or(f)),
            file: Some((*f).to_owned()),
        })
        .collect();
    steps.push(PlannedStep {
        id: "check".into(),
        title: "Check that it builds".into(),
        file: None,
    });
    let start = now - Duration::from_secs(150);
    p.apply(
        Report::Plan {
            steps,
            estimate_minutes: Some(6),
        },
        start,
    );
    let notes = [
        "Kept both: the new busy guard from dev and the worktree root from this branch.",
        "Took dev's panel order; this branch only renamed a variable there.",
        "Merged both new helpers; they don't overlap.",
        "Regenerated from both sides' Cargo.toml.",
    ];
    for (i, f) in FILES.iter().enumerate().take(done) {
        let at = start + Duration::from_secs(40 * i as u64);
        p.apply(step(f, StepStatus::Running, None), at);
        p.apply(
            step(f, StepStatus::Done, Some(notes[i])),
            at + Duration::from_secs(35),
        );
    }
    if let Some(next) = FILES.get(done) {
        p.apply(
            step(next, StepStatus::Running, None),
            now - Duration::from_secs(12),
        );
    }
    p
}

fn step(id: &str, status: StepStatus, note: Option<&str>) -> Report {
    Report::Step {
        id: id.to_owned(),
        status,
        note: note.map(str::to_owned),
    }
}

fn run(phase: Phase, now: Instant) -> Run {
    let done = match phase {
        Phase::Working => 2,
        _ => 4,
    };
    let mut progress = progress(done, now);
    let mut results = Vec::new();
    if phase == Phase::Ready {
        progress.apply(
            step(
                "check",
                StepStatus::Failed,
                Some("cargo isn't allowed here"),
            ),
            now,
        );
        progress.apply(
            Report::Finish {
                ok: true,
                summary: "All four files resolved. Where both sides changed run_ops, I kept dev's \
                          busy guard and this branch's worktree root. Not built: check it compiles."
                    .into(),
            },
            now,
        );
        results = vec![
            (FILES[0].to_owned(), FileResult::Resolved),
            (FILES[1].to_owned(), FileResult::Resolved),
            (FILES[2].to_owned(), FileResult::ChangedMeanwhile),
            (FILES[3].to_owned(), FileResult::Resolved),
        ];
    }
    let picked: BTreeSet<String> = results
        .iter()
        .filter(|(_, r)| r.applies())
        .map(|(p, _)| p.clone())
        .collect();
    let working = phase == Phase::Working;
    Run {
        id: "look".into(),
        engine: Engine::Claude,
        files: FILES.iter().map(|f| (*f).to_owned()).collect(),
        started: now - Duration::from_secs(if working { 150 } else { 214 }),
        ended: (!working).then_some(now),
        phase,
        progress,
        workspace: None,
        agent: working.then(|| {
            Running::fake(vec![
                Activity::Said("Both sides changed run_ops; reading dev's version.".into()),
                Activity::Tool("Running git log -5 MERGE_HEAD -- src/git/command.rs".into()),
                Activity::Tool("Asking gitcraft: blame".into()),
                Activity::Tool("Editing src/git/command.rs".into()),
            ])
        }),
        mcp_file: None,
        stopping: false,
        results,
        picked,
        applied: 0,
        error: None,
        last_words: None,
        cost_usd: (!working).then_some(0.42),
        review: None,
        parked: None,
    }
}

/// The first file under review: two conflicts, one side taken, both sides kept in the other.
fn under_review(as_diff: bool) -> super::ReviewState {
    let before = "impl RepoTab {
    pub(super) fn run_ops(&mut self, label: &str, ops: Vec<Vec<String>>, ctx: &Context) {
<<<<<<< HEAD
        if self.busy_op.is_some() {
            self.status = Some((\"Wait for the running operation\".into(), true));
            return;
        }
=======
        let ops = self.worktree_root.rewrite(ops);
>>>>>>> dev
        self.busy_op = Some(label.to_owned());
<<<<<<< HEAD
        self.op_started = Some(Instant::now());
=======
        self.started_at = Instant::now();
>>>>>>> dev
        self.spawn(ops, ctx);
    }
}
";
    let after = "impl RepoTab {
    pub(super) fn run_ops(&mut self, label: &str, ops: Vec<Vec<String>>, ctx: &Context) {
        if self.busy_op.is_some() {
            self.status = Some((\"Wait for the running operation\".into(), true));
            return;
        }
        let ops = self.worktree_root.rewrite(ops);
        self.busy_op = Some(label.to_owned());
        self.op_started = Some(Instant::now());
        self.spawn(ops, ctx);
    }
}
";
    let files = FILES
        .iter()
        .map(|f| {
            (
                f.to_string(),
                super::review::review(Some(before), Some(after)),
            )
        })
        .collect();
    super::ReviewState {
        files: Some(files),
        selected: 0,
        as_diff,
    }
}

fn repo() -> Result<crate::git::fixture::Repo, String> {
    let repo = crate::git::fixture::Repo::new("resolver look")?;
    repo.commit("base", &[("a.txt", Some("base\n"))])?;
    repo.git(&["checkout", "-q", "-b", "dev"])?;
    repo.commit("dev", &[("a.txt", Some("dev\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("main\n"))])?;
    assert!(repo.git(&["merge", "-q", "dev"]).is_err());
    Ok(repo)
}

type H<'a> = Harness<'a, Option<RepoTab>>;

fn harness(path: PathBuf, dark: bool) -> H<'static> {
    Harness::builder()
        .with_size([1280.0, 800.0])
        .wgpu()
        .build_ui_state(
            move |ui, tab: &mut Option<RepoTab>| {
                let tab = tab.get_or_insert_with(|| {
                    let settings = Settings {
                        avatars: false,
                        auto_fetch_minutes: 0,
                        ..Settings::default()
                    };
                    settings.set(ui.ctx());
                    crate::theme::install_fonts(ui.ctx());
                    let theme = if dark {
                        crate::theme::ThemeId::default()
                    } else {
                        crate::theme::ThemeId("Light".to_owned())
                    };
                    crate::theme::apply(ui.ctx(), &theme, &Default::default());
                    RepoTab::new(path.clone(), ui.ctx())
                });
                tab.ui(ui);
                crate::a11y::end_frame(ui.ctx());
            },
            None,
        )
}

fn settle(h: &mut H<'_>) {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        h.step();
        if h.state()
            .as_ref()
            .is_some_and(|t| !t.opening() && t.settled())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for _ in 0..5 {
        h.step();
    }
}

#[test]
#[ignore = "draws pictures to look at: see the module's documentation"]
fn the_card_in_each_state() -> Result<(), String> {
    let out =
        PathBuf::from(std::env::var("GITCRAFT_LOOK_DIR").map_err(|_| "set GITCRAFT_LOOK_DIR")?);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let repo = repo()?;
    for dark in [true, false] {
        let mut h = harness(repo.path().to_owned(), dark);
        settle(&mut h);
        let head = repo.git(&["rev-parse", "HEAD"])?.trim().to_owned();
        // `pick`: the parked row selected (Some(true)), or a commit's (Some(false)).
        for (name, phase, folded, review, put_aside, pick) in [
            ("working", Phase::Working, false, None, false, None),
            ("ready", Phase::Ready, false, None, false, None),
            ("folded", Phase::Working, true, None, false, None),
            ("review", Phase::Ready, false, Some(false), false, None),
            ("review-diff", Phase::Ready, false, Some(true), false, None),
            (
                "parked-working",
                Phase::Working,
                false,
                None,
                true,
                Some(true),
            ),
            ("parked-ready", Phase::Ready, false, None, true, Some(true)),
            (
                "parked-elsewhere",
                Phase::Working,
                false,
                None,
                true,
                Some(false),
            ),
        ] {
            let ctx = h.ctx.clone();
            if let Some(tab) = h.state_mut().as_mut() {
                let mut run = run(phase, Instant::now());
                run.review = review.map(under_review);
                let parked = put_aside.then(|| crate::resolver::park::Parked {
                    id: "look".into(),
                    operation: "merge".into(),
                    branch: Some("main".into()),
                    head: head.clone(),
                    theirs_name: Some("dev".into()),
                    // A real commit, for its files to show.
                    snapshot: head.clone(),
                    files: vec!["a.txt".into()],
                    at: crate::git::now() - 200,
                    ..Default::default()
                });
                run.parked = parked.clone();
                tab.resolver.parked = parked.into_iter().collect();
                tab.resolver.run = Some(run);
                tab.resolver.folded = folded;
                tab.rebuild_rows();
                let parked_row = tab
                    .commits
                    .iter()
                    .position(|c| c.hash.starts_with("\u{0}parked:"));
                let other = tab.commits.iter().position(|c| c.hash == head);
                match pick {
                    Some(true) => parked_row.into_iter().for_each(|i| tab.select(i, &ctx)),
                    Some(false) => other.into_iter().for_each(|i| tab.select(i, &ctx)),
                    None => {}
                }
            }
            // The files read in the background come in.
            for _ in 0..30 {
                h.step();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let theme = if dark { "dark" } else { "light" };
            let image = h.render()?;
            let file = out.join(format!("resolver-{name}-{theme}.png"));
            image.save(&file).map_err(|e| e.to_string())?;
            println!("{}", file.display());
            if let Some(tab) = h.state_mut().as_mut() {
                // Nothing is cleaned up for a run that made nothing.
                tab.resolver.run = None;
                tab.resolver.parked.clear();
                tab.rebuild_rows();
            }
        }
    }
    Ok(())
}
