"""Regression cases for configuration/snapshot bypasses of the Clippy gate."""

import contextlib
import io
from pathlib import Path
import subprocess
import tempfile
import unittest

from complexity_policy import main, policy_errors

CONFIG = b"cognitive-complexity-threshold = 25\n"
MANIFEST = b'[package]\nname = "fixture"\nversion = "0.0.0"\n[lints.clippy]\ncognitive_complexity = "forbid"\n'


class PolicyTests(unittest.TestCase):
    def errors(self, source=b"", extra=()):
        return policy_errors([("clippy.toml", CONFIG), ("Cargo.toml", MANIFEST), ("src/lib.rs", source), *extra])

    def test_local_thresholds_in_all_attribute_forms_are_rejected(self):
        for source in (
            '#[clippy::cognitive_complexity = "100"] fn f() {}',
            '#![clippy /* nested /* comment */ */ :: cognitive_complexity = "100"]',
            '#[cfg_attr(any(), clippy::cognitive_complexity = "100")] fn f() {}',
            '#[r#clippy :: r#cognitive_complexity = "100"] fn f() {}',
            'macro_rules! hide { () => { #[clippy::cognitive_complexity = "100"] fn f() {} } }',
        ):
            with self.subTest(source=source):
                self.assertIn("overrides are forbidden", self.errors(source.encode())[0])

    def test_documentation_literals_and_comments_are_not_overrides(self):
        source = '''
// #[clippy::cognitive_complexity = "100"]
/* #[clippy::cognitive_complexity = "100"] */
const DOC: &str = r##"#[clippy::cognitive_complexity = "100"]"##;
'''
        self.assertEqual(self.errors(source.encode()), [])

    def test_all_packages_and_nearer_configs_cannot_weaken_policy(self):
        for path in ("crates/new/Cargo.toml", "vendor/Cargo.toml", "tests/fixture/Cargo.toml"):
            with self.subTest(path=path):
                self.assertIn("must forbid", self.errors(extra=[(path, b'[package]\nname="new"')])[0])
        for path in (".clippy.toml", "src/clippy.toml", "crates/new/.clippy.toml"):
            with self.subTest(path=path):
                self.assertIn("single clippy.toml", self.errors(extra=[(path, CONFIG)])[0])
        for level in (b"allow", b"warn", b"deny"):
            self.assertIn("must forbid", self.errors(extra=[("other/Cargo.toml", MANIFEST.replace(b"forbid", level))])[0])
        self.assertEqual(self.errors(extra=[("other/Cargo.toml", MANIFEST.replace(b'"forbid"', b'{ level="forbid" }'))]), [])

    def test_missing_or_different_threshold_fails_closed(self):
        self.assertEqual(len(policy_errors([])), 2)
        for limit in ("24", "26", "true", '"25"'):
            with self.subTest(limit=limit):
                self.assertIn("must be 25", policy_errors([
                    ("clippy.toml", f"cognitive-complexity-threshold = {limit}".encode()),
                    ("Cargo.toml", MANIFEST),
                ])[0])


class SnapshotTests(unittest.TestCase):
    def test_index_and_revision_ignore_other_snapshot_edits(self):
        with tempfile.TemporaryDirectory() as temp:
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
            (root / "Cargo.toml").write_bytes(MANIFEST)
            (root / "clippy.toml").write_bytes(CONFIG)
            (root / "lib.rs").write_text("pub fn f() {}\n")
            git("add", ".")
            git("commit", "-qm", "valid policy")
            (root / "clippy.toml").write_bytes(CONFIG.replace(b"25", b"100"))
            self.assertEqual(check(), 1)
            self.assertEqual(check("--staged"), 0)
            self.assertEqual(check("--revision", "HEAD"), 0)
            git("add", "clippy.toml")
            self.assertEqual(check("--staged"), 1)
            self.assertEqual(check("--revision", "HEAD"), 0)
            (root / "clippy.toml").write_text("invalid TOML: [")
            self.assertEqual(check(), 2)
