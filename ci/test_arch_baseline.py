"""The architecture baseline may grow only with a fresh, scoped declaration."""
import copy
import unittest

import arch_baseline as gate


class GrowthTests(unittest.TestCase):
    def setUp(self):
        self.old = {"budgets": {"repo-core": {"domains": 2, "symbols": 10, "exported": 3}},
                    "forbidden_edges": [], "cycles": [], "repo_tab_mutations": [], "reexports": []}

    def test_unacknowledged_budget_growth_fails(self):
        new = copy.deepcopy(self.old)
        new["budgets"]["repo-core"]["symbols"] += 1
        with self.assertRaisesRegex(ValueError, "budget/repo-core/symbols=11"):
            gate.validate(self.old, new, {}, {})

    def test_scoped_fresh_declaration_and_shrink(self):
        new = copy.deepcopy(self.old)
        new["budgets"]["repo-core"]["symbols"] += 1
        entry = {"changes": ["budget/repo-core/symbols=11"], "reason": "new API", "owner": "@owner", "remediation": "narrow API"}
        approvals = {"feature": entry}
        gate.validate(self.old, new, {}, approvals)
        with self.assertRaises(ValueError):
            gate.validate(self.old, new, approvals, approvals)
        new["budgets"]["repo-core"]["exported"] += 1
        with self.assertRaises(ValueError):
            gate.validate(self.old, new, {}, approvals)
        gate.validate(new, self.old, {}, {})

    def test_exception_and_cycle_growth_cannot_hide_behind_removals(self):
        new = copy.deepcopy(self.old)
        new["forbidden_edges"] = [{"from": "a", "to": "b"}]
        new["repo_tab_mutations"] = [{"domain": "a", "field": "status"}]
        new["reexports"] = [{"module": "a", "path": "b", "target_domain": "b"}]
        new["cycles"] = [{"domains": ["a", "b"], "edges": [["a", "b"], ["b", "a"]]}]
        self.assertEqual(len(gate.growth(self.old, new)), 7)
        with self.assertRaises(ValueError):
            gate.validate(self.old, new, {}, {})

    def test_removing_a_symbol_limit_is_growth(self):
        new = copy.deepcopy(self.old)
        del new["budgets"]["repo-core"]["symbols"]
        self.assertIn("budget/repo-core/symbols=unlimited", gate.growth(self.old, new))


class GitCandidateTests(unittest.TestCase):
    def test_merge_base_and_committed_candidate_ignore_worktree_edits(self):
        import json
        from pathlib import Path
        import subprocess
        import tempfile

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)

            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root, stderr=subprocess.PIPE, text=True).strip()

            git("init", "-q")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            target = root / gate.BASELINE
            target.parent.mkdir(parents=True)
            old = {"budgets": {"a": {"domains": 1, "exported": 1}},
                   "forbidden_edges": [], "cycles": [], "repo_tab_mutations": [], "reexports": []}
            target.write_text(json.dumps(old))
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            new = copy.deepcopy(old)
            new["budgets"]["a"]["exported"] = 2
            target.write_text(json.dumps(new))
            git("add", ".")
            git("commit", "-qm", "undeclared increase")
            self.assertEqual(gate.main(["--root", folder, "--base", base]), 1)
            # Editing the working baseline or declaration cannot authorize a committed increase.
            declaration = {"feature": {"changes": ["budget/a/exported=2"], "reason": "API", "owner": "o", "remediation": "narrow"}}
            (root / gate.APPROVALS).write_text(json.dumps(declaration))
            target.write_text(json.dumps(old))
            self.assertEqual(gate.main(["--root", folder, "--base", base]), 1)
            target.write_text(json.dumps(new))
            git("add", ".")
            git("commit", "-qm", "declare increase")
            self.assertEqual(gate.main(["--root", folder, "--base", base]), 0)
            # Advancing the comparison target uses merge-base, not its newer file contents.
            candidate = git("rev-parse", "HEAD")
            git("checkout", "-qb", "target", base)
            (root / "unrelated").write_text("target advanced")
            git("add", ".")
            git("commit", "-qm", "target advance")
            advanced = git("rev-parse", "HEAD")
            self.assertEqual(gate.main(["--root", folder, "--base", advanced, "--revision", candidate]), 0)
            self.assertEqual(gate.main(["--root", folder, "--base", "missing-revision"]), 1)
