#!/usr/bin/env python3
"""Release gate: wait until CI's required check passed for this exact commit (issue #211).

Publishing is allowed only when a CI run of `.github/workflows/ci.yml` for the
same head SHA, from a push or a merge-queue (merge_group) event, finished with
its aggregate job (`CI result`) successful. Such a run used the workflow files of
that same commit, so a green run for another SHA, merge result or workflow
revision never counts. Pull-request runs test a merge commit, not this SHA, and
are ignored. The gate waits while a matching run is queued or running (including
a re-run), and fails when every matching run finished without the required job
passing, or when none appeared in time.

    GH_TOKEN=... python3 ci/await_ci.py --repo owner/name --sha <sha>
"""

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request

WORKFLOW_PATH = ".github/workflows/ci.yml"
EVENTS = ("push", "merge_group")
REQUIRED_JOB = "CI result"


def decide(runs, jobs_of, sha, required=REQUIRED_JOB):
    """('passed'|'waiting'|'failed', explanation) for the runs the API returned."""
    matching = [run for run in runs if run.get("head_sha") == sha and run.get("event") in EVENTS
                and run.get("path", "").split("@", 1)[0] == WORKFLOW_PATH]
    if not matching:
        return "waiting", f"no {'/'.join(EVENTS)} CI run for {sha} yet"
    pending, finished = [], []
    for run in sorted(matching, key=lambda run: run.get("id", 0)):
        where = f"run {run.get('id')} attempt {run.get('run_attempt', 1)} ({run.get('event')}) {run.get('html_url', '')}"
        if run.get("status") != "completed":
            pending.append(f"{where}: {run.get('status')}")
            continue
        jobs = [job for job in jobs_of(run) if job.get("name") == required]
        if len(jobs) != 1:
            finished.append(f"{where}: {len(jobs)} '{required}' jobs (expected exactly one)")
            continue
        job = jobs[0]
        if job.get("status") == "completed" and job.get("conclusion") == "success":
            return "passed", f"{where}: '{required}' succeeded"
        finished.append(f"{where}: '{required}' {job.get('status')}/{job.get('conclusion')}")
    if pending:
        return "waiting", "; ".join(pending + finished)
    return "failed", "; ".join(finished)


def api(token):
    def get(url):
        request = urllib.request.Request(url, headers={
            "Accept": "application/vnd.github+json", "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "ci-release-gate"})
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.load(response)
    return get


def wait(get, repo, sha, *, timeout, interval, appear, clock=time.monotonic, sleep=time.sleep):
    base = f"https://api.github.com/repos/{repo}/actions"
    started = clock()
    while True:
        try:
            runs = get(f"{base}/workflows/ci.yml/runs?head_sha={sha}&per_page=100")["workflow_runs"]
            state, why = decide(runs, lambda run: get(
                f"{base}/runs/{run['id']}/jobs?filter=latest&per_page=100")["jobs"], sha)
        except (OSError, KeyError, ValueError, urllib.error.URLError) as error:
            state, why = "waiting", f"GitHub API error, retrying: {error}"
        elapsed = clock() - started
        print(f"[{elapsed:.0f}s] {state}: {why}", flush=True)
        if state != "waiting":
            return state == "passed", why
        if why.startswith("no ") and elapsed >= appear:
            return False, f"no CI run for {sha} appeared within {appear:.0f}s"
        if elapsed >= timeout:
            return False, f"timed out after {timeout:.0f}s: {why}"
        sleep(interval)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--timeout", type=float, default=150 * 60)
    parser.add_argument("--appear", type=float, default=15 * 60, help="seconds for a matching run to appear")
    parser.add_argument("--interval", type=float, default=30)
    args = parser.parse_args(argv)
    token = os.environ.get("GH_TOKEN")
    if not token:
        print("::error::GH_TOKEN is required")
        return 1
    passed, why = wait(api(token), args.repo, args.sha, timeout=args.timeout,
                       interval=args.interval, appear=args.appear)
    if not passed:
        print(f"::error title=CI has not passed for {args.sha}::{why}")
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
