use super::*;
use crate::ci::timing::TimedStep;

/// A job that was queued at `ready`, ran from `start` to `end` (seconds).
fn job(name: &str, ready: i64, start: Option<i64>, end: Option<i64>) -> TimedJob {
    TimedJob {
        id: name.len() as u64,
        name: name.to_owned(),
        state: CheckState::Passed,
        created: Some(ready),
        started: start,
        finished: end,
        ..TimedJob::default()
    }
}

fn run(jobs: Vec<TimedJob>) -> TimedRun {
    TimedRun {
        info: RunInfo {
            id: 1,
            name: "CI".to_owned(),
            number: 7,
            sha: "a".repeat(40),
            path: ".github/workflows/ci.yml".to_owned(),
            ..RunInfo::default()
        },
        jobs,
        more: false,
    }
}

const WORKFLOW: &str = "jobs:
  lint: {runs-on: x}
  build: {runs-on: x}
  test: {needs: [lint, build], runs-on: x}
  docs: {runs-on: x}
";

fn jobs() -> Vec<TimedJob> {
    vec![
        job("lint", 0, Some(5), Some(60)),
        job("build", 0, Some(5), Some(300)),
        job("test", 301, Some(310), Some(500)),
        job("docs", 0, Some(5), Some(100)),
    ]
}

#[test]
fn the_critical_path_follows_the_dependency_that_ended_last() {
    let w = workflow::parse_github(WORKFLOW).unwrap_or_default();
    let deps = file_deps(&jobs(), &w, false);
    assert_eq!(deps.edges.len(), 2);
    assert!(deps.edges.iter().all(|d| d.evidence == Evidence::Observed));
    let critical = critical_path(&jobs(), &deps);
    assert_eq!(critical.jobs, [1, 2], "build then test");
    assert_eq!(critical.evidence, Some(Evidence::Observed));
    assert_eq!(critical.span, Some(500));
    assert!(critical.link(1, 2) && !critical.link(0, 2));
}

#[test]
fn a_matched_matrix_job_makes_its_links_inferred() {
    let w = workflow::parse_github(
        "jobs:\n  build:\n    strategy: {matrix: {os: [a, b]}}\n  test: {needs: build}\n",
    )
    .unwrap_or_default();
    let jobs = vec![
        job("build (a)", 0, Some(1), Some(50)),
        job("build (b)", 0, Some(1), Some(80)),
        job("test", 81, Some(82), Some(120)),
    ];
    let deps = file_deps(&jobs, &w, false);
    assert_eq!(deps.edges.len(), 2);
    assert!(deps.edges.iter().all(|d| d.evidence == Evidence::Inferred));
    let critical = critical_path(&jobs, &deps);
    assert_eq!(critical.jobs, [1, 2]);
    assert_eq!(critical.evidence, Some(Evidence::Inferred));
}

#[test]
fn unmatched_jobs_are_said_to_have_unknown_dependencies() {
    let w = workflow::parse_github(WORKFLOW).unwrap_or_default();
    let mut jobs = jobs();
    jobs.push(job("Something else", 0, Some(1), Some(2)));
    let deps = file_deps(&jobs, &w, false);
    assert_eq!(deps.matched[4], None);
    assert!(deps.notes.iter().any(|n| n.contains("1 of 5 jobs")));
}

#[test]
fn without_a_file_the_waits_are_inferred_from_timing() {
    let deps = timing_deps(&jobs());
    // `test` was queued a second after `build` ended; nothing else ended then.
    assert_eq!(
        deps.edges,
        [Dep {
            from: 1,
            to: 2,
            evidence: Evidence::Inferred
        }]
    );
    let critical = critical_path(&jobs(), &deps);
    assert_eq!(critical.jobs, [1, 2]);
    assert_eq!(critical.evidence, Some(Evidence::Inferred));
}

#[test]
fn gitlab_jobs_wait_for_the_stage_before_unless_they_need_otherwise() {
    let text = "stages: [build, test, deploy]
compile: {stage: build, script: [x]}
unit: {stage: test, script: [x]}
lint: {stage: test, needs: [], script: [x]}
pages: {stage: deploy, needs: [compile], script: [x]}
";
    let w = workflow::parse_gitlab(text).unwrap_or_default();
    let mut jobs = vec![
        job("compile", 0, Some(1), Some(60)),
        job("unit", 0, Some(61), Some(200)),
        job("lint", 0, Some(1), Some(30)),
        job("pages", 0, Some(61), Some(90)),
    ];
    for (j, stage) in jobs.iter_mut().zip(["build", "test", "test", "deploy"]) {
        j.stage = stage.to_owned();
    }
    let deps = file_deps(&jobs, &w, true);
    let pairs: Vec<(usize, usize)> = deps.edges.iter().map(|d| (d.from, d.to)).collect();
    assert_eq!(pairs, [(0, 1), (0, 3)]);
    // Without the file: the stages as listed, inferred.
    let staged = stage_deps(&jobs);
    let pairs: Vec<(usize, usize)> = staged.edges.iter().map(|d| (d.from, d.to)).collect();
    assert_eq!(pairs, [(0, 1), (0, 2), (1, 3), (2, 3)]);
    assert!(
        staged
            .edges
            .iter()
            .all(|d| d.evidence == Evidence::Inferred)
    );
}

#[test]
fn jobs_without_an_end_are_never_on_the_path_and_nothing_known_is_no_path() {
    let jobs = vec![
        job("a", 0, Some(1), Some(10)),
        job("b", 0, None, None),
        job("c", 0, Some(1), None),
    ];
    let critical = critical_path(&jobs, &timing_deps(&jobs));
    assert_eq!(critical.jobs, [0]);
    let none = vec![job("a", 0, None, None)];
    assert_eq!(
        critical_path(&none, &timing_deps(&none)),
        Critical::default()
    );
}

#[test]
fn a_file_that_cant_be_read_falls_back_to_timing_and_says_why() {
    let deps = deps_of(&run(jobs()), Forge::GitHub, |_, _| {
        Err("ci.yml isn't in this repository at aaaaaaa".to_owned())
    });
    assert_eq!(deps.source, DepSource::Timing);
    assert!(deps.notes[0].contains("isn't in this repository"));
    let read = deps_of(&run(jobs()), Forge::GitHub, |_, _| Ok(WORKFLOW.to_owned()));
    assert_eq!(read.source, DepSource::File);
}

fn step(name: &str, start: i64, end: i64) -> TimedStep {
    TimedStep {
        name: name.to_owned(),
        started: Some(start),
        finished: Some(end),
        ..TimedStep::default()
    }
}

#[test]
fn newly_slow_needs_both_a_share_and_seconds() {
    assert!(compare::newly_slow(100, 60));
    assert!(!compare::newly_slow(70, 60), "+10 s isn't enough");
    assert!(!compare::newly_slow(1030, 1000), "+3 % isn't enough");
}

#[test]
fn runs_are_aligned_by_job_and_step_names_and_recorded_differences_shown() {
    let mut before = jobs();
    before[1].steps = vec![step("compile", 5, 100), step("upload", 100, 120)];
    before[1].runner = vec![("labels".to_owned(), "windows-2022".to_owned())];
    let mut now = jobs();
    now[1].finished = Some(500);
    now[1].steps = vec![step("compile", 5, 400), step("upload", 400, 420)];
    now[1].runner = vec![
        ("labels".to_owned(), "windows-2025".to_owned()),
        ("runner".to_owned(), "r2".to_owned()),
    ];
    now.remove(3);
    now.push(job("new", 0, Some(1), Some(2)));
    let c = compare::compare(&run(now), &run(before));
    let names: Vec<&str> = c.jobs.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(names, ["lint", "build", "test", "new", "docs"]);
    let build = &c.jobs[1];
    assert!(build.slower);
    assert_eq!(build.delta(), Some(200));
    assert!(build.steps[0].slower && !build.steps[1].slower);
    assert_eq!(
        build.differences,
        [
            compare::Difference {
                what: "labels".to_owned(),
                before: "windows-2022".to_owned(),
                now: "windows-2025".to_owned(),
            },
            compare::Difference {
                what: "runner".to_owned(),
                before: "not recorded".to_owned(),
                now: "r2".to_owned(),
            },
        ]
    );
    assert_eq!((c.jobs[3].now, c.jobs[3].before), (Some(3), None));
    assert_eq!((c.jobs[4].now, c.jobs[4].before), (None, Some(3)));
    assert_eq!(c.slower().count(), 1);
}

#[test]
fn the_same_work_spread_over_more_time_is_lost_parallelism() {
    // Before: a and b side by side; now one after the other, each as long.
    let before = vec![
        job("a", 0, Some(0), Some(100)),
        job("b", 0, Some(0), Some(100)),
    ];
    let now = vec![
        job("a", 0, Some(0), Some(100)),
        job("b", 100, Some(100), Some(200)),
    ];
    let c = compare::compare(&run(now.clone()), &run(before.clone()));
    assert_eq!((c.span_now, c.span_before), (Some(200), Some(100)));
    assert_eq!(c.work_now, c.work_before);
    assert!(c.lost_parallelism);
    assert!(c.slower().next().is_none());
    // Longer because the jobs themselves took longer: not lost parallelism.
    let mut slower = before.clone();
    slower[0].finished = Some(200);
    slower[1].finished = Some(200);
    assert!(!compare::compare(&run(slower), &run(before)).lost_parallelism);
}

#[test]
fn provenance_says_when_a_run_is_read_in_part() {
    let project = Project {
        forge: Forge::GitHub,
        host: "github.com".to_owned(),
        path: "o/r".to_owned(),
    };
    let mut r = run(jobs());
    assert_eq!(
        provenance(&project, &r, None, ("HEAD", "abc")).partial,
        None
    );
    r.more = true;
    r.jobs[0].finished = None;
    let p = provenance(&project, &r, None, ("HEAD", "abc"));
    let partial = p.partial.unwrap_or_default();
    assert!(partial.contains("first 100 jobs") && partial.contains("hadn't finished"));
    assert_eq!(p.tips, [("HEAD".to_owned(), "abc".to_owned())]);
}

/// The workflow file is read at the run's commit: a later commit that changes its `needs`
/// gives the later run other dependencies, and the earlier run keeps its own.
#[test]
fn the_workflow_is_read_at_each_runs_revision() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("ci-path-needs")?;
    let path = ".github/workflows/ci.yml";
    let first = repo.commit("ci", &[(path, Some(WORKFLOW))])?;
    let changed = WORKFLOW.replace("needs: [lint, build]", "needs: [docs]");
    let second = repo.commit("ci: test after docs", &[(path, Some(&changed))])?;
    let file = |sha: &str, path: &str| workflow::at_revision(repo.path(), sha, path);
    let mut at_first = run(jobs());
    at_first.info.sha = first.clone();
    let mut at_second = run(jobs());
    at_second.info.sha = second;
    let pairs = |deps: &Deps| -> Vec<(usize, usize)> {
        deps.edges.iter().map(|d| (d.from, d.to)).collect()
    };
    let before = deps_of(&at_first, Forge::GitHub, file);
    let after = deps_of(&at_second, Forge::GitHub, file);
    assert_eq!(before.source, DepSource::File);
    assert_eq!(pairs(&before), [(0, 2), (1, 2)]);
    assert_eq!(pairs(&after), [(3, 2)]);
    assert_ne!(
        critical_path(&at_first.jobs, &before).jobs,
        critical_path(&at_second.jobs, &after).jobs
    );
    // A commit the repository doesn't have: no file, and so timing, said so.
    let mut elsewhere = run(jobs());
    elsewhere.info.sha = "b".repeat(40);
    let unknown = deps_of(&elsewhere, Forge::GitHub, file);
    assert_eq!(unknown.source, DepSource::Timing);
    assert!(
        unknown
            .notes
            .iter()
            .any(|n| n.contains("isn't in this repository"))
    );
    assert!(workflow::at_revision(repo.path(), &first, "-x").is_err());
    Ok(())
}
