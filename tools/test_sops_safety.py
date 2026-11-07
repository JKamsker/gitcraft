"""Static safety unit only. No production imports, process tables, commands or signals."""
import ast
import itertools
from pathlib import Path
import unittest

from sops_safety import LaunchReceipt, NotOwned, ProcessRecord, RecordingSignalSink, synthetic_stop_plan


OWNER = "synthetic-launch-owner"
RECEIPT = LaunchReceipt(OWNER, "worker", "launcher", 100, 100, True)


def row(handle, parent, pid, *, session=100, group=100, owner=OWNER, alive=True):
    return ProcessRecord(handle, parent, pid, session, group, owner, alive)


def fixture():
    return (
        row("init", None, 1, session=1, group=1, owner="host"),
        row("ancestor", "init", 10, session=10, group=10, owner="host"),
        row("launcher", "ancestor", 20, session=10, group=20),
        row("worker", "launcher", 100),
        row("child", "worker", 101),
        row("grandchild", "child", 102),
        row("sibling", "launcher", 110, session=10, group=110),
        row("unrelated", "init", 200, session=200, group=200, owner="host"),
    )


class SyntheticSafetyTests(unittest.TestCase):
    def test_exact_owned_tree_excludes_init_ancestors_siblings_and_unrelated(self):
        records = fixture()
        plan = synthetic_stop_plan(RECEIPT, records)
        self.assertEqual(plan.handles, ("grandchild", "child", "worker"))
        self.assertEqual(records, fixture())  # no default-map insertions/mutation
        self.assertFalse(any(type(handle) is int for handle in plan.handles))

    def test_recorder_has_only_owned_handle_intents_and_rejects_foreign_targets(self):
        plan = synthetic_stop_plan(RECEIPT, fixture())
        sink = RecordingSignalSink(plan)
        for handle in plan.handles:
            sink.record(handle)
        self.assertEqual(sink.records, [(h, "stop-owned-handle") for h in plan.handles])
        for foreign in ("init", "ancestor", "launcher", "unrelated", "0", "1", "-100"):
            with self.assertRaises(NotOwned):
                sink.record(foreign)
        with self.assertRaises(NotOwned):
            sink.record("worker", "KILL")
        self.assertEqual(len(sink.records), 3)

    def test_pid_zero_one_negative_and_group_session_mismatch_fail_closed(self):
        for label in (-100, -1, 0, 1):
            records = (row("worker", "launcher", label, session=label, group=label),)
            receipt = LaunchReceipt(OWNER, "worker", "launcher", label, label, True)
            with self.subTest(label=label), self.assertRaises(NotOwned):
                synthetic_stop_plan(receipt, records)
        for kwargs in ({"session": 99}, {"group": 99}, {"owner": "foreign"}, {"alive": False}):
            with self.subTest(kwargs=kwargs), self.assertRaises(NotOwned):
                synthetic_stop_plan(RECEIPT, (row("worker", "launcher", 100, **kwargs),))

    def test_missing_wrong_or_unowned_root_never_produces_a_plan(self):
        invalid = (
            LaunchReceipt(OWNER, "absent", "launcher", 100, 100, True),
            LaunchReceipt(OWNER, "worker", "launcher", 100, 100, False),
            LaunchReceipt("foreign", "worker", "launcher", 100, 100, True),
            LaunchReceipt(OWNER, "worker", "worker", 100, 100, True),
            LaunchReceipt(OWNER, "worker", "other-launcher", 100, 100, True),
            LaunchReceipt(OWNER, "worker", "launcher", 100.0, 100, True),
            LaunchReceipt(OWNER, "worker", "launcher", 100, True, True),
        )
        for receipt in invalid:
            with self.subTest(receipt=receipt), self.assertRaises(NotOwned):
                synthetic_stop_plan(receipt, fixture())

    def test_foreign_scope_escape_dead_records_and_unknown_parent_are_not_signaled(self):
        records = fixture() + (
            row("foreign-child", "worker", 201, owner="foreign"),
            row("foreign-link-child", "foreign-child", 202),
            row("escaped-session", "worker", 203, session=203),
            row("escaped-group", "worker", 204, group=204),
            row("unknown-parent", "not-in-registry", 205),
            row("dead", "worker", 206, alive=False),
            row("reparented", "init", 207),
            row("pid-zero", "worker", 0),
        )
        self.assertEqual(synthetic_stop_plan(RECEIPT, records).handles, ("grandchild", "child", "worker"))

    def test_duplicate_handles_and_pid_reuse_ambiguities_refuse_the_whole_plan(self):
        for extra in (row("child", "worker", 300), row("new-handle", "worker", 101)):
            with self.subTest(extra=extra), self.assertRaises(NotOwned):
                synthetic_stop_plan(RECEIPT, fixture() + (extra,))
        with self.assertRaises(NotOwned):
            synthetic_stop_plan(RECEIPT, fixture() + (row("bad-type", "worker", True),))

    def test_cycles_cannot_add_ancestors_or_unrelated_nodes(self):
        records = fixture() + (row("cycle-a", "cycle-b", 300), row("cycle-b", "cycle-a", 301), row("self", "self", 302))
        self.assertEqual(synthetic_stop_plan(RECEIPT, records).handles, ("grandchild", "child", "worker"))
        bad = (row("launcher", "ancestor", 20), row("worker", "launcher", 100))
        with self.assertRaises(NotOwned):
            synthetic_stop_plan(RECEIPT, bad)

    def test_record_order_does_not_change_the_exact_plan(self):
        records = fixture()
        for order in (records, records[::-1], records[3:] + records[:3]):
            self.assertEqual(synthetic_stop_plan(RECEIPT, order).handles, ("grandchild", "child", "worker"))

    def test_exhaustive_small_parent_graphs_never_include_unreachable_handles(self):
        handles = ("a", "b", "c")
        # 6^3 independent synthetic graphs, no live PID snapshot or process introspection.
        for parents in itertools.product(("worker", "launcher", "foreign", "a", "b", "c"), repeat=3):
            records = (row("worker", "launcher", 100),) + tuple(row(h, p, 101+i) for i, (h, p) in enumerate(zip(handles, parents)))
            plan = synthetic_stop_plan(RECEIPT, records)
            self.assertEqual(plan.handles[-1], "worker")
            for handle in plan.handles[:-1]:
                seen = set()
                while handle != "worker":
                    self.assertIn(handle, handles)
                    self.assertNotIn(handle, seen)
                    seen.add(handle)
                    handle = parents[handles.index(handle)]

    def test_model_has_no_process_io_dynamic_execution_or_signal_capability(self):
        source = Path(__file__).with_name("sops_safety.py").read_text()
        tree = ast.parse(source)
        imports = {node.module for node in ast.walk(tree) if isinstance(node, ast.ImportFrom)}
        self.assertEqual(imports, {"dataclasses"})
        self.assertFalse(any(isinstance(node, ast.Import) for node in ast.walk(tree)))
        calls = {node.func.id for node in ast.walk(tree) if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)}
        self.assertFalse(calls & {"open", "exec", "eval", "compile", "__import__", "getattr"})

    def test_runtime_and_fixture_remain_quarantined_without_remote_process_apis(self):
        root = Path(__file__).resolve().parents[1]
        encrypted = root / "src/repo/diff/encrypted"
        tool = (encrypted / "tool.rs").read_text()
        module = (encrypted / "mod.rs").read_text()
        tests = (encrypted / "tests.rs").read_text()
        for source in (tool, module):
            self.assertIn("RECOVERY_QUARANTINE_UNSAFE_SOPS_WATCHDOG", source)
            self.assertIn("compile_error!", source)
        for forbidden in ("stop_tree()", "ps -eo", "kill -", "watchdog=$!", "sleep {seconds}", "remote::", "remote::Request", "fn request("):
            self.assertNotIn(forbidden, tool)
        self.assertIn("pub(super) fn command(format: Format) -> Command", tool)
        self.assertIn('Command::new(program)', tool)
        self.assertIn('current_dir(std::env::temp_dir())', tool)
        self.assertNotIn('replace("sleep 20;"', tests)
        self.assertNotIn("remote_sops_request_preserves_stdin_and_fixed_argv", tests)
        self.assertIn("sops_command_is_local_and_has_only_fixed_format_arguments", tests)
        self.assertNotIn("tool::decrypt(\n                &repo.0,", tests)
        self.assertNotIn("private_output", (root / "src/remote/mod.rs").read_text())
        self.assertNotIn("private_output", (root / "src/remote/session.rs").read_text())
        self.assertNotIn("private_transport", (root / "src/remote/session.rs").read_text())
        self.assertIn("this computer", (encrypted / "view.rs").read_text())
        self.assertIn("only ciphertext", (root / "README.md").read_text())


if __name__ == "__main__":
    unittest.main()
