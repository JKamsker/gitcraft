"""Workflow triggers, conditional jobs, concurrency and release gating (issue #211).

Uses the constrained reader of ci/test_workflow_gates.py; unsupported layouts fail
closed. Concurrency and `if:` expressions are evaluated for concrete events by a
small evaluator of the expression subset these workflows use.
"""

from pathlib import Path
import re
import sys
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import await_ci  # noqa: E402
from test_workflow_gates import dependencies, mapping, nested  # noqa: E402

WORKFLOWS = Path(__file__).resolve().parents[1] / ".github/workflows"
SHA_A, SHA_B = "a" * 40, "b" * 40


def read(name):
    return (WORKFLOWS / name).read_text(encoding="utf-8")


def jobs_of(text):
    return {name: mapping(body, 4) for name, (_, body) in nested(mapping(text, 0), "jobs", 2).items()}


def scalar(fields, key):
    return fields.get(key, ("",))[0]


def evaluate(expression, github, needs=None):
    """Evaluate `${{ … }}` templates (or a bare `if:` expression) for one event."""
    def value(source):
        source = source.strip()
        python = re.sub(r"\bformat\(", "_format(", source)
        python = python.replace("&&", " and ").replace("||", " or ")
        python = re.sub(r"!(?!=)", " not ", python)
        python = re.sub(r"\bgithub\.([A-Za-z_.]+)", lambda m: f"_github({m.group(1)!r})", python)
        python = re.sub(r"\bneeds\.([A-Za-z_-]+)\.outputs\.([A-Za-z_-]+)",
                        lambda m: f"_needs({m.group(1)!r}, {m.group(2)!r})", python)
        python = re.sub(r"\bstartsWith\(", "_starts(", python)
        require = {"_format": lambda text, *args: text.format(*args),
                   "_github": lambda path: github.get(path, ""),
                   "_needs": lambda job, key: (needs or {}).get(job, {}).get(key, ""),
                   "_starts": lambda text, prefix: str(text).startswith(prefix), "true": True, "false": False}
        return eval(python, {"__builtins__": {}}, require)  # noqa: S307 - fixed workflow text only

    if "${{" not in expression:
        return value(expression)
    parts = re.split(r"\$\{\{(.*?)\}\}", expression)
    if len(parts) == 3 and not parts[0] and not parts[2]:
        return value(parts[1])
    return "".join(part if index % 2 == 0 else str(value(part)) for index, part in enumerate(parts))


def event(name, ref, sha=SHA_A, **extra):
    return {"event_name": name, "ref": ref, "sha": sha, **extra}


class TriggerTests(unittest.TestCase):
    def test_ci_runs_for_prs_merge_queue_dev_pushes_tags_nightly_and_by_hand(self):
        triggers = nested(mapping(read("ci.yml"), 0), "on", 2)
        self.assertEqual(set(triggers), {"push", "pull_request", "merge_group", "schedule", "workflow_dispatch"})
        push = nested(triggers, "push", 4)
        self.assertIn("dev", scalar(push, "branches"))
        self.assertIn('"v*"', scalar(push, "tags"))
        self.assertEqual(scalar(nested(triggers, "merge_group", 4), "types"), "[checks_requested]")

    def test_shared_workflows_are_reusable_only(self):
        for name in ("package.yml", "windows-tests.yml", "rust-file-size.yml"):
            with self.subTest(workflow=name):
                self.assertEqual(set(nested(mapping(read(name), 0), "on", 2)), {"workflow_call"})


class ConcurrencyTests(unittest.TestCase):
    def setUp(self):
        concurrency = nested(mapping(read("ci.yml"), 0), "concurrency", 2)
        self.group, self.cancel = scalar(concurrency, "group"), scalar(concurrency, "cancel-in-progress")

    def outcome(self, github):
        return evaluate(self.group, github), evaluate(self.cancel, github)

    def test_a_newer_pr_push_cancels_only_the_same_pr(self):
        first, cancel = self.outcome(event("pull_request", "refs/pull/7/merge", SHA_A))
        second, _ = self.outcome(event("pull_request", "refs/pull/7/merge", SHA_B))
        other, _ = self.outcome(event("pull_request", "refs/pull/8/merge", SHA_A))
        self.assertTrue(cancel)
        self.assertEqual(first, second)
        self.assertNotEqual(first, other)

    def test_merge_queue_candidates_pushes_and_nightlies_never_cancel_each_other(self):
        cases = [event("merge_group", "refs/heads/gh-readonly-queue/dev/pr-7-" + SHA_A, SHA_A),
                 event("merge_group", "refs/heads/gh-readonly-queue/dev/pr-8-" + SHA_B, SHA_B),
                 event("push", "refs/heads/dev", SHA_A), event("push", "refs/heads/dev", SHA_B),
                 event("push", "refs/tags/v1.0.0", SHA_A), event("schedule", "refs/heads/dev", SHA_A),
                 event("workflow_dispatch", "refs/heads/dev", SHA_A)]
        groups = []
        for github in cases:
            group, cancel = self.outcome(github)
            with self.subTest(github=github):
                self.assertFalse(cancel)
            groups.append(group)
        self.assertEqual(len(set(groups)), len(groups), groups)

    def test_the_old_ref_only_group_is_rejected_for_merge_queues(self):
        old = "ci-${{ github.ref }}"
        first = evaluate(old, event("push", "refs/heads/dev", SHA_A))
        second = evaluate(old, event("push", "refs/heads/dev", SHA_B))
        self.assertEqual(first, second)  # the pattern the issue reported as cancelling post-merge runs
        self.assertNotEqual(self.group, old)


class CiJobTests(unittest.TestCase):
    def setUp(self):
        self.jobs = jobs_of(read("ci.yml"))

    def test_linux_tests_cover_the_release_runner_and_the_latest_ubuntu(self):
        test = self.jobs["test"]
        self.assertEqual(scalar(test, "name"), "test (${{ matrix.os }})")
        matrix = nested(nested(test, "strategy", 6), "matrix", 8)
        self.assertEqual(scalar(matrix, "os"), "[ubuntu-22.04, ubuntu-latest]")
        self.assertNotIn("matrix.os ==", test["steps"][1])
        package = jobs_of(read("package.yml"))["build"]
        self.assertIn("os: ubuntu-22.04", package["strategy"][1])

    def test_the_stable_windows_and_aggregate_check_names_are_kept(self):
        self.assertEqual(scalar(self.jobs["windows-test-result"], "name"), "test (windows-latest)")
        self.assertEqual(scalar(self.jobs["ci-result"], "name"), await_ci.REQUIRED_JOB)

    def test_pull_requests_run_the_release_packaging_with_a_prerelease_version(self):
        smoke, version = self.jobs["package-smoke"], self.jobs["package-version"]
        self.assertEqual(scalar(smoke, "uses"), "./.github/workflows/package.yml")
        self.assertNotIn("if", smoke)
        self.assertNotIn("if", version)
        self.assertIn("ci/release_version.py compute --dev true", version["steps"][1])
        release = jobs_of(read("release.yml"))
        self.assertEqual(scalar(release["build"], "uses"), "./.github/workflows/package.yml")
        package = read("package.yml")
        self.assertIn('python ci/release_version.py apply "$VERSION"', package)
        self.assertIn('rpm_version="$(python ci/release_version.py rpm "$VERSION")"', package)
        self.assertNotIn("tr '-' '~'", package)

    def test_the_forge_lane_runs_for_forge_changes_nightly_and_by_hand_only(self):
        condition = scalar(self.jobs["forges"], "if")
        self.assertEqual(dependencies(self.jobs["forges"]), {"rust-file-size", "changes"})
        for github, forge, expected in ((event("pull_request", "r"), "true", True),
                                        (event("pull_request", "r"), "false", False),
                                        (event("merge_group", "r"), "false", False),
                                        (event("push", "refs/heads/dev"), "true", True),
                                        (event("schedule", "r"), "false", True),
                                        (event("workflow_dispatch", "r"), "false", True)):
            with self.subTest(github=github, forge=forge):
                self.assertEqual(bool(evaluate(condition, github, {"changes": {"forge": forge}})), expected)

    def test_reports_upload_even_when_tests_fail_or_time_out(self):
        windows = read("windows-tests.yml")
        upload = windows[windows.index("- name: Upload test report and diagnostics"):]
        self.assertIn("if: always()", upload.split("\n      - ", 1)[0])
        self.assertIn("path: ci-reports/", upload)
        self.assertIn("--report-dir ci-reports", windows)
        linux = self.jobs["test"]["steps"][1]
        upload = linux[linux.index("- name: Upload test reports"):]
        self.assertIn("if: always()", upload)
        self.assertEqual(linux.count("python ci/libtest_watchdog.py"), 3)
        self.assertIn("attempt-${{ github.run_attempt }}", upload)


class ReleaseGateTests(unittest.TestCase):
    def setUp(self):
        self.jobs = jobs_of(read("release.yml"))

    def test_publishing_needs_ci_on_the_same_commit(self):
        release, gate = self.jobs["release"], self.jobs["ci-gate"]
        self.assertTrue({"ci-gate", "build", "version"} <= dependencies(release))
        self.assertIn('run: python3 ci/await_ci.py --repo "$GITHUB_REPOSITORY" --sha "$GITHUB_SHA"', gate["steps"][1])
        self.assertNotIn("continue-on-error", gate)
        self.assertIn("actions: read", gate["permissions"][1])

    def test_no_event_publishes_without_the_gate_running(self):
        gate_if, release_if = scalar(self.jobs["ci-gate"], "if"), scalar(self.jobs["release"], "if")
        for github in (event("push", "refs/heads/dev"), event("push", "refs/tags/v1.0.0"),
                       event("workflow_dispatch", "refs/heads/dev"), event("workflow_dispatch", "refs/tags/v1.0.0")):
            for prerelease in ("true", "false"):
                publishes = evaluate(release_if, github, {"version": {"prerelease": prerelease}})
                with self.subTest(github=github, prerelease=prerelease):
                    if publishes:
                        self.assertTrue(evaluate(gate_if, github))
        self.assertFalse(re.search(r"\b(?:always|cancelled|failure)\s*\(", release_if))

    def test_release_does_not_repeat_the_ci_suites(self):
        self.assertNotIn("windows-tests", self.jobs)
        self.assertNotIn("cargo test", read("release.yml"))
        self.assertNotIn("cargo test", read("package.yml"))


if __name__ == "__main__":
    unittest.main()
