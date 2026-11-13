"""Windows compilation must retain diagnostics alongside the executable inventory."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from test_workflow_gates import job_steps, nested, require


SPEC = importlib.util.spec_from_file_location(
    "windows_test_build", Path(__file__).with_name("windows-test-build.py")
)
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)
WORKFLOW = Path(__file__).resolve().parents[1] / ".github/workflows/windows-tests.yml"
BUILD_COMMAND = "python ci/windows-test-build.py"


def verify_binaries_upload(text):
    """The artifact must carry exactly the files the build step listed, not a guessed glob."""
    steps = job_steps(text, "windows-build")
    builds = [step for step in steps if step.get("run", ("",))[0] == BUILD_COMMAND]
    require(len(builds) == 1, "the build job must run the test build exactly once")
    step_id = builds[0].get("id", ("",))[0]
    require(bool(step_id), "the test build step needs an id for its upload-paths output")
    uploads = [step for step in steps if step.get("uses", ("",))[0] == "actions/upload-artifact@v4"
               and nested(step, "with", 10).get("name", ("",))[0] == "windows-test-binaries"]
    require(len(uploads) == 1, "the build job must upload windows-test-binaries exactly once")
    options = nested(uploads[0], "with", 10)
    require(options.get("if-no-files-found", ("",))[0] == "error",
            "a missing test binary must fail the upload")
    scalar, body = options.get("path", ("", ""))
    require(scalar == "|", "unsupported upload path layout")
    paths = [line.strip() for line in body.splitlines()
             if line.strip() and not line.lstrip().startswith("#")]
    require(f"${{{{ steps.{step_id}.outputs.upload-paths }}}}" in paths,
            "the upload must include the build step's upload-paths output")


def build_dir_executable(root, name="app-0123abcd.exe"):
    # Cargo's build-dir layout (stable Rust 1.100 on windows-latest) no longer puts test
    # executables in target/debug/deps/.
    binary = root / "target/debug/build/app/0123abcd/out" / name
    binary.parent.mkdir(parents=True)
    binary.touch()
    return binary


class WindowsTestBuildTests(unittest.TestCase):
    def run_build(self, root, messages, status, github_output=None):
        metadata = {"packages": [{"id": "app", "manifest_path": str(root / "Cargo.toml")}]}
        proc = SimpleNamespace(
            stdout=io.StringIO("\n".join(json.dumps(message) for message in messages)),
            wait=Mock(return_value=status),
        )
        output = io.StringIO()
        environment = {key: value for key, value in os.environ.items() if key != "GITHUB_OUTPUT"}
        if github_output is not None:
            environment["GITHUB_OUTPUT"] = str(github_output)
        with (
            patch.dict(os.environ, environment, clear=True),
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

    def test_build_dir_layout_executables_are_recorded_and_listed_for_upload(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "ci").mkdir()
            binary = build_dir_executable(root)
            github_output = root / "github-output"
            github_output.write_text("earlier=kept\n")
            result, _ = self.run_build(root, [
                {"reason": "compiler-artifact", "profile": {"test": True},
                 "executable": str(binary), "package_id": "app"},
            ], 0, github_output)
            self.assertEqual(result, 0)
            path = "target/debug/build/app/0123abcd/out/app-0123abcd.exe"
            manifest = json.loads((root / "ci/test-binaries.json").read_text())
            self.assertEqual(manifest, {"binaries": [{"path": path, "cwd": "."}]})
            self.assertEqual(github_output.read_text(), (
                "earlier=kept\n"
                "upload-paths<<GITCRAFT_UPLOAD_PATHS\n"
                f"{path}\nci/test-binaries.json\n"
                "GITCRAFT_UPLOAD_PATHS\n"
            ))

    def test_upload_paths_are_not_written_outside_actions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "ci").mkdir()
            binary = build_dir_executable(root)
            result, _ = self.run_build(root, [
                {"reason": "compiler-artifact", "profile": {"test": True},
                 "executable": str(binary), "package_id": "app"},
            ], 0)
            self.assertEqual(result, 0)
            self.assertEqual(sorted(p.name for p in root.iterdir()), ["ci", "target"])

    def test_unrendered_diagnostic_remains_visible(self):
        with tempfile.TemporaryDirectory() as temporary:
            message = {"reason": "compiler-message", "message": {"message": "unrendered error"}}
            result, output = self.run_build(Path(temporary).resolve(), [message], 1)
            self.assertEqual(result, 1)
            self.assertIn(json.dumps(message), output)


class WindowsBinariesUploadTests(unittest.TestCase):
    """Run 43403186250: test executables moved to the build-dir layout, the upload glob
    only matched target/debug/deps/*.exe, and every shard found its binaries missing."""

    def test_workflow_uploads_the_build_steps_exact_file_list(self):
        verify_binaries_upload(WORKFLOW.read_text(encoding="utf-8"))

    def test_glob_only_or_unconnected_uploads_are_rejected(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        output = "${{ steps.build.outputs.upload-paths }}"
        for mutation in (
            text.replace(output, "target/debug/deps/*.exe"),
            text.replace(output, "${{ steps.other.outputs.upload-paths }}"),
            text.replace("        id: build\n", ""),
            text.replace("if-no-files-found: error", "if-no-files-found: warn"),
        ):
            self.assertNotEqual(mutation, text)
            with self.subTest(), self.assertRaises(ValueError):
                verify_binaries_upload(mutation)


if __name__ == "__main__":
    unittest.main()
