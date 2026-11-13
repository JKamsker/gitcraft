"""The pre-push command runs exactly CI's gate commands (issue #211)."""

from pathlib import Path
import shlex
import sys
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import check  # noqa: E402
from test_workflow_gates import CLIPPY_CHECK, COMPLEXITY_CHECK, SIZE_CHECK, TOOLING_CHECK  # noqa: E402

WORKFLOWS = Path(__file__).resolve().parents[1] / ".github/workflows"


def normalized(command):
    words = shlex.split(command) if isinstance(command, str) else list(command)
    return [("python" if word in ("python", "python3") else word) for word in words if word != "-v"]


class PrePushTests(unittest.TestCase):
    def test_the_gates_are_the_commands_ci_requires(self):
        ci = [normalized(command) for command in
              ("cargo fmt --check", TOOLING_CHECK, SIZE_CHECK, COMPLEXITY_CHECK, CLIPPY_CHECK)]
        self.assertEqual([normalized(command) for _, command in check.GATES], ci)
        workflows = "".join(path.read_text(encoding="utf-8") for path in WORKFLOWS.glob("*.yml"))
        for command in ("cargo fmt --check", TOOLING_CHECK, SIZE_CHECK, COMPLEXITY_CHECK, CLIPPY_CHECK):
            self.assertIn(f"run: {command}\n", workflows)

    def test_focused_full_and_default_plans(self):
        self.assertEqual(check.plan()[-1][1], ["cargo", "test", "--locked", "--no-run"])
        focused = check.plan(["repo::stack", "git::drafts"])
        self.assertEqual([command for _, command in focused[-2:]],
                         [["cargo", "test", "--locked", "repo::stack"], ["cargo", "test", "--locked", "git::drafts"]])
        self.assertEqual(check.plan(full=True)[-1][1], ["cargo", "test", "--locked"])

    def test_cargo_runs_at_low_priority(self):
        command, extra = check.low_priority(["cargo", "test"])
        self.assertTrue(command[:3] == ["nice", "-n", "19"] or extra.get("creationflags") == 0x40)
        self.assertEqual(check.low_priority(["python3", "x.py"]), (["python3", "x.py"], {}))

    def test_dry_run_prints_without_running(self):
        self.assertEqual(check.main(["--dry-run", "--test", "x"]), 0)


if __name__ == "__main__":
    unittest.main()
