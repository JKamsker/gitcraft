//! Reading the forge: the builds of a commit and their failed logs, open pull requests and one
//! in detail. Through the same tools and logins the window uses (`gh`, `glab`, `tea`).

use serde_json::{Value, json};

use super::{Ctx, Kind, ToolDef, boolean, number, object, short, string};
use crate::ci::{self, CheckState, Project};
use crate::forge::{self, Hosting};
use crate::git;
use crate::mcp::args::{self, Args};
use crate::mcp::output::{capped, text_page};

pub(super) const TOOLS: &[&ToolDef] = &[&CI, &PULL_REQUESTS];

/// The longest pull request description returned whole.
const MAX_BODY: usize = 8_000;
/// The default and largest page of a failed build's log.
const LOG_PAGE: u64 = 30_000;
const MAX_LOG_PAGE: u64 = 200_000;

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
                  default, and how each went. With `run`, that run's jobs and steps; add \
                  `failed_log: true` for what its failed jobs printed (the end of it, paged).",
    kind: Kind::Read,
    schema: || {
        object(
            json!({
                "commit": string("The commit. Default: HEAD."),
                "run": super::id("A run's id, from this tool's answer."),
                "failed_log": boolean("With `run`: the log of its failed jobs.", false),
                "offset": number("Byte offset in the log.", 0, 0, u64::from(u32::MAX)),
                "max_bytes": number("Size of the page of the log.", LOG_PAGE, 1000, MAX_LOG_PAGE),
            }),
            &[],
        )
    },
    run: ci_status,
};

fn ci_status(ctx: &Ctx, a: &Args) -> Result<Value, String> {
    a.only(&["commit", "run", "failed_log", "offset", "max_bytes"])?;
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
    let (hosting, remotes) = hosting(ctx)?;
    let project = Project::of(hosting.forge, &remotes).ok_or_else(|| {
        format!(
            "gitcraft doesn't read the builds of {} yet",
            hosting.forge.name()
        )
    })?;
    if let Some(run) = run {
        return run_jobs(ctx, &project, run, failed_log, offset, max);
    }
    let sha = super::history::resolve_commit(ctx, rev)?;
    let runs = ci::runs(&ctx.repo, &project, &sha)?;
    let (runs, more) = capped(runs, 50);
    let mut out = json!({
        "commit": short(&sha),
        "forge": hosting.forge.name(),
        "runs": runs.iter().map(|r| json!({
            "id": r.id,
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
    Ok(out)
}

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
        "jobs": jobs.iter().map(|j| json!({
            "name": j.name,
            "stage": j.stage,
            "state": state_word(j.state),
            "url": j.url,
            "steps": j.steps.iter().map(|s| json!({
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
        "title": d.title,
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
        "labels": d.labels,
        "checks": d.checks.iter().map(|c| json!({
            "name": c.name,
            "state": state_word(c.state),
            "detail": c.detail,
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
        };
        let args = serde_json::Map::from_iter([("failed_log".to_owned(), json!(true))]);
        let e = ci_status(&ctx, &Args::new(&args));
        assert!(e.is_err_and(|e| e.contains("needs `run`")));
        let args = serde_json::Map::from_iter([("number".to_owned(), json!("7"))]);
        assert!(pull_requests(&ctx, &Args::new(&args)).is_err());
    }
}
