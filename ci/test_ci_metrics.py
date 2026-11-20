"""CI reliability metrics over recorded GitHub API data, offline (issue #211).

The fixture is cut from a real recording (`ci_metrics.py --record`) of
JKamsker/gitcraft: PR #255, whose dev push run failed the repo-d-f Windows shard
on attempt 1 and passed on a re-run, and PR #244, whose dev push run stalled
repo-rest so its Release run's CI gate failed. Only the fields the metrics read
are kept; jobs not needed for these cases were dropped.
"""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import ci_metrics  # noqa: E402
import ci_metrics_fetch  # noqa: E402

FIXTURE = Path(__file__).parent / "fixtures" / "ci_metrics_window.json"


def load():
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


def job(name, conclusion, attempt=1, started="2026-11-20T16:00:00Z", completed="2026-11-20T16:10:00Z", job_id=1):
    return {"id": job_id, "run_id": 9, "run_attempt": attempt, "name": name, "status": "completed",
            "conclusion": conclusion, "started_at": started, "completed_at": completed, "html_url": f"u/{job_id}"}


class LogTests(unittest.TestCase):
    def test_libtest_failures_and_watchdog_stalls_name_their_tests(self):
        log = "\n".join([
            "2026-11-15T14:29:07.27Z test repo::a::passes ... ok",
            "2026-11-15T14:29:08.27Z test repo::b::breaks ... FAILED",
            "2026-11-15T14:41:27.09Z ##[error]gitcraft-1.exe: repo-rest after 736s; exit 1; running: "
            "repo::timeline::receipt_tests::a_squash; terminated Job Object",
            "2026-11-15T14:41:28.00Z ##[error]gitcraft-1.exe: core after 9s; exit 1; running: unknown; x",
            "2026-11-15T14:41:29.00Z test repo::b::breaks ... FAILED",
        ])
        self.assertEqual(ci_metrics.failing_tests(log),
                         ["repo::b::breaks", "repo::timeline::receipt_tests::a_squash"])


class JobTests(unittest.TestCase):
    def test_jobs_copied_into_a_rerun_attempt_count_once(self):
        jobs = [job("lint", "success", 1, job_id=1), job("lint", "success", 2, job_id=2),
                job("shard", "failure", 1, job_id=3),
                job("shard", "success", 2, "2026-11-20T17:00:00Z", "2026-11-20T17:05:00Z", job_id=4)]
        self.assertEqual([item["id"] for item in ci_metrics.unique_jobs(jobs)], [1, 3, 4])

    def test_attempt_one_comes_from_the_attempts_of_a_rerun(self):
        run = {"id": 7, "run_attempt": 2, "conclusion": "success", "run_started_at": "b", "created_at": "a"}
        attempts = {"7": [{"run_attempt": 1, "conclusion": "failure", "run_started_at": "a"}]}
        self.assertEqual(ci_metrics.first_attempt(run, attempts), ("failure", "a"))
        self.assertEqual(ci_metrics.first_attempt(dict(run, run_attempt=1), {}), ("success", "b"))


class WindowTests(unittest.TestCase):
    def setUp(self):
        self.metrics = ci_metrics.compute(load())

    def test_dev_first_attempt_pass_rate_excludes_nothing_but_counts_the_rerun_as_red(self):
        dev = self.metrics["dev_runs"]
        self.assertEqual((dev["first_attempt_passed"], dev["first_attempt_finished"]), (0, 2))
        self.assertEqual(dev["first_attempt_pass_rate"], 0.0)
        self.assertEqual(dev["merges_without_run"], [])

    def test_cancelled_pr_runs_are_counted_apart_from_failures(self):
        pr = self.metrics["pr_runs"]
        self.assertGreater(pr["cancelled"], 0)
        self.assertEqual(pr["first_attempt_finished"] + pr["cancelled"], pr["completed"])
        rows = {row["pr"]: row for row in self.metrics["per_pr"]}
        self.assertEqual(set(rows), {244, 255})

    def test_same_sha_flakes_name_the_shard_that_failed_then_passed(self):
        flaky = {(item["sha"][:8], item["job"]) for item in self.metrics["flakes"]}
        self.assertIn(("7dc0bdf9", "windows-tests / Windows tests (repo-d-f)"), flaky)
        self.assertIn(("014add85", "windows-tests / Windows tests (repo-rest)"), flaky)
        self.assertFalse(any(job in ci_metrics.AGGREGATE_JOBS for _, job in flaky))
        stalled = next(item for item in self.metrics["flakes"] if item["sha"].startswith("014add85"))
        self.assertEqual(stalled["failures"][0]["tests"],
                         ["repo::timeline::receipt_tests::a_squash_says_the_files_are_identical_and_a_commit_went"])

    def test_reruns_keep_every_attempt(self):
        reruns = {item["sha"][:8]: item["attempts"] for item in self.metrics["reruns"]}
        self.assertEqual(reruns["7dc0bdf9"], ["failure", "success"])
        self.assertEqual(reruns["014add85"], ["failure", "success"])

    def test_time_to_first_actionable_failure_skips_aggregate_jobs(self):
        runs = {item["run"]: item for item in self.metrics["time_to_first_failure"]["runs"]}
        stalled = runs[43721621876]
        self.assertEqual(stalled["job"], "windows-tests / Windows tests (repo-rest)")
        self.assertEqual(stalled["minutes"], 45.1)  # 13:56:27 to 14:41:32.

    def test_release_reds_list_the_failed_gate(self):
        releases = {item["pr"]: item for item in self.metrics["releases"]}
        self.assertEqual(releases[244]["first"], "failure")
        self.assertEqual(releases[244]["failed_jobs"], ["CI passed for this commit"])
        self.assertEqual(releases[255]["failed_jobs"], ["CI passed for this commit"])
        self.assertEqual(self.metrics["release_reds"], 2)

    def test_last_green_dev_sha_is_the_newest_green_dev_run(self):
        self.assertTrue(self.metrics["last_green_dev"]["sha"].startswith("7dc0bdf9"))
        self.assertEqual(self.metrics["last_green_dev"]["attempt"], 2)

    def test_duration_drift_compares_recent_passes_with_the_window_median(self):
        drift = ci_metrics.durations(
            [{"id": 9, "head_sha": "s", "event": "push"}],
            {"9": [job("shard", "success", 1, f"2026-11-20T1{i}:00:00Z", f"2026-11-20T1{i}:{m}:00Z", i)
                   for i, m in enumerate(("10", "10", "10", "20", "20"))]}, recent=2)
        self.assertEqual(drift, [{"job": "shard", "samples": 5, "median_minutes": 10.0,
                                  "recent_median_minutes": 20.0, "drift": 1.0}])

    def test_markdown_and_json_report_from_a_replay(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "m.json"
            done = subprocess.run([sys.executable, str(Path(__file__).parent / "ci_metrics.py"),
                                   "--replay", str(FIXTURE), "--json", str(out)],
                                  stdout=subprocess.PIPE, text=True, encoding="utf-8", check=True)
            self.assertIn("## CI reliability: last 2 merges into dev", done.stdout)
            self.assertIn("Last known-green dev SHA: `7dc0bdf9", done.stdout)
            self.assertEqual(json.loads(out.read_text(encoding="utf-8"))["release_reds"], 2)


class FetchTests(unittest.TestCase):
    def test_server_errors_are_retried_and_the_budget_is_enforced(self):
        replies = [subprocess.CompletedProcess([], 1, "", "gh: Server Error (HTTP 502)"),
                   subprocess.CompletedProcess([], 0, '{"ok": 1}', "")]
        gh = ci_metrics_fetch.Gh(2, run=lambda *a, **k: replies.pop(0), sleep=lambda s: None)
        self.assertEqual(gh.json("x"), {"ok": 1})
        with self.assertRaisesRegex(ci_metrics_fetch.ApiError, "budget"):
            gh.json("y")

    def test_rate_limit_errors_stop_at_once(self):
        calls = []

        def run(*args, **kwargs):
            calls.append(args)
            return subprocess.CompletedProcess([], 1, "", "gh: API rate limit exceeded (HTTP 403)")
        gh = ci_metrics_fetch.Gh(10, run=run, sleep=lambda s: None)
        with self.assertRaisesRegex(ci_metrics_fetch.ApiError, "rate limit"):
            gh.json("x")
        self.assertEqual(len(calls), 1)

    def test_the_last_merges_stop_paginating_once_older_than_the_window(self):
        def pr(number, merged, updated):
            return {"number": number, "created_at": "2026-11-01T00:00:00Z", "merged_at": merged,
                    "updated_at": updated, "merge_commit_sha": f"m{number}", "head": {"ref": f"b{number}",
                                                                                     "sha": f"h{number}"},
                    "base": {"ref": "dev"}}
        first = [pr(n, f"2026-11-{10 + n % 10:02}T00:00:00Z", f"2026-11-{10 + n % 10:02}T01:00:00Z")
                 for n in range(100)]
        requested = []

        class Fake:
            def json(self, path):
                requested.append(path)
                return first if path.endswith("page=1") else [pr(999, "2026-11-30T00:00:00Z", "2026-11-30T00:00:00Z")]
        found = ci_metrics_fetch.merged_prs(Fake(), "o/r", last=3)
        self.assertEqual(len(requested), 1)
        self.assertEqual([item["merged_at"][:10] for item in found], ["2026-11-19"] * 3)
        self.assertEqual(set(found[0]), {"number", "created_at", "merged_at", "merge_commit_sha", "head_ref",
                                         "head_sha", "base_ref"})


if __name__ == "__main__":
    unittest.main()
