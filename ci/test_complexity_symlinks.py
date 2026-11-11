"""Policy inputs must describe the same files that Cargo and Clippy will open."""

import contextlib
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from complexity_policy import main

MANIFEST = b'[package]\nname="fixture"\nversion="0.0.0"\n[lints.clippy]\ncognitive_complexity="forbid"\n'
CONFIG = b"cognitive-complexity-threshold = 25\n"
CAP = b'[build]\nrustflags=["--cap-lints=allow"]\n'


class SymlinkPolicyTests(unittest.TestCase):
    def test_policy_file_and_cargo_directory_links_fail_closed_in_every_snapshot(self):
        for relative in ("src/lib.rs", "Cargo.toml", "clippy.toml", ".cargo/config",
                         ".cargo/config.toml", ".cargo", ".CARGO"):
            with self.subTest(path=relative), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)

                def git(*args):
                    return subprocess.check_output(["git", "-C", temp, *args], stderr=subprocess.PIPE)

                def check(*args):
                    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                        return main(["--root", temp, *args])

                git("init", "-q")
                git("config", "user.name", "Fixture")
                git("config", "user.email", "fixture@example.invalid")
                git("config", "commit.gpgsign", "false")
                git("config", "core.symlinks", "true")
                (root / "Cargo.toml").write_bytes(MANIFEST)
                (root / "clippy.toml").write_bytes(CONFIG)
                (root / "src").mkdir()
                (root / "src/lib.rs").write_text("pub fn f() {}\n")
                path = root / relative
                directory = relative.casefold() == ".cargo"
                if directory:
                    path.write_bytes(b"")
                elif not path.exists():
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("[env]\n")
                git("add", ".")
                git("commit", "-qm", "regular policy inputs")
                self.assertEqual(check("--revision", "HEAD"), 0)
                if directory:
                    path.unlink()
                    target = root / "shared-cargo"
                    target.mkdir()
                    (target / "config").write_bytes(CAP)
                    saved = None
                else:
                    saved = path.read_bytes()
                    path.unlink()
                    target = path.parent / "[env]"
                    target.write_bytes(CAP)
                try:
                    os.symlink(os.path.relpath(target, path.parent), path, target_is_directory=directory)
                except OSError as error:
                    self.skipTest(f"symlink creation unavailable: {error}")
                # The link payload can be valid TOML; policy must reject its mode,
                # rather than accidentally treating it as an empty [env] table.
                self.assertEqual(check(), 2)
                self.assertEqual(check("--staged"), 0)
                self.assertEqual(check("--revision", "HEAD"), 0)
                git("add", ".")
                self.assertEqual(check("--staged"), 2)
                self.assertEqual(check("--revision", "HEAD"), 0)
                git("commit", "-qm", "linked policy input")
                self.assertEqual(check("--revision", "HEAD"), 2)
                if saved is not None:
                    path.unlink()
                    path.write_bytes(saved)
                    self.assertEqual(check(), 0)
                    self.assertEqual(check("--staged"), 2)
                    self.assertEqual(check("--revision", "HEAD"), 2)
