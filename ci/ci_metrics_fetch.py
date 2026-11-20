"""Record the GitHub API data ci/ci_metrics.py measures, through `gh api` (issue #211).

The recording is plain JSON, trimmed to the fields the metrics read, so a window
can be replayed offline (`ci_metrics.py --replay`) and small fixtures can be cut
from it. Nothing personal is kept: no actors, authors, emails or runner names.

API use is bounded: every list is fetched with per_page=100 and paginated only as
far as the window needs, jobs are fetched once per run (all attempts in one
listing), earlier attempts only for re-run runs, and job logs only with --logs and
only for jobs that failed on a SHA where the same job also passed.
"""

import json
import subprocess
import time
import urllib.parse

PER_PAGE = 100
RETRIES = 4
MIN_REMAINING = 200

RUN_FIELDS = ("id", "name", "path", "event", "head_branch", "head_sha", "run_attempt", "status",
              "conclusion", "created_at", "run_started_at", "updated_at", "html_url")
ATTEMPT_FIELDS = ("id", "run_attempt", "status", "conclusion", "run_started_at", "updated_at")
JOB_FIELDS = ("id", "run_id", "run_attempt", "name", "status", "conclusion", "started_at", "completed_at",
              "html_url")


class ApiError(RuntimeError):
    pass


def trim(record, fields):
    return {field: record.get(field) for field in fields}


def trim_pr(pr):
    return {"number": pr["number"], "created_at": pr["created_at"], "merged_at": pr["merged_at"],
            "merge_commit_sha": pr["merge_commit_sha"], "head_ref": pr["head"]["ref"],
            "head_sha": pr["head"]["sha"], "base_ref": pr["base"]["ref"]}


class Gh:
    """`gh api` with retries on server errors and a call budget."""

    def __init__(self, max_calls, run=subprocess.run, sleep=time.sleep):
        self.max_calls, self.calls, self.run, self.sleep = max_calls, 0, run, sleep

    def text(self, path):
        for attempt in range(RETRIES):
            if self.calls >= self.max_calls:
                raise ApiError(f"call budget of {self.max_calls} exhausted (raise --max-calls)")
            self.calls += 1
            done = self.run(["gh", "api", "-H", "Accept: application/vnd.github+json", path],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
            if done.returncode == 0:
                return done.stdout
            error = (done.stderr or done.stdout).strip()
            if "rate limit" in error.lower():
                raise ApiError(f"GitHub rate limit reached at {path}: {error}")
            if "HTTP 5" not in error or attempt == RETRIES - 1:
                raise ApiError(f"gh api {path} failed: {error}")
            self.sleep(2 ** (attempt + 1))
        raise ApiError(f"gh api {path} failed")

    def json(self, path):
        return json.loads(self.text(path))

    def check_rate_limit(self):
        core = self.json("rate_limit")["resources"]["core"]
        if core["remaining"] < MIN_REMAINING:
            raise ApiError(f"only {core['remaining']} GitHub API calls left until {core['reset']}; try later")
        return core["remaining"]


def pages(gh, path, key, keep_going=lambda page: True):
    """Items of a paginated list endpoint; stops early when keep_going(page) is false."""
    page_number = 1
    while True:
        joiner = "&" if "?" in path else "?"
        body = gh.json(f"{path}{joiner}per_page={PER_PAGE}&page={page_number}")
        page = body[key] if key else body
        yield from page
        if len(page) < PER_PAGE or not keep_going(page):
            return
        page_number += 1


def merged_prs(gh, repo, last=None, since=None, until=None):
    """PRs merged into dev, newest first: the last `last`, or those merged in [since, until)."""
    found = []

    def keep_going(page):
        oldest_update = min(pr["updated_at"] for pr in page)
        # updated_at >= merged_at, so once a page is older than the boundary no later merge follows.
        if last is not None:
            merges = sorted((pr["merged_at"] for pr in found), reverse=True)
            return len(merges) < last or oldest_update >= merges[last - 1]
        return oldest_update >= since

    for pr in pages(gh, f"repos/{repo}/pulls?state=closed&base=dev&sort=updated&direction=desc", None,
                    keep_going):
        if pr.get("merged_at"):
            found.append(trim_pr(pr))
    found.sort(key=lambda pr: pr["merged_at"], reverse=True)
    if last is not None:
        return found[:last]
    return [pr for pr in found if since <= pr["merged_at"] < until]


def workflow_runs(gh, repo, workflow, **query):
    path = f"repos/{repo}/actions/workflows/{workflow}/runs?{urllib.parse.urlencode(query)}"
    return [trim(run, RUN_FIELDS) for run in pages(gh, path, "workflow_runs")]


def pr_runs(gh, repo, workflow, pr):
    runs = workflow_runs(gh, repo, workflow, event="pull_request", branch=pr["head_ref"])
    return [dict(run, pr=pr["number"]) for run in runs
            if pr["created_at"] <= run["created_at"] <= pr["merged_at"]]


def dev_runs(gh, repo, workflow, start, shas):
    runs = workflow_runs(gh, repo, workflow, event="push", branch="dev", created=f">={start}")
    return [run for run in runs if run["head_sha"] in shas]


def fetch(gh, repo, prs, ci="ci.yml", release="release.yml"):
    """Record a window: its PRs, their CI runs, dev CI/Release runs, attempts and jobs."""
    shas = {pr["merge_commit_sha"] for pr in prs}
    start = min(pr["merged_at"] for pr in prs)[:10]
    runs = [run for pr in prs for run in pr_runs(gh, repo, ci, pr)]
    runs += dev_runs(gh, repo, ci, start, shas)
    releases = dev_runs(gh, repo, release, start, shas)
    attempts, jobs = {}, {}
    for run in runs + releases:
        if run["status"] != "completed":
            continue
        attempts[str(run["id"])] = [trim(gh.json(f"repos/{repo}/actions/runs/{run['id']}/attempts/{n}"),
                                         ATTEMPT_FIELDS) for n in range(1, run["run_attempt"])]
        if run["conclusion"] == "cancelled" and run["run_attempt"] == 1:
            continue  # Superseded before it finished: its jobs say nothing about the code.
        jobs[str(run["id"])] = [trim(job, JOB_FIELDS) for job in pages(
            gh, f"repos/{repo}/actions/runs/{run['id']}/jobs?filter=all", "jobs")]
    return {"schema": "gitcraft-ci-metrics/1", "repo": repo, "prs": prs, "ci_runs": runs,
            "release_runs": releases, "attempts": attempts, "jobs": jobs, "logs": {}}


def fetch_logs(gh, repo, job_ids, extract):
    """Failing test names from the logs of the given jobs (only what `extract` keeps)."""
    found = {}
    for job_id in job_ids:
        try:
            found[str(job_id)] = extract(gh.text(f"repos/{repo}/actions/jobs/{job_id}/logs"))
        except ApiError as error:  # Expired logs must not lose the rest of the report.
            found[str(job_id)] = [f"(log unavailable: {str(error)[:120]})"]
    return found
