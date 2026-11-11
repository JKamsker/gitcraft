"""Compiled-inventory compatibility checks; no Rust tests execute here."""
import importlib.util
from collections import Counter
from pathlib import Path
import tempfile
import unittest

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


if __name__ == "__main__":
    unittest.main()
