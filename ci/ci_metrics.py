#!/usr/bin/env python3
"""CI reliability metrics over a window of merges into dev (issue #211).

    python3 ci/ci_metrics.py                          # the last 20 merges into dev
    python3 ci/ci_metrics.py --last 40 --json m.json  # also write the metrics as JSON
    python3 ci/ci_metrics.py --since 2026-11-10 --until 2026-11-16
    python3 ci/ci_metrics.py --record window.json     # keep the API data for --replay
    python3 ci/ci_metrics.py --replay window.json     # recompute offline
    python3 ci/ci_metrics.py --logs                   # name the failing tests of flaky jobs

The window is a set of PRs merged into dev. It measures their pull_request CI
runs (created between PR creation and merge), the dev push CI runs and the
Release runs of their merge commits:

- first-attempt pass rate of PR runs and of dev push runs: attempt 1 concluded
  success, over attempts 1 that finished success or failure (cancelled runs are
  superseded, not broken, and are counted separately);
- re-runs: runs that needed more than one attempt, and how each attempt ended;
- cancelled ratio: runs whose attempt 1 was cancelled, over completed runs;
- same-SHA flakes: a job that failed and also passed on the same head SHA (a
  passing re-run does not prove the failure harmless; it is listed, not erased).
  A PR run's head SHA is the PR head: two runs of it can test different dev bases;
- time to first actionable failure: from attempt 1 starting to the first failing
  job that is not an aggregate (`CI result`, `test (windows-latest)`);
- post-merge Release reds, with the failed jobs;
- duration drift: each job's recent median against its window median;
- the last known-green dev SHA (a dev push CI run that ended green).

The computation (`compute`) is a pure function of the recorded JSON. Prints
Markdown (also to GITHUB_STEP_SUMMARY when set); exits 0 unless the API fails.
"""

import argparse
from datetime import datetime, timedelta
import json
import os
from pathlib import Path
import re
from statistics import median
import sys

AGGREGATE_JOBS = ("CI result", "test (windows-latest)")
FAILED = ("failure", "timed_out", "startup_failure")
FINISHED = ("success",) + FAILED
RECENT = 5
MIN_SECONDS = 60
TEST_FAILED = re.compile(r"\btest (\S+) \.\.\. FAILED")
WATCHDOG = re.compile(r"##\[error\].*?; running: ([^;\s]+(?:, [^;\s]+)*); ")


def when(stamp):
    return datetime.fromisoformat(stamp.replace("Z", "+00:00")) if stamp else None


def seconds(start, end):
    return (when(end) - when(start)).total_seconds()


def rate(passed, total):
    return round(passed / total, 3) if total else None


def failing_tests(log):
    """Test names a job log reports as failed, stalled or timed out (libtest and ci/libtest_watchdog.py)."""
    names = []
    for line in log.splitlines():
        found = TEST_FAILED.search(line)
        stalled = WATCHDOG.search(line)
        for name in ([found.group(1)] if found else []) + (stalled.group(1).split(", ") if stalled else []):
            if name not in names and name != "unknown":
                names.append(name)
    return names


def first_attempt(run, attempts):
    """(conclusion, started) of attempt 1."""
    if run["run_attempt"] == 1:
        return run["conclusion"], run["run_started_at"]
    for attempt in attempts.get(str(run["id"]), []):
        if attempt["run_attempt"] == 1:
            return attempt["conclusion"], attempt["run_started_at"]
    return None, run["created_at"]


def unique_jobs(jobs):
    """Jobs as they really ran: a re-run of failed jobs copies the passed ones into the new attempt."""
    seen, unique = set(), []
    for job in sorted(jobs, key=lambda job: (job["run_attempt"], job["id"])):
        key = (job["name"], job["started_at"], job["completed_at"], job["conclusion"])
        if key not in seen:
            seen.add(key)
            unique.append(job)
    return unique


def pass_rate(runs, attempts):
    firsts = [first_attempt(run, attempts)[0] for run in runs if run["status"] == "completed"]
    finished = [conclusion for conclusion in firsts if conclusion in FINISHED]
    passed = sum(conclusion == "success" for conclusion in finished)
    cancelled = firsts.count("cancelled")  # Superseded before attempt 1 finished.
    return {"runs": len(runs), "completed": len(firsts), "first_attempt_finished": len(finished),
            "first_attempt_passed": passed, "first_attempt_pass_rate": rate(passed, len(finished)),
            "cancelled": cancelled, "cancelled_ratio": rate(cancelled, len(firsts))}


def reruns(runs, attempts):
    found = []
    for run in runs:
        if run["run_attempt"] > 1:
            earlier = sorted(attempts.get(str(run["id"]), []), key=lambda attempt: attempt["run_attempt"])
            found.append({"run": run["id"], "event": run["event"], "pr": run.get("pr"), "sha": run["head_sha"],
                          "attempts": [attempt["conclusion"] for attempt in earlier] + [run["conclusion"]],
                          "url": run["html_url"]})
    return found


def flakes(runs, jobs, logs, aggregate=AGGREGATE_JOBS):
    by_key = {}
    for run in runs:
        for job in unique_jobs(jobs.get(str(run["id"]), [])):
            if job["name"] not in aggregate:
                by_key.setdefault((run["head_sha"], job["name"]), []).append((run, job))
    found = []
    for (sha, name), seen in sorted(by_key.items()):
        failed = [(run, job) for run, job in seen if job["conclusion"] in FAILED]
        if failed and any(job["conclusion"] == "success" for _, job in seen):
            found.append({"sha": sha, "job": name, "pr": failed[0][0].get("pr"), "event": failed[0][0]["event"],
                          "failures": [{"run": run["id"], "attempt": job["run_attempt"], "url": job["html_url"],
                                        "tests": logs.get(str(job["id"]), [])} for run, job in failed],
                          "passes": sum(job["conclusion"] == "success" for _, job in seen)})
    return found


def time_to_failure(runs, attempts, jobs, aggregate=AGGREGATE_JOBS):
    found = []
    for run in runs:
        conclusion, started = first_attempt(run, attempts)
        if conclusion not in FAILED:
            continue
        failing = [job for job in jobs.get(str(run["id"]), []) if job["run_attempt"] == 1
                   and job["conclusion"] in FAILED and job["name"] not in aggregate and job["completed_at"]]
        if failing:
            first = min(failing, key=lambda job: job["completed_at"])
            found.append({"run": run["id"], "event": run["event"], "pr": run.get("pr"), "job": first["name"],
                          "minutes": round(seconds(started, first["completed_at"]) / 60, 1), "url": first["html_url"]})
    minutes = [item["minutes"] for item in found]
    return {"runs": found, "median_minutes": median(minutes) if minutes else None,
            "max_minutes": max(minutes) if minutes else None}


def releases(prs, release_runs, attempts, jobs):
    by_sha = {}
    for run in release_runs:
        by_sha.setdefault(run["head_sha"], []).append(run)
    found = []
    for pr in sorted(prs, key=lambda pr: pr["merged_at"]):
        for run in sorted(by_sha.get(pr["merge_commit_sha"], []), key=lambda run: run["id"]):
            first, _ = first_attempt(run, attempts)
            failed = sorted({job["name"] for job in jobs.get(str(run["id"]), [])
                             if job["run_attempt"] == 1 and job["conclusion"] in FAILED})
            found.append({"pr": pr["number"], "sha": pr["merge_commit_sha"], "run": run["id"], "first": first,
                          "final": run["conclusion"], "status": run["status"], "failed_jobs": failed,
                          "url": run["html_url"]})
    return found


def durations(runs, jobs, recent=RECENT, min_seconds=MIN_SECONDS, aggregate=AGGREGATE_JOBS):
    samples = {}
    for run in runs:
        for job in unique_jobs(jobs.get(str(run["id"]), [])):
            if job["conclusion"] == "success" and job["name"] not in aggregate and job["started_at"]:
                samples.setdefault(job["name"], []).append((job["completed_at"],
                                                            seconds(job["started_at"], job["completed_at"])))
    found = []
    for name, values in samples.items():
        ordered = [value for _, value in sorted(values)]
        window = median(ordered)
        if len(ordered) < 3 or window < min_seconds:
            continue
        latest = median(ordered[-recent:])
        found.append({"job": name, "samples": len(ordered), "median_minutes": round(window / 60, 1),
                      "recent_median_minutes": round(latest / 60, 1), "drift": round((latest - window) / window, 3)})
    return sorted(found, key=lambda item: -abs(item["drift"]))


def last_green(dev_runs):
    green = [run for run in dev_runs if run["conclusion"] == "success"]
    if not green:
        return None
    run = max(green, key=lambda run: run["created_at"])
    return {"sha": run["head_sha"], "run": run["id"], "attempt": run["run_attempt"], "url": run["html_url"]}


def compute(data, recent=RECENT):
    prs, attempts, jobs = data["prs"], data["attempts"], data["jobs"]
    pr_runs = [run for run in data["ci_runs"] if run["event"] == "pull_request"]
    dev = [run for run in data["ci_runs"] if run["event"] == "push"]
    released = releases(prs, data["release_runs"], attempts, jobs)
    merged = sorted(prs, key=lambda pr: pr["merged_at"])
    return {
        "repo": data["repo"],
        "window": {"merges": len(prs), "first_merge": merged[0]["merged_at"] if merged else None,
                   "last_merge": merged[-1]["merged_at"] if merged else None,
                   "prs": [pr["number"] for pr in merged]},
        "pr_runs": pass_rate(pr_runs, attempts),
        "dev_runs": dict(pass_rate(dev, attempts),
                         merges_without_run=sorted(pr["number"] for pr in prs
                                                   if pr["merge_commit_sha"] not in {run["head_sha"] for run in dev})),
        "per_pr": per_pr(merged, pr_runs, attempts),
        "reruns": reruns(data["ci_runs"], attempts),
        "flakes": flakes(data["ci_runs"], jobs, data.get("logs", {})),
        "time_to_first_failure": time_to_failure(data["ci_runs"], attempts, jobs),
        "releases": released,
        "release_reds": sum(item["first"] in FAILED for item in released),
        "durations": durations(data["ci_runs"], jobs, recent),
        "last_green_dev": last_green(dev),
    }


def per_pr(prs, pr_runs, attempts):
    rows = []
    for pr in prs:
        runs = [run for run in pr_runs if run.get("pr") == pr["number"]]
        firsts = [first_attempt(run, attempts)[0] for run in runs]
        rows.append({"pr": pr["number"], "runs": len(runs), "cancelled": firsts.count("cancelled"),
                     "first_attempt_failed": sum(first in FAILED for first in firsts),
                     "first_attempt_passed": firsts.count("success")})
    return rows


def percent(value):
    return "n/a" if value is None else f"{value * 100:.0f}%"


def markdown(metrics):
    window, pr, dev = metrics["window"], metrics["pr_runs"], metrics["dev_runs"]
    ttf = metrics["time_to_first_failure"]
    lines = [f"## CI reliability: last {window['merges']} merges into dev ({metrics['repo']})", "",
             f"Merged {window['first_merge']} to {window['last_merge']}: "
             + ", ".join(f"#{number}" for number in window["prs"]) + ".", "",
             "| Metric | PR CI runs | dev push CI runs |", "| --- | --- | --- |",
             f"| Runs (completed) | {pr['runs']} ({pr['completed']}) | {dev['runs']} ({dev['completed']}) |",
             f"| First-attempt pass rate | {percent(pr['first_attempt_pass_rate'])} "
             f"({pr['first_attempt_passed']}/{pr['first_attempt_finished']}) | "
             f"{percent(dev['first_attempt_pass_rate'])} ({dev['first_attempt_passed']}/{dev['first_attempt_finished']}) |",
             f"| Cancelled ratio | {percent(pr['cancelled_ratio'])} ({pr['cancelled']}) | "
             f"{percent(dev['cancelled_ratio'])} ({dev['cancelled']}) |", ""]
    if dev["merges_without_run"]:
        lines += [f"Merges without a dev push CI run: {', '.join(f'#{n}' for n in dev['merges_without_run'])}.", ""]
    lines += [f"Re-runs needed: {len(metrics['reruns'])}. Same-SHA flaky jobs: {len(metrics['flakes'])}. "
              f"Post-merge Release reds (attempt 1): {metrics['release_reds']} of {len(metrics['releases'])}. "
              f"Time to first actionable failure: median {ttf['median_minutes']} min, max {ttf['max_minutes']} min "
              f"over {len(ttf['runs'])} failed run(s).", ""]
    green = metrics["last_green_dev"]
    lines += [f"Last known-green dev SHA: `{green['sha']}` ([run {green['run']}]({green['url']}), attempt "
              f"{green['attempt']})." if green else "Last known-green dev SHA: none in the window.", ""]
    lines += section_tables(metrics)
    return "\n".join(lines) + "\n"


def section_tables(metrics):
    lines = []
    if metrics["releases"]:
        lines += ["### Release runs of the merges", "", "| PR | SHA | Attempt 1 | Final | Failed jobs (attempt 1) |",
                  "| --- | --- | --- | --- | --- |"]
        lines += [f"| #{item['pr']} | [`{item['sha'][:8]}`]({item['url']}) | {item['first'] or item['status']} | "
                  f"{item['final'] or item['status']} | {', '.join(item['failed_jobs']) or '-'} |"
                  for item in metrics["releases"]]
        lines.append("")
    if metrics["flakes"]:
        lines += ["### Same-SHA flaky jobs (failed, and passed on the same SHA)", "",
                  "| Event | PR | SHA | Job | Failed attempts | Failing tests |", "| --- | --- | --- | --- | --- | --- |"]
        for item in metrics["flakes"]:
            failures = ", ".join(f"[run {f['run']} #{f['attempt']}]({f['url']})" for f in item["failures"])
            tests = "; ".join(sorted({test for f in item["failures"] for test in f["tests"]})) or "-"
            lines.append(f"| {item['event']} | {('#' + str(item['pr'])) if item['pr'] else '-'} | "
                         f"`{item['sha'][:8]}` | {item['job']} | {failures} | {tests} |")
        lines.append("")
    if metrics["reruns"]:
        lines += ["### Re-run runs", "", "| Event | PR | SHA | Attempts |", "| --- | --- | --- | --- |"]
        lines += [f"| {item['event']} | {('#' + str(item['pr'])) if item['pr'] else '-'} | "
                  f"[`{item['sha'][:8]}`]({item['url']}) | {' → '.join(str(a) for a in item['attempts'])} |"
                  for item in metrics["reruns"]]
        lines.append("")
    lines += per_pr_and_timing_tables(metrics)
    return lines


def per_pr_and_timing_tables(metrics):
    lines = ["### PR CI runs per PR", "", "| PR | Runs | Cancelled | Attempt 1 failed | Attempt 1 passed |",
             "| --- | --- | --- | --- | --- |"]
    lines += [f"| #{row['pr']} | {row['runs']} | {row['cancelled']} | {row['first_attempt_failed']} | "
              f"{row['first_attempt_passed']} |" for row in metrics["per_pr"]]
    lines.append("")
    if metrics["time_to_first_failure"]["runs"]:
        lines += ["### Time to first actionable failure", "", "| Event | PR | First failing job | Minutes |",
                  "| --- | --- | --- | --- |"]
        lines += [f"| {item['event']} | {('#' + str(item['pr'])) if item['pr'] else '-'} | [{item['job']}]({item['url']}) "
                  f"| {item['minutes']} |" for item in metrics["time_to_first_failure"]["runs"]]
        lines.append("")
    if metrics["durations"]:
        lines += [f"### Job duration drift (median of the last {RECENT} passes vs the window median)", "",
                  "| Job | Passes | Window median (min) | Recent median (min) | Drift |", "| --- | --- | --- | --- | --- |"]
        lines += [f"| {item['job']} | {item['samples']} | {item['median_minutes']} | {item['recent_median_minutes']} | "
                  f"{item['drift'] * 100:+.0f}% |" for item in metrics["durations"]]
        lines.append("")
    return lines


def flaky_job_ids(data):
    return [failure_job for item in flakes(data["ci_runs"], data["jobs"], {})
            for failure_job in failed_job_ids(data, item)]


def failed_job_ids(data, item):
    return [job["id"] for failure in item["failures"] for job in data["jobs"].get(str(failure["run"]), [])
            if job["name"] == item["job"] and job["run_attempt"] == failure["attempt"] and job["conclusion"] in FAILED]


def window_dates(args):
    until = (datetime.fromisoformat(args.until) + timedelta(days=1)).strftime("%Y-%m-%d") if args.until else "9999"
    return args.since, until


def record(args):
    import ci_metrics_fetch as fetch  # noqa: PLC0415 - only needed with network access.
    gh = fetch.Gh(args.max_calls)
    gh.check_rate_limit()
    if args.since:
        since, until = window_dates(args)
        prs = fetch.merged_prs(gh, args.repo, since=since, until=until)
    else:
        prs = fetch.merged_prs(gh, args.repo, last=args.last)
    if not prs:
        raise fetch.ApiError("no merged PRs in the window")
    data = fetch.fetch(gh, args.repo, prs)
    if args.logs:
        data["logs"] = fetch.fetch_logs(gh, args.repo, flaky_job_ids(data), failing_tests)
    print(f"({gh.calls} GitHub API calls)", file=sys.stderr)
    return data


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", default="JKamsker/gitcraft")
    parser.add_argument("--last", type=int, default=20, help="the last N merges into dev (default 20)")
    parser.add_argument("--since", help="merged on or after this date (YYYY-MM-DD) instead of --last")
    parser.add_argument("--until", help="merged on or before this date (YYYY-MM-DD), with --since")
    parser.add_argument("--logs", action="store_true", help="read the logs of flaky jobs for failing test names")
    parser.add_argument("--max-calls", type=int, default=1500, help="GitHub API call budget")
    parser.add_argument("--record", type=Path, help="write the fetched API data here")
    parser.add_argument("--replay", type=Path, help="compute from recorded API data instead of fetching")
    parser.add_argument("--json", type=Path, help="write the metrics as JSON here")
    args = parser.parse_args(argv)
    if args.until and not args.since:
        parser.error("--until needs --since")
    if args.replay:
        data = json.loads(args.replay.read_text(encoding="utf-8"))
    else:
        try:
            data = record(args)
        except RuntimeError as error:
            print(f"error: {error}", file=sys.stderr)
            return 1
    if args.record:
        args.record.write_text(json.dumps(data, indent=1) + "\n", encoding="utf-8")
    metrics = compute(data)
    text = markdown(metrics)
    # The report is Markdown with `→`: write UTF-8 even where the console or a redirect
    # would otherwise use a legacy code page (cp1252 on a Windows console).
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stdout.write(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(text)
    if args.json:
        args.json.write_text(json.dumps(metrics, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
