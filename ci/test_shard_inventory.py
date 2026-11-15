"""Compiled-inventory compatibility checks; no Rust tests execute here."""
import importlib.util
from collections import Counter
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("shards", Path(__file__).with_name("test-shards.py"))
shards = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shards)


class MigrationTests(unittest.TestCase):
    def test_missing_duplicate_stale_and_incomplete_inventory_fail(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "renames.md"
            path.write_text("| `old::test` | `new::test` |\n")
            for inventory in [Counter(), Counter({"new::test": 2}),
                              Counter({"old::test": 1, "new::test": 1})]:
                with self.assertRaises(RuntimeError):
                    shards.verify_renamed_tests(inventory, [(path, 1)])
            with self.assertRaises(RuntimeError):
                shards.verify_renamed_tests(Counter({"new::test": 1}), [(path, 2)])
            shards.verify_renamed_tests(Counter({"new::test": 1}), [(path, 1)])
            shards.verify_renamed_tests(Counter(), [(path, 1)], {"new::test"})
            with self.assertRaises(RuntimeError):
                shards.verify_renamed_tests(Counter({"new::test": 1}), [(path, 1)], {"new::test"})


class BalancedPartitionTests(unittest.TestCase):
    def test_moved_groups_stay_serial_or_isolated_and_partition_is_complete(self):
        # Model libtest substring selection without launching native children.
        names = ["git::apple::test", "git::zebra::test", "repo::apple::test",
                 "repo::delta::test", "repo::golf::test", "repo::november::test",
                 "repo::quebec::test", "repo::stack::other", "repo::zebra::test",
                 "app::test", "standalone::other"]
        names += [name for group in shards.ISOLATED.values() for name in group]

        def inventory(binaries, filters):
            includes, excludes = [], []
            iterator = iter(filters)
            for value in iterator:
                if value == "--skip":
                    excludes.append(next(iterator))
                elif value != "--exact":
                    includes.append(value)
            return Counter({name: 1 for name in names
                            if (not includes or any(part in name for part in includes))
                            and not any(part in name for part in excludes)})

        with patch.object(shards, "inventory", side_effect=inventory), \
             patch.object(shards, "verify_renamed_tests"):
            shards.verify_partition([])
        self.assertEqual(shards.SERIAL_GROUPS, {"repo-g-m": ["standalone::"]})
        self.assertNotIn("core", shards.ISOLATED)
        self.assertIn("parallel_bisect::tests::finds_the_first_bad_commit_with_any_parallelism",
                      shards.ISOLATED["repo-g-m"])


if __name__ == "__main__":
    unittest.main()
