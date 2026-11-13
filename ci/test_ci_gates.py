"""Required-check, release-gate, path-filter and failure-summary semantics (issue #211)."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import await_ci  # noqa: E402
import changed_paths  # noqa: E402
import ci_summary  # noqa: E402
import require_results  # noqa: E402

SHA = "a" * 40


class RequiredResultTests(unittest.TestCase):
    def needs(self, **results):
        return {name: {"result": result, "outputs": {}} for name, result in results.items()}

    def test_only_success_passes_a_required_job(self):
        self.assertTrue(require_results.evaluate(self.needs(lint="success", test="success"))[0])
        for result in ("failure", "cancelled", "skipped", None, "neutral"):
            with self.subTest(result=result):
                self.assertFalse(require_results.evaluate(self.needs(lint="success", test=result))[0])

    def test_cancelled_is_reported_as_cancelled_not_as_a_failure(self):
        passed, lines = require_results.evaluate(self.needs(test="cancelled"))
        self.assertFalse(passed)
        self.assertIn("CANCELLED", lines[0])
        self.assertNotIn("FAILED", lines[0])

    def test_only_named_conditional_lanes_may_skip(self):
        self.assertTrue(require_results.evaluate(self.needs(lint="success", forges="skipped"), ["forges"])[0])
        self.assertFalse(require_results.evaluate(self.needs(lint="skipped", forges="skipped"), ["forges"])[0])
        self.assertFalse(require_results.evaluate(self.needs(forges="failure"), ["forges"])[0])
        self.assertFalse(require_results.evaluate(self.needs(forges="cancelled"), ["forges"])[0])

    def test_empty_or_mismatched_aggregates_fail_closed(self):
        self.assertFalse(require_results.evaluate({})[0])
        self.assertFalse(require_results.evaluate(self.needs(lint="success"), ["forges"])[0])

    def test_the_command_reads_needs_json(self):
        script = Path(__file__).with_name("require_results.py")
        for needs, status in ((self.needs(a="success"), 0), (self.needs(a="cancelled"), 1), ("not json", 1)):
            env = {**os.environ, "NEEDS": needs if isinstance(needs, str) else json.dumps(needs)}
            result = subprocess.run([sys.executable, str(script)], env=env, stdout=subprocess.PIPE, text=True)
            self.assertEqual(result.returncode, status, result.stdout)


def run(run_id, event="push", sha=SHA, status="completed", attempt=1, path=await_ci.WORKFLOW_PATH):
    return {"id": run_id, "event": event, "head_sha": sha, "status": status, "run_attempt": attempt, "path": path}


class ReleaseGateTests(unittest.TestCase):
    def jobs(self, table):
        return lambda run: [{"name": name, "status": "completed", "conclusion": conclusion}
                            for name, conclusion in table.get(run["id"], [])]

    def test_only_a_same_sha_push_or_merge_group_run_with_a_green_aggregate_passes(self):
        green = {1: [("CI result", "success")]}
        for event in ("push", "merge_group"):
            with self.subTest(event=event):
                self.assertEqual(await_ci.decide([run(1, event)], self.jobs(green), SHA)[0], "passed")
        for other in (run(1, "pull_request"), run(1, sha="b" * 40), run(1, path=".github/workflows/release.yml")):
            with self.subTest(other=other):
                self.assertEqual(await_ci.decide([other], self.jobs(green), SHA)[0], "waiting")

    def test_a_green_run_whose_aggregate_did_not_succeed_fails(self):
        for conclusion in ("failure", "cancelled", "skipped"):
            with self.subTest(conclusion=conclusion):
                state, why = await_ci.decide([run(1)], self.jobs({1: [("CI result", conclusion)]}), SHA)
                self.assertEqual(state, "failed")
                self.assertIn(conclusion, why)
        self.assertEqual(await_ci.decide([run(1)], self.jobs({1: [("lint", "success")]}), SHA)[0], "failed")
        duplicate = {1: [("CI result", "success"), ("CI result", "success")]}
        self.assertEqual(await_ci.decide([run(1)], self.jobs(duplicate), SHA)[0], "failed")

    def test_running_runs_and_reruns_are_waited_for(self):
        jobs = self.jobs({1: [("CI result", "failure")]})
        self.assertEqual(await_ci.decide([run(1), run(2, status="in_progress")], jobs, SHA)[0], "waiting")
        self.assertEqual(await_ci.decide([run(1, status="queued", attempt=2)], jobs, SHA)[0], "waiting")

    def test_waiting_is_bounded_and_api_errors_are_retried(self):
        clock = iter(range(0, 10_000, 10))
        responses = [OSError("rate limited"), {"workflow_runs": []}, {"workflow_runs": [run(1)]},
                     {"jobs": [{"name": "CI result", "status": "completed", "conclusion": "success"}]}]

        def get(url):
            item = responses.pop(0)
            if isinstance(item, Exception):
                raise item
            return item

        passed, _ = await_ci.wait(get, "o/r", SHA, timeout=1000, interval=0, appear=500,
                                  clock=lambda: next(clock), sleep=lambda _: None)
        self.assertTrue(passed)
        passed, why = await_ci.wait(lambda url: {"workflow_runs": []}, "o/r", SHA, timeout=1000, interval=0,
                                    appear=50, clock=lambda: next(clock), sleep=lambda _: None)
        self.assertFalse(passed)
        self.assertIn("no CI run", why)


class ChangedPathTests(unittest.TestCase):
    def test_forge_adapters_transport_and_the_lane_definition_are_relevant(self):
        relevant = ["src/forge/gitea/live.rs", "src/forge/http/proxy.rs", "ci/forge-server.sh",
                    ".github/workflows/ci.yml", "Cargo.lock"]
        self.assertEqual(changed_paths.forge_relevant(relevant), sorted(relevant))
        self.assertEqual(changed_paths.forge_relevant(["src/app/palette.rs", "docs/ci.md", "src/forgery.rs"]), [])

    def test_an_unknown_base_runs_the_lane(self):
        for base in ("", "0" * 40, "no-such-revision-for-sure"):
            with self.subTest(base=base):
                self.assertIsNone(changed_paths.changed(base))
        with tempfile.NamedTemporaryFile("r", suffix=".out", delete=False) as output:
            pass
        try:
            changed_paths.main(["--base", "0" * 40, "--output", output.name])
            self.assertEqual(Path(output.name).read_text(), "forge=true\n")
        finally:
            os.unlink(output.name)


def report(name, attempt, processes):
    return {"schema": "gitcraft-test-report/1", "name": name, "processes": processes,
            "metadata": {"env": {"GITHUB_SHA": SHA, "GITHUB_RUN_ATTEMPT": str(attempt), "ImageOS": "win22"}}}


def process(outcome, *tests, **extra):
    return {"label": "lib.exe: core", "outcome": outcome, "log": "logs/00.log", "exit_code": extra.get("exit_code"),
            "tests": [{"binary": "lib.exe", "name": name, "outcome": result, **detail} for name, result, detail in tests],
            **extra}


class SummaryTests(unittest.TestCase):
    def classes(self, *reports):
        results, process_failures = ci_summary.classify(list(reports))
        return ({item["test"]: item["class"] for item in results}, [item["class"] for item in process_failures])

    def test_failures_are_classified_with_evidence(self):
        panic = {"location": "src/a.rs:1:2", "message": "left != right at 0x1f in C:\\tmp\\x 12"}
        tests, processes = self.classes(
            report("core", 1, [process("failed", ("a::regressed", "failed", panic), ("a::fine", "passed", {}))]),
            report("repo-rest", 1, [process("timeout", ("r::hung", "timeout", {}), ("r::queued", "not-finished", {}),
                                            running_at_timeout=["r::hung"])]),
            report("repo-s", 1, [process("crashed", ("s::x", "not-finished", {}), exit_code=3221225477)]),
            report("git-a-m", 1, [process("launch-error", error="missing.exe")]))
        self.assertEqual(tests, {"a::regressed": "deterministic-regression", "r::hung": "timeout-or-hang",
                                 "s::x": "unknown"})
        self.assertEqual(processes, ["infrastructure-or-environment", "infrastructure-or-environment"])

    def test_a_hang_after_a_failed_test_is_still_reported(self):
        panic = {"location": "src/a.rs:1:2", "message": "boom"}
        tests, processes = self.classes(
            report("repo-rest", 1, [process("timeout", ("r::failed", "failed", panic), running_at_timeout=[])]))
        self.assertEqual(tests, {"r::failed": "deterministic-regression"})
        self.assertEqual(processes, ["timeout-or-hang"])

    def test_a_passing_retry_marks_possible_flakiness_without_erasing_the_failure(self):
        panic = {"location": "src/a.rs:1:2", "message": "lock acquisition failed because the operation would block"}
        results, _ = ci_summary.classify([
            report("core", 1, [process("failed", ("a::parks", "failed", panic))]),
            report("core", 2, [process("passed", ("a::parks", "passed", {}))])])
        self.assertEqual([(item["test"], item["class"]) for item in results], [("a::parks", "possible-flaky")])
        self.assertEqual(results[0]["evidence"][0]["attempt"], "1")

    def test_signatures_ignore_numbers_addresses_and_paths(self):
        first = ci_summary.signature({"outcome": "failed", "message": "took 20.4206 s at 0xdead in /tmp/a/b"}, {"outcome": "failed"})
        second = ci_summary.signature({"outcome": "failed", "message": "took 20.496 s at 0xbeef in /tmp/c"}, {"outcome": "failed"})
        self.assertEqual(first, second)

    def test_the_command_writes_markdown_and_json_and_shows_cancelled_jobs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "reports" / "core").mkdir(parents=True)
            (root / "reports" / "core" / "report.json").write_text(json.dumps(
                report("core", 1, [process("timeout", ("a::hung", "timeout", {}))])))
            env = {**os.environ, "GITHUB_STEP_SUMMARY": str(root / "summary.md"),
                   "NEEDS": json.dumps({"windows-tests": {"result": "cancelled"}})}
            subprocess.run([sys.executable, str(Path(__file__).with_name("ci_summary.py")), str(root / "reports"),
                            "--json", str(root / "summary.json")], env=env, check=True, stdout=subprocess.DEVNULL)
            text = (root / "summary.md").read_text()
            self.assertIn("timeout-or-hang", text)
            self.assertIn("cancelled (superseded or stopped; not a failure)", text)
            self.assertEqual(json.loads((root / "summary.json").read_text())["failures"][0]["test"], "a::hung")


if __name__ == "__main__":
    unittest.main()
