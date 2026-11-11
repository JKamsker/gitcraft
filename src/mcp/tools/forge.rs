//! Reading the forge: the builds of a commit and their failed logs, open pull requests and one
//! in detail. Through the same tools and logins the window uses (`gh`, `glab`, `tea`).

use serde_json::{Value, json};

use super::{Ctx, Kind, ToolDef, boolean, number, object, short, string};
use crate::ci::{self, CheckState, Project, Run, route};
use crate::forge::{self, Hosting};
use crate::git;
use crate::mcp::args::{self, Args};
use crate::mcp::output::{capped, clip, text_page};

pub(super) const TOOLS: &[&ToolDef] = &[&CI, &PULL_REQUESTS];

/// The most jobs, steps of a job, labels and checks listed.
const MAX_ITEMS: usize = 100;
/// The longest pull request description returned whole.
const MAX_BODY: usize = 8_000;
/// The default and largest page of a failed build's log.
const LOG_PAGE: u64 = 30_000;
const MAX_LOG_PAGE: u64 = 200_000;

/// Detects the repository's forge and its configured remotes.
fn hosting(ctx: &Ctx) -> Result<(Hosting, Vec<git::Remote>), String> {
    let remotes = git::remotes(&ctx.repo)?;
    let hosting = forge::detect(&ctx.repo, &remotes).ok_or(
        "this repository's remote isn't on a forge gitcraft knows (GitHub, GitLab, Forgejo, Gitea)",
    )?;
    Ok((hosting, remotes))
}

pub(crate) fn state_word(state: CheckState) -> &'static str {
    match state {
        CheckState::Passed => "passed",
        CheckState::Failed => "failed",
        CheckState::Running => "running",
        CheckState::Skipped => "skipped",
    }
}

static CI: ToolDef = ToolDef {
    name: "ci_status",
    title: "CI builds",
    description: "The CI runs (GitHub Actions) or pipelines (GitLab CI) of a commit, HEAD by \
                  default, and how each went, from every project of the repository's remotes \
                  that builds it (a fork and its upstream, say). With `run` (and its `project`), \
                  that run's jobs and steps; add `failed_log: true` for what its failed jobs \
                  printed (the end of it, paged).",
    kind: Kind::Read,
    schema: || {
        object(
            json!({
                "commit": string("The commit. Default: HEAD."),
                "run": super::id("A run's id, from this tool's answer."),
                "project": string(
                    "With `run`: the project it is in, as this tool's answer names it. \
                     Default: where the commit's branch is pushed."
                ),
                "failed_log": boolean("With `run`: the log of its failed jobs.", false),
                "offset": number("Byte offset in the log.", 0, 0, u64::from(u32::MAX)),
                "max_bytes": number("Size of the page of the log.", LOG_PAGE, 1000, MAX_LOG_PAGE),
            }),
            &[],
        )
    },
    run: ci_status,
};

/// Lists routed runs or reads jobs from a validated repository project.
fn ci_status(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&[
        "commit",
        "run",
        "project",
        "failed_log",
        "offset",
        "max_bytes",
    ])?;
    let rev = args::revision(a.opt_str("commit")?.unwrap_or("HEAD"))?;
    let run = match a.uint("run", 0, 0, u64::MAX)? {
        0 => None,
        id => Some(id),
    };
    let failed_log = a.bool("failed_log", false)?;
    let offset = a.uint("offset", 0, 0, u64::from(u32::MAX))? as usize;
    let max = a.uint("max_bytes", LOG_PAGE, 1000, MAX_LOG_PAGE)? as usize;
    if failed_log && run.is_none() {
        return Err("`failed_log` needs `run`: the id of a run from this tool's answer".to_owned());
    }
    let named = a.opt_str("project")?;
    if named.is_some() && run.is_none() {
        return Err("`project` goes with `run`".to_owned());
    }
    let (hosting, remotes) = hosting(ctx)?;
    let main = Project::of(hosting.forge, &remotes).ok_or_else(|| {
        format!(
            "gitcraft doesn't read the builds of {} yet",
            hosting.forge.name()
        )
    })?;
    if let (Some(run), Some(path)) = (run, named) {
        let project = named_project(path, &route::known(&main, &remotes))?;
        return run_jobs(ctx, &project, run, failed_log, offset, max);
    }
    if let Some(run) = run {
        let project = default_run_project(ctx, rev, &main, &remotes)?;
        return run_jobs(ctx, &project, run, failed_log, offset, max);
    }
    let sha = super::history::resolve_commit(ctx, rev)?;
    let refs = git::refs(&ctx.repo)?;
    let projects = route::projects_for(&sha, &main, &remotes, &refs);
    let answers = projects
        .iter()
        .map(|project| (project, ci::runs(&ctx.repo, project, &sha)))
        .collect();
    let AllRuns { runs, errors } = runs_of_all(answers)?;
    let (runs, more) = capped(runs, 50);
    let mut out = json!({
        "commit": short(&sha),
        "forge": hosting.forge.name(),
        "runs": runs.iter().map(|(project, r)| json!({
            "id": r.id,
            "project": project.path,
            "name": r.name,
            "title": r.title,
            "state": state_word(r.state),
            "event": r.event,
            "branch": r.branch,
            "attempt": r.attempt,
            "url": r.url,
        })).collect::<Vec<_>>(),
    });
    if more > 0 {
        out["runs_not_listed"] = json!(more);
    }
    if !errors.is_empty() {
        out["errors"] = json!(errors);
    }
    Ok(out)
}

/// Uses the commit's first candidate, or the main project for an unborn default HEAD.
fn default_run_project(
    ctx: &Ctx,
    rev: &str,
    main: &Project,
    remotes: &[git::Remote],
) -> Result<Project, String> {
    let refs = git::refs(&ctx.repo)?;
    if rev == "HEAD" && refs.head.is_empty() {
        return Ok(main.clone());
    }
    let sha = super::history::resolve_commit(ctx, rev)?;
    Ok(route::projects_for(&sha, main, remotes, &refs)
        .into_iter()
        .next()
        .unwrap_or_else(|| main.clone()))
}

/// The project `path` names, if it is one of the repository's remotes.
fn named_project(path: &str, known: &[Project]) -> Result<Project, String> {
    known
        .iter()
        .find(|p| p.path == path)
        .cloned()
        .ok_or_else(|| {
            let names: Vec<&str> = known.iter().map(|p| p.path.as_str()).collect();
            format!(
                "`project` must be one of this repository's remotes: {}",
                names.join(", ")
            )
        })
}

/// The runs of a commit from every project asked, each with its project.
#[derive(Debug)]
struct AllRuns<'a> {
    runs: Vec<(&'a Project, Run)>,
    /// Why a project couldn't be read, when another could.
    errors: Vec<String>,
}

/// Each project's runs of a commit, put together. When no project could be read, that is the
/// error.
fn runs_of_all(answers: Vec<(&Project, Result<Vec<Run>, String>)>) -> Result<AllRuns<'_>, String> {
    let asked = answers.len();
    let mut all = AllRuns {
        runs: Vec::new(),
        errors: Vec::new(),
    };
    for (project, answer) in answers {
        match answer {
            Ok(list) => all.runs.extend(list.into_iter().map(|run| (project, run))),
            Err(e) => all.errors.push(format!("{}: {e}", project.path)),
        }
    }
    if asked > 0 && all.errors.len() == asked {
        return Err(all.errors.join("; "));
    }
    Ok(all)
}

/// Reads a run's jobs and optionally a bounded page of its failed log.
fn run_jobs(
    ctx: &Ctx,
    project: &Project,
    run: u64,
    failed_log: bool,
    offset: usize,
    max: usize,
) -> Result<Value, String> {
    let jobs = ci::jobs(&ctx.repo, project, run)?;
    let mut out = json!({
        "run": run,
        "jobs_not_listed": jobs.len().saturating_sub(MAX_ITEMS),
        "jobs": jobs.iter().take(MAX_ITEMS).map(|j| json!({
            "name": j.name,
            "stage": j.stage,
            "state": state_word(j.state),
            "url": j.url,
            "steps": j.steps.iter().take(MAX_ITEMS).map(|s| json!({
                "name": s.name,
                "state": state_word(s.state),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    if failed_log {
        let log = ci::failed_log(&ctx.repo, project, run, &jobs)?;
        out["failed_log"] = text_page(&log, offset, max);
    }
    Ok(out)
}

static PULL_REQUESTS: ToolDef = ToolDef {
    name: "pull_requests",
    title: "Pull requests",
    description: "The open pull (or merge) requests of the repository's forge. With `number`, \
                  that one in detail: description, branches, mergeability, review decision, \
                  labels and its checks with how each went.",
    kind: Kind::Read,
    schema: || {
        object(
            json!({ "number": super::id("A pull request's number.") }),
            &[],
        )
    },
    run: pull_requests,
};

/// Lists pull requests or returns the requested pull request's details.
fn pull_requests(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&["number"])?;
    let number = a.uint("number", 0, 0, u64::MAX)?;
    let (hosting, _) = hosting(ctx)?;
    let provider = hosting.forge.provider();
    if number == 0 {
        let prs = provider.list(&ctx.repo, &hosting)?;
        let (prs, more) = capped(prs, 100);
        return Ok(json!({
            "forge": hosting.forge.name(),
            "pull_requests": prs.iter().map(|p| json!({
                "number": p.number,
                "title": p.title,
                "branch": p.branch,
                "commit": short(&p.sha),
                "author": p.author,
                "draft": p.draft,
                "url": p.url,
            })).collect::<Vec<_>>(),
            "not_listed": more,
        }));
    }
    let pages = provider.pages().ok_or_else(|| {
        format!(
            "gitcraft reads only the list of {}'s {}s, not one in detail",
            hosting.forge.name(),
            hosting.forge.noun()
        )
    })?;
    let d = pages.details(&ctx.repo, &hosting, number)?;
    let body = text_page(&d.body, 0, MAX_BODY);
    Ok(json!({
        "number": d.number,
        "title": clip(&d.title, 500),
        "state": d.state,
        "draft": d.draft,
        "author": d.author,
        "url": d.url,
        "base": d.base_branch,
        "head": d.head_branch,
        "head_commit": short(&d.head_oid),
        "body": body["text"],
        "body_cut": body.get("next_offset").is_some(),
        "additions": d.additions,
        "deletions": d.deletions,
        "changed_files": d.changed_files,
        "mergeable": d.mergeable,
        "merge_state": d.merge_state,
        "review_decision": d.review_decision,
        "labels": d.labels.iter().take(MAX_ITEMS).collect::<Vec<_>>(),
        "checks_not_listed": d.checks.len().saturating_sub(MAX_ITEMS),
        "checks": d.checks.iter().take(MAX_ITEMS).map(|c| json!({
            "name": clip(&c.name, 200),
            "state": state_word(c.state),
            "detail": clip(&c.detail, 300),
            "url": c.url,
        })).collect::<Vec<_>>(),
        "checks_error": d.checks_error,
        "commits": d.commits.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_log_needs_its_run() {
        let ctx = Ctx {
            repo: std::env::temp_dir(),
            agent: "t".into(),
            link_dir: None,
            wait: std::time::Duration::from_secs(1),
            cancelled: std::sync::Arc::new(|| false),
            run: None,
        };
        let args = serde_json::Map::from_iter([("failed_log".to_owned(), json!(true))]);
        let e = ci_status(&ctx, &Args::new(&args));
        assert!(e.is_err_and(|e| e.contains("needs `run`")));
        let args = serde_json::Map::from_iter([("number".to_owned(), json!("7"))]);
        assert!(pull_requests(&ctx, &Args::new(&args)).is_err());
        let args = serde_json::Map::from_iter([("project".to_owned(), json!("me/repo"))]);
        let e = ci_status(&ctx, &Args::new(&args));
        assert!(e.is_err_and(|e| e.contains("goes with `run`")));
    }

    fn project(path: &str) -> Project {
        Project {
            forge: crate::forge::Forge::GitHub,
            host: "github.com".to_owned(),
            path: path.to_owned(),
        }
    }

    fn run(id: u64) -> Run {
        Run {
            id,
            ..Run::default()
        }
    }

    /// A run's project has to be one of the remotes': an agent can't send the tools elsewhere.
    #[test]
    fn a_named_project_must_be_a_remotes() {
        let known = [project("up/repo"), project("me/repo")];
        let found = named_project("me/repo", &known);
        assert_eq!(found.map(|p| p.path), Ok("me/repo".to_owned()));
        let e = named_project("someone/else", &known);
        assert_eq!(
            e.err().as_deref(),
            Some("`project` must be one of this repository's remotes: up/repo, me/repo")
        );
    }

    /// Run details remain usable in an empty repository, while explicit bad revisions fail.
    #[test]
    fn run_details_default_to_main_only_when_head_is_unborn() -> Result<(), String> {
        let repo = git::fixture::Repo::new("ci-unborn-head")?;
        repo.git(&["remote", "add", "origin", "https://github.com/up/repo.git"])?;
        repo.git(&["remote", "add", "fork", "https://github.com/me/repo.git"])?;
        let ctx = Ctx {
            repo: repo.dir.clone(),
            agent: "t".into(),
            link_dir: None,
            wait: std::time::Duration::from_secs(1),
            cancelled: std::sync::Arc::new(|| false),
            run: None,
        };
        let remotes = git::remotes(repo.path())?;
        let main = project("up/repo");
        assert_eq!(default_run_project(&ctx, "HEAD", &main, &remotes)?, main);
        assert!(default_run_project(&ctx, "missing", &main, &remotes).is_err());
        repo.commit("one", &[])?;
        repo.git(&["config", "branch.main.pushRemote", "fork"])?;
        assert_eq!(
            default_run_project(&ctx, "HEAD", &main, &remotes)?,
            project("me/repo")
        );
        Ok(())
    }

    /// A remote outside the listing budget is still a valid named run-details target.
    #[test]
    fn named_run_projects_are_validated_independently_of_the_query_budget() {
        let main = project("up/repo");
        let remotes: Vec<_> = (0..8)
            .map(|i| git::Remote {
                name: format!("r{i}"),
                url: format!("https://github.com/o{i}/repo.git"),
            })
            .collect();
        let known = route::known(&main, &remotes);
        assert_eq!(named_project("o7/repo", &known), Ok(project("o7/repo")));
        assert!(named_project("unknown/repo", &known).is_err());
    }

    #[test]
    fn the_runs_of_every_project_are_listed_with_what_failed() {
        let (fork, upstream) = (project("me/repo"), project("up/repo"));
        let mirror = project("other/repo");
        let answers = vec![
            (&fork, Ok(vec![run(1), run(2)])),
            (&mirror, Ok(vec![run(3)])),
            (&upstream, Err("not signed in".to_owned())),
        ];
        let read = runs_of_all(answers);
        assert!(read.is_ok(), "{read:?}");
        let Ok(AllRuns { runs, errors }) = read else {
            return;
        };
        let ids: Vec<(&str, u64)> = runs.iter().map(|(p, r)| (p.path.as_str(), r.id)).collect();
        assert_eq!(ids, [("me/repo", 1), ("me/repo", 2), ("other/repo", 3)]);
        assert_eq!(errors, ["up/repo: not signed in"]);
    }

    #[test]
    fn when_no_project_can_be_read_that_is_the_error() {
        let (fork, upstream) = (project("me/repo"), project("up/repo"));
        let answers = vec![
            (&fork, Err("network".to_owned())),
            (&upstream, Err("not signed in".to_owned())),
        ];
        let read = runs_of_all(answers);
        assert_eq!(
            read.err().as_deref(),
            Some("me/repo: network; up/repo: not signed in")
        );
    }
}
