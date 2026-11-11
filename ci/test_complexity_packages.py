"""Package-discovery regressions and a real compiler coverage negative control."""

import contextlib
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from clippy_packages import RECORDING_MANIFEST, lint_packages, packages


class Repository:
    def __init__(self, root):
        self.root = root
        self.git("init", "-q")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], stderr=subprocess.PIPE)

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        self.git("add", "--", relative)

    def package(self, relative, name, extra=""):
        self.write(relative, f'[package]\nname="{name}"\nversion="0.0.0"\nedition="2024"\n'
                   '[lints.clippy]\ncognitive_complexity="forbid"\n' + extra)


class PackageRunnerTests(unittest.TestCase):
    def test_every_package_is_selected_including_independent_and_nondefault_members(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Repository(Path(temp))
            repo.package("Cargo.toml", "app", '[workspace]\nmembers=["member"]\ndefault-members=["."]\n')
            repo.package("member/Cargo.toml", "member")
            repo.package("independent space/Cargo.toml", "independent", "[workspace]\n")
            repo.package(RECORDING_MANIFEST, "recording", "[workspace]\n")
            repo.write("virtual/Cargo.toml", "[workspace]\n")
            repo.write("notCargo.toml", "invalid ignored TOML")
            calls = []
            with contextlib.redirect_stdout(io.StringIO()):
                lint_packages(repo.root, lambda command, **kwargs: calls.append((command, kwargs)))
            self.assertEqual(len(calls), 4)
            actual = {}
            for command, options in calls:
                self.assertEqual(command[:4], ["cargo", "clippy", "--locked", "--manifest-path"])
                self.assertEqual(options, {"cwd": repo.root, "check": True})
                self.assertEqual(command[5:7], ["--package", dict(packages(repo.root))[command[4]]])
                self.assertEqual(command[7:11], ["--all-targets", "--", "-D", "warnings"])
                actual[command[4]] = command[11:]
            self.assertEqual(actual.pop(RECORDING_MANIFEST), ["--cfg", "recovery_recording"])
            self.assertTrue(all(flags == [] for flags in actual.values()))
            # A fifth newly tracked package needs no edits to an allowlist or workflow.
            repo.package("future/Cargo.toml", "future", "[workspace]\n")
            self.assertEqual(len(packages(repo.root)), 5)

    def test_manifest_failures_are_detected_before_any_compiler_command(self):
        for invalid in ("deleted", "malformed", "missing name", "file link", "directory link"):
            with self.subTest(invalid=invalid), tempfile.TemporaryDirectory() as temp:
                repo = Repository(Path(temp))
                repo.package("Cargo.toml", "app")
                repo.package("later/Cargo.toml", "later")
                path = repo.root / "later/Cargo.toml"
                if invalid == "deleted":
                    path.unlink()
                elif invalid == "malformed":
                    path.write_text("not TOML")
                elif invalid == "missing name":
                    path.write_text("[package]\n")
                else:
                    outside = repo.root / "target-manifest"
                    path.rename(outside)
                    try:
                        if invalid == "file link":
                            path.symlink_to(outside)
                        else:
                            path.parent.rmdir()
                            (repo.root / "external").mkdir()
                            outside.rename(repo.root / "external/Cargo.toml")
                            path.parent.symlink_to(repo.root / "external", target_is_directory=True)
                    except OSError as error:
                        self.skipTest(f"symlink creation unavailable: {error}")
                calls = []
                with self.assertRaises((OSError, ValueError)):
                    lint_packages(repo.root, lambda *args, **kwargs: calls.append(args))
                self.assertEqual(calls, [])

    def test_empty_inventory_and_any_package_compiler_failure_fail_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Repository(Path(temp))
            repo.write("Cargo.toml", "[workspace]\n")
            with self.assertRaisesRegex(ValueError, "no tracked Cargo packages"):
                packages(repo.root)
            repo.package("Cargo.toml", "app")
            repo.package("second/Cargo.toml", "second")
            for failing in ("app", "second"):
                calls = []

                def compiler(command, **kwargs):
                    calls.append(command)
                    if command[6] == failing:
                        raise subprocess.CalledProcessError(1, command)

                with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(subprocess.CalledProcessError):
                    lint_packages(repo.root, compiler)
                self.assertEqual(calls[-1][6], failing)


class PackageCompilerTests(unittest.TestCase):
    def test_root_default_lint_misses_packages_that_the_complete_runner_rejects(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Repository(Path(temp))
            repo.write("Cargo.toml", '[workspace]\nresolver="3"\nmembers=["default", "nondefault"]\ndefault-members=["default"]\n')
            repo.write("clippy.toml", "cognitive-complexity-threshold=25\n")
            for name in ("default", "nondefault", "independent"):
                repo.package(f"{name}/Cargo.toml", name, "[workspace]\n" if name == "independent" else "")
                repo.write(f"{name}/src/lib.rs", "pub fn score(_: usize) -> usize { 0 }\n")
            repo.package(RECORDING_MANIFEST, "recording", '[workspace]\n[lints.rust]\n'
                         'unexpected_cfgs={level="warn", check-cfg=["cfg(recovery_recording)"]}\n')
            repo.write(str(Path(RECORDING_MANIFEST).parent / "src/lib.rs"),
                       '#[cfg(not(recovery_recording))] compile_error!("recording profile required");\n')
            env = dict(os.environ, CARGO_TARGET_DIR=str(repo.root / "target"), CLIPPY_CONF_DIR=str(repo.root))
            logs = []

            def cargo(command, **kwargs):
                try:
                    result = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                            text=True, **kwargs)
                except subprocess.CalledProcessError as error:
                    logs.append(error.stdout)
                    raise
                logs.append(result.stdout)
                return result

            for manifest in ("Cargo.toml", "independent/Cargo.toml", RECORDING_MANIFEST):
                cargo(["cargo", "generate-lockfile", "--offline", "--manifest-path", manifest], cwd=repo.root, check=True)
            branches = " ".join(f"if value == {i} {{ score += 1; }}" for i in range(25))
            for name in ("nondefault", "independent"):
                with self.subTest(package=name):
                    repo.write(f"{name}/src/lib.rs", f"pub fn score(value: usize) -> usize {{ let mut score=0; {branches} score }}\n")
                    # Negative control: the former root command succeeds without compiling this package.
                    cargo(["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"], cwd=repo.root, check=True)
                    with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(subprocess.CalledProcessError) as failed:
                        lint_packages(repo.root, cargo)
                    self.assertIn("cognitive_complexity", failed.exception.stdout)
                    repo.write(f"{name}/src/lib.rs", "pub fn score(_: usize) -> usize { 0 }\n")
                    with contextlib.redirect_stdout(io.StringIO()):
                        lint_packages(repo.root, cargo)
            print("\n".join(logs))
