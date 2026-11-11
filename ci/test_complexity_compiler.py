"""Execute Clippy on dependency-free fixtures to protect the real lint boundary."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


def branching(count):
    checks = "\n".join(f"if value == {i} {{ score += 1; }}" for i in range(count))
    return f"let mut score = 0; {checks} score"


class CompilerTests(unittest.TestCase):
    def test_threshold_functions_closures_async_and_local_suppressions(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text('''[package]
name = "complexity-gate-fixture"
version = "0.0.0"
edition = "2024"
[workspace]
[lints.clippy]
cognitive_complexity = "forbid"
''')
            (root / "clippy.toml").write_text("cognitive-complexity-threshold = 25\n")
            cases = [
                ("boundary passes", f"pub fn score(value: usize) -> usize {{ {branching(24)} }}", True),
                ("boundary fails", f"pub fn score(value: usize) -> usize {{ {branching(25)} }}", False),
                ("closure fails", f"pub fn score() -> impl Fn(usize) -> usize {{ |value| {{ {branching(25)} }} }}", False),
                ("async fails", f"pub async fn score(value: usize) -> usize {{ {branching(25)} }}", False),
                ("nested function fails", f"pub fn score(value: usize) -> usize {{ fn inner(value: usize) -> usize {{ {branching(25)} }} inner(value) }}", False),
                ("local allow fails", f"#[allow(clippy::cognitive_complexity)] pub fn score(value: usize) -> usize {{ {branching(25)} }}", False),
            ]
            for name, source, passes in cases:
                with self.subTest(name=name):
                    (root / "src/lib.rs").write_text(source + "\n")
                    env = os.environ.copy()
                    env["CARGO_TARGET_DIR"] = str(root / "target")
                    env["CLIPPY_CONF_DIR"] = str(root)
                    result = subprocess.run(
                        ["cargo", "clippy", "--offline", "--all-targets", "--", "-D", "warnings"],
                        cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                    )
                    self.assertEqual(result.returncode == 0, passes, result.stdout)
                    if not passes:
                        self.assertIn("cognitive_complexity", result.stdout)
