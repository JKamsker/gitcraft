"""Windows compilation must retain diagnostics alongside the executable inventory."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


SPEC = importlib.util.spec_from_file_location(
    "windows_test_build", Path(__file__).with_name("windows-test-build.py")
)
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class WindowsTestBuildTests(unittest.TestCase):
    def run_build(self, root, messages, status):
        metadata = {"packages": [{"id": "app", "manifest_path": str(root / "Cargo.toml")}]}
        proc = SimpleNamespace(
            stdout=io.StringIO("\n".join(json.dumps(message) for message in messages)),
            wait=Mock(return_value=status),
        )
        output = io.StringIO()
        with (
            patch.object(Path, "cwd", return_value=root),
            patch.object(BUILD.subprocess, "check_output", return_value=json.dumps(metadata)),
            patch.object(BUILD.subprocess, "Popen", return_value=proc),
            contextlib.redirect_stdout(output),
        ):
            result = BUILD.main()
        proc.wait.assert_called_once_with()
        return result, output.getvalue()

    def test_failed_compile_prints_rendered_error_and_retains_failure_status(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            diagnostic = "error[E0432]: unresolved import\n --> src/lib.rs:4:5\n"
            result, output = self.run_build(root, [{
                "reason": "compiler-message", "message": {"rendered": diagnostic}
            }], 101)
            self.assertEqual(result, 101)
            self.assertIn(diagnostic, output)
            self.assertFalse((root / "ci/test-binaries.json").exists())

    def test_warning_does_not_discard_successful_executable_inventory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "ci").mkdir()
            binary = root / "target/debug/deps/app.exe"
            binary.parent.mkdir(parents=True)
            binary.touch()
            result, output = self.run_build(root, [
                {"reason": "compiler-message", "message": {"rendered": "warning: fixture\n"}},
                {"reason": "compiler-artifact", "profile": {"test": True},
                 "executable": str(binary), "package_id": "app"},
            ], 0)
            self.assertEqual(result, 0)
            self.assertIn("warning: fixture\n", output)
            manifest = json.loads((root / "ci/test-binaries.json").read_text())
            self.assertEqual(manifest, {"binaries": [{
                "path": "target/debug/deps/app.exe", "cwd": "."
            }]})

    def test_unrendered_diagnostic_remains_visible(self):
        with tempfile.TemporaryDirectory() as temporary:
            message = {"reason": "compiler-message", "message": {"message": "unrendered error"}}
            result, output = self.run_build(Path(temporary).resolve(), [message], 1)
            self.assertEqual(result, 1)
            self.assertIn(json.dumps(message), output)


if __name__ == "__main__":
    unittest.main()
