"""Recording-only ownership and bounded-command checks for stack capture."""
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock, patch

import windows_dump


class DumpTests(unittest.TestCase):
    def test_exited_owned_child_is_never_opened(self):
        child = Mock(pid=123)
        child.poll.return_value = 0
        with patch.object(windows_dump.sys, "platform", "win32"), patch.object(windows_dump.subprocess, "run") as run:
            self.assertEqual(windows_dump.capture(child, Path("stacks.dmp"))["status"], "unavailable")
            run.assert_not_called()

    def test_only_direct_owned_pid_is_passed_with_a_bounded_wait(self):
        child = Mock(pid=123)
        child.poll.return_value = None
        with patch.object(windows_dump.sys, "platform", "win32"), patch.object(windows_dump.subprocess, "run") as run:
            run.return_value = Mock(returncode=0, stderr="")
            result = windows_dump.capture(child, Path("stacks.dmp"))
            self.assertEqual(result["status"], "captured")
            args, kwargs = run.call_args
            self.assertEqual(args[0][-2:], ["123", "stacks.dmp"])
            self.assertEqual(kwargs["timeout"], 10)

    def test_dump_timeout_is_reported_without_masking_the_test_timeout(self):
        child = Mock(pid=123)
        child.poll.return_value = None
        with patch.object(windows_dump.sys, "platform", "win32"), patch.object(windows_dump.subprocess, "run", side_effect=subprocess.TimeoutExpired("dump", 10)):
            self.assertEqual(windows_dump.capture(child, Path("stacks.dmp"))["status"], "failed")

class DeadlineMarkerTests(unittest.TestCase):
    def test_marker_captures_owned_child_then_stops_the_shard(self):
        import io
        import tempfile
        import libtest_watchdog as watchdog
        child = Mock(pid=123, warnings=[])
        child.stdout = io.BytesIO(b"[hang-deadline lane] active stages: [poll]\n")
        child.poll.return_value = None
        child.wait.return_value = 1
        child.snapshot.return_value = {"processes": [{"pid": 123}]}
        child.kill_tree.return_value = "owned tree"
        with tempfile.TemporaryDirectory() as directory, patch.object(watchdog, "OwnedProcess", return_value=child), patch.object(watchdog, "capture_dump", return_value={"status": "captured"}) as dump:
            report = watchdog.Report("synthetic", directory)
            record = watchdog.run_process(report, "lane", ["fake"], echo=io.BytesIO(), expected={"lane": 1})
            self.assertEqual(record["outcome"], "timeout")
            self.assertIn("active stages", record["test_deadline"])
            self.assertEqual(record["snapshot"]["processes"], [{"pid": 123}])
            dump.assert_called_once_with(child, Path(directory) / "00-stacks.dmp")
            child.kill_tree.assert_called_once()

@unittest.skipUnless(windows_dump.sys.platform == "win32", "native Windows minidump")
class NativeDumpTests(unittest.TestCase):
    def test_owned_child_dump_contains_thread_stacks(self):
        import sys
        import tempfile
        from process_tree import OwnedProcess
        with tempfile.TemporaryDirectory() as directory:
            child = OwnedProcess([sys.executable, "-c", "import time; time.sleep(30)"])
            try:
                path = Path(directory) / "stacks.dmp"
                result = windows_dump.capture(child, path)
                self.assertEqual(result["status"], "captured", result)
                self.assertEqual(path.read_bytes()[:4], b"MDMP")
            finally:
                child.kill_tree()
                child.wait(timeout=30)
                child.close()
