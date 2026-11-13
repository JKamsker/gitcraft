"""Watchdog, report and owned-process-tree checks for ci/libtest_watchdog.py (issue #211).

Synthetic output and process snapshots are checked first; the real-process tests
only start children of this test and stop them through their owned container.
"""

from collections import Counter
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
import xml.etree.ElementTree as ElementTree

sys.path.insert(0, str(Path(__file__).parent))
import libtest_watchdog as watchdog  # noqa: E402
import process_tree  # noqa: E402

spec = importlib.util.spec_from_file_location("shards", Path(__file__).with_name("test-shards.py"))
shards = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shards)


def feed(parser, text):
    for number, line in enumerate(textwrap.dedent(text).strip("\n").splitlines()):
        parser.feed(line, float(number))
    return parser


def alive(pid):
    if sys.platform == "win32":
        listing = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH"], stdout=subprocess.PIPE, text=True)
        return str(pid) in listing.stdout
    state = subprocess.run(["ps", "-p", str(pid), "-o", "stat="], stdout=subprocess.PIPE, text=True).stdout.strip()
    return bool(state) and not state.startswith("Z")


class ParserTests(unittest.TestCase):
    def test_parallel_serial_and_interleaved_results(self):
        parser = feed(watchdog.LibtestParser("lib.exe"), """
            running 4 tests
            test a::passes ... ok
            test a::fails ... FAILED
            test a::serial ...
            some --nocapture output
            ok
            test a::skipped ... ignored, needs a server
        """)
        outcomes = {name: entry["outcome"] for (_, name), entry in parser.tests.items()}
        self.assertEqual(outcomes, {"a::passes": "passed", "a::fails": "failed",
                                    "a::serial": "passed", "a::skipped": "ignored"})
        self.assertEqual(parser.tests[("lib.exe", "a::serial")]["duration"], 2.0)

    def test_panics_are_attributed_only_to_test_named_threads(self):
        parser = feed(watchdog.LibtestParser("lib.exe"), """
            thread 'a::new_style' panicked at src/a.rs:10:5:
            assertion `left == right` failed
            thread '<unnamed>' panicked at src/worker.rs:1:1:
            worker died
            thread 'a::old_style' panicked at 'boom', src/b.rs:3:9
            test a::new_style ... FAILED
        """)
        self.assertEqual(parser.messages[("lib.exe", "a::new_style")],
                         ("src/a.rs:10:5", "assertion `left == right` failed"))
        self.assertEqual(parser.messages[("lib.exe", "a::old_style")], ("src/b.rs:3:9", "boom"))
        self.assertNotIn(("lib.exe", "<unnamed>"), parser.tests)

    def test_cargo_binaries_doctests_and_long_running_notices(self):
        parser = feed(watchdog.LibtestParser(), """
                 Running unittests src/lib.rs (target/debug/deps/gitcraft-0123abcd)
            test x::one ... ok
            test x::slow has been running for over 60 seconds
               Doc-tests gitcraft
            test src/lib.rs - f (line 3) ... ok
        """)
        self.assertIn(("gitcraft-0123abcd", "x::one"), parser.tests)
        self.assertIn(("doctests:gitcraft", "src/lib.rs - f (line 3)"), parser.tests)
        self.assertEqual(parser.running(), [("gitcraft-0123abcd", "x::slow")])

    def test_timeouts_name_running_and_unfinished_tests_and_exit_zero_stays_authoritative(self):
        parser = feed(watchdog.LibtestParser("lib.exe"), """
            test a::done ... ok
            test a::stuck has been running for over 120 seconds
        """)
        expected = Counter({"a::done": 1, "a::stuck": 1, "a::queued": 1})
        tests = {test["name"]: test["outcome"] for test in watchdog.tests_of(parser, expected, "deadline", None)}
        self.assertEqual(tests, {"a::done": "passed", "a::stuck": "timeout", "a::queued": "not-finished"})
        tests = {test["name"]: test["outcome"] for test in watchdog.tests_of(parser, expected, None, 0)}
        self.assertEqual(tests["a::queued"], "unknown")
        self.assertEqual(watchdog.process_outcome(None, 0, []), "passed")
        self.assertEqual(watchdog.process_outcome("stall", None, []), "stalled")
        self.assertEqual(watchdog.process_outcome(None, 101, [{"outcome": "failed"}]), "failed")
        self.assertEqual(watchdog.process_outcome(None, 3221225477, [{"outcome": "not-finished"}]), "crashed")

    def test_secrets_are_redacted(self):
        text = "token ghp_" + "a" * 30 + " Authorization: Bearer abc https://user:pw@example.com/x"
        redacted = watchdog.redact(text)
        for secret in ("ghp_aaaa", "Bearer abc", ":pw@"):
            self.assertNotIn(secret, redacted)


class SnapshotTests(unittest.TestCase):
    def test_ps_rows_are_limited_to_the_owned_group_and_ordered_as_a_tree(self):
        listing = """
          10     1    10 00:05 S  python fake
          12    10    10 00:04 S  git status --porcelain
          13    12    10 00:04 S  git-remote-http origin
          99     1    99 00:01 S  unrelated
          14     1    10 00:03 S  orphaned helper
        """
        rows = process_tree.tree_order(process_tree.parse_ps(listing, 10), 10)
        self.assertEqual([(row["pid"], row["depth"]) for row in rows], [(10, 0), (12, 1), (13, 2), (14, 0)])
        self.assertTrue(rows[-1]["orphaned"])
        self.assertNotIn(99, [row["pid"] for row in rows])

    def test_windows_cim_rows_are_limited_to_job_members(self):
        text = json.dumps([{"ProcessId": 5, "ParentProcessId": 4, "Name": "git.exe", "CommandLine": "git log"},
                           {"ProcessId": 6, "ParentProcessId": 1, "Name": "other.exe"}])
        self.assertEqual([row["pid"] for row in process_tree.parse_cim(text, {5})], [5])
        self.assertEqual(process_tree.parse_cim(json.dumps({"ProcessId": 5}), {5})[0]["pid"], 5)


FAKE = textwrap.dedent('''
    import subprocess, sys, time
    mode = sys.argv[1]
    print("running 3 tests", flush=True)
    print("test a::one ... ok", flush=True)
    if mode == "hang":
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(120)"])
        print("grandchild", child.pid, flush=True)
        print("test a::two has been running for over 60 seconds", flush=True)
        time.sleep(120)
    if mode == "stall":
        time.sleep(120)
    if mode == "leak":
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(120)"])
        print("grandchild", child.pid, flush=True)
    print("test a::two ... ok", flush=True)
    print("token ghp_" + "b" * 30, flush=True)
    if mode == "fail":
        print("thread 'a::three' panicked at src/x.rs:1:2:", flush=True)
        print("it broke", flush=True)
        print("test a::three ... FAILED", flush=True)
        sys.exit(101)
    print("test a::three ... ok", flush=True)
''')


class RealProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.script = self.root / "fake.py"
        self.script.write_text(FAKE)
        self.report = watchdog.Report("unit", self.root / "report")
        self.expected = Counter({"a::one": 1, "a::two": 1, "a::three": 1})

    def tearDown(self):
        self.temp.cleanup()

    def run_fake(self, mode, **limits):
        echo = io.BytesIO()
        record = watchdog.run_process(self.report, f"fake {mode}", [sys.executable, str(self.script), mode],
                                      binary="fake", expected=self.expected, heartbeat=0.5, echo=echo, **limits)
        return record, echo.getvalue().decode()

    def grandchild(self, output):
        return int(next(line.split()[1] for line in output.splitlines() if line.startswith("grandchild")))

    def test_a_passing_process_writes_json_and_junit_reports(self):
        record, output = self.run_fake("pass", deadline=60)
        self.assertEqual(record["outcome"], "passed")
        self.assertIn("test a::three ... ok", output)
        self.report.finish("passed")
        data = json.loads((self.root / "report" / "report.json").read_text())
        self.assertEqual(data["schema"], watchdog.SCHEMA)
        self.assertIn("git", data["metadata"])
        self.assertEqual([test["outcome"] for test in data["processes"][0]["tests"]], ["passed"] * 3)
        suite = ElementTree.parse(self.root / "report" / "junit.xml").getroot()[0]
        self.assertEqual(suite.get("tests"), "3")
        log = (self.root / "report" / record["log"]).read_text()
        self.assertIn("[REDACTED]", log)
        self.assertNotIn("ghp_bbbb", log)

    def test_a_failing_test_is_reported_with_its_panic(self):
        record, _ = self.run_fake("fail", deadline=60)
        self.assertEqual(record["outcome"], "failed")
        failed = [test for test in record["tests"] if test["outcome"] == "failed"]
        self.assertEqual([(test["name"], test["message"]) for test in failed], [("a::three", "it broke")])
        suite = ElementTree.fromstring(watchdog.junit(self.report.data))[0]
        self.assertEqual(suite.get("failures"), "1")

    def test_a_deadline_records_and_stops_the_whole_owned_tree(self):
        started = time.monotonic()
        record, output = self.run_fake("hang", deadline=4)
        self.assertLess(time.monotonic() - started, 60)
        self.assertEqual(record["outcome"], "timeout")
        outcomes = {test["name"]: test["outcome"] for test in record["tests"]}
        self.assertEqual(outcomes, {"a::one": "passed", "a::two": "timeout", "a::three": "not-finished"})
        self.assertEqual(record["running_at_timeout"], ["a::two"])
        grandchild = self.grandchild(output)
        self.assertIn(grandchild, [row["pid"] for row in record["snapshot"]["processes"]])
        for _ in range(50):
            if not alive(grandchild):
                break
            time.sleep(0.1)
        self.assertFalse(alive(grandchild))
        active = json.loads((self.root / "report" / "report.json").read_text())
        self.assertNotIn("active", active)

    def test_no_progress_is_stopped_before_the_deadline(self):
        record, _ = self.run_fake("stall", deadline=60, stall=2)
        self.assertEqual(record["outcome"], "stalled")
        self.assertLess(record["duration"], 30)

    def test_descendants_holding_output_open_are_stopped_without_failing_a_pass(self):
        grace, watchdog.OUTPUT_GRACE_SECONDS = watchdog.OUTPUT_GRACE_SECONDS, 1
        try:
            record, output = self.run_fake("leak", deadline=60)
        finally:
            watchdog.OUTPUT_GRACE_SECONDS = grace
        self.assertEqual(record["outcome"], "passed")
        self.assertTrue(record["warnings"])
        grandchild = self.grandchild(output)
        for _ in range(50):
            if not alive(grandchild):
                break
            time.sleep(0.1)
        self.assertFalse(alive(grandchild))

    def test_a_missing_executable_is_a_launch_error(self):
        record = watchdog.run_process(self.report, "missing", [str(self.root / "missing.exe")],
                                      deadline=5, echo=io.BytesIO())
        self.assertEqual(record["outcome"], "launch-error")

    def test_the_command_line_wrapper_returns_the_failure(self):
        status = subprocess.run([sys.executable, str(Path(__file__).with_name("libtest_watchdog.py")),
                                 "--name", "cli", "--report-dir", str(self.root / "cli"), "--deadline", "60",
                                 "--", sys.executable, str(self.script), "fail"],
                                stdout=subprocess.DEVNULL).returncode
        self.assertEqual(status, 101)
        data = json.loads((self.root / "cli" / "report.json").read_text())
        self.assertEqual(data["outcome"], "failed")


FAKE_LIBTEST = textwrap.dedent('''
    #!PYTHON
    import os, sys, time
    TESTS = ["repo::stack::tests::hangs", "repo::stack::tests::passes", "repo::sync::tests::passes"]
    args = sys.argv[1:]
    skips = [args[i + 1] for i, arg in enumerate(args) if arg == "--skip"]
    filters = [arg for i, arg in enumerate(args) if not arg.startswith("--") and (i == 0 or args[i - 1] != "--skip")]
    exact = "--exact" in args
    def selected(name):
        if any(skip in name for skip in skips):
            return False
        return not filters or any(name == f if exact else f in name for f in filters)
    chosen = [name for name in TESTS if selected(name)]
    if "--list" in args:
        for name in chosen:
            print(name + ": test")
        sys.exit(0)
    print("running %d tests" % len(chosen), flush=True)
    for name in chosen:
        if name == os.environ.get("FAKE_HANG"):
            time.sleep(120)
        print("test %s ... ok" % name, flush=True)
''')


@unittest.skipIf(sys.platform == "win32", "the fake libtest binary is a POSIX script")
class ShardRunTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.binary = self.root / "fake-libtest"
        self.binary.write_text(FAKE_LIBTEST.replace("PYTHON", sys.executable, 1).lstrip())
        self.binary.chmod(0o755)
        isolated = shards.ISOLATED["repo-s"]
        shards.ISOLATED["repo-s"] = ["repo::stack::tests::hangs"]
        shards.SHARDS["repo-s"] = [*shards.REPO_S, "--skip", "git::", "--skip", "repo::stack::tests::hangs"]
        self.addCleanup(shards.ISOLATED.__setitem__, "repo-s", isolated)
        self.addCleanup(shards.SHARDS.__setitem__, "repo-s",
                        [*shards.REPO_S, "--skip", "git::", *[a for t in isolated for a in ("--skip", t)]])

    def tearDown(self):
        os.environ.pop("FAKE_HANG", None)
        self.temp.cleanup()

    def test_a_shard_reports_every_process_and_a_hung_isolated_test_by_name(self):
        binaries = [(self.binary, self.root)]
        processes = list(shards.shard_processes(binaries, "repo-s"))
        self.assertEqual([kind for _, _, _, kind, _, _ in processes], ["parallel", "isolated"])
        self.assertEqual(shards.run_shard(binaries, "repo-s", self.root / "ok", budget=60, stall=30), 0)
        os.environ["FAKE_HANG"] = "repo::stack::tests::hangs"
        self.assertNotEqual(shards.run_shard(binaries, "repo-s", self.root / "hang", budget=60, stall=2), 0)
        data = json.loads((self.root / "hang" / "report.json").read_text())
        self.assertEqual(data["outcome"], "stalled")
        isolated = data["processes"][-1]
        self.assertEqual(isolated["kind"], "isolated")
        self.assertEqual([(t["name"], t["outcome"]) for t in isolated["tests"]],
                         [("repo::stack::tests::hangs", "timeout")])
        self.assertTrue(isolated["snapshot"]["processes"])


class ShardBudgetTests(unittest.TestCase):
    def test_the_shard_budget_ends_before_the_actions_step_timeout(self):
        workflow = (Path(__file__).parents[1] / ".github/workflows/windows-tests.yml").read_text()
        step = workflow[workflow.index("- name: Run precompiled test shard"):]
        minutes = int(step.split("timeout-minutes:", 1)[1].split()[0])
        self.assertLessEqual(shards.SHARD_BUDGET_SECONDS + 60, minutes * 60)
        self.assertLess(shards.STALL_SECONDS, shards.SHARD_BUDGET_SECONDS)


if __name__ == "__main__":
    unittest.main()
