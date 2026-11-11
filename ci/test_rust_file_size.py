"""Full-tree size-policy regression tests using disposable Git repositories."""

import contextlib
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from rust_file_size import HARD_LIMIT, WARNING_LINES, inventory, main, physical_lines, tracked_contents


class LineTests(unittest.TestCase):
    def test_physical_boundaries(self):
        for count in (0, 1, 299, 300, 499, 500, 501):
            for ending in (b"\n", b"\r\n"):
                for final_newline in (True, False):
                    with self.subTest(count=count, ending=ending, final=final_newline):
                        data = ending.join([b"// comment"] * count)
                        if count and final_newline:
                            data += ending
                        self.assertEqual(physical_lines(data), count)
        self.assertEqual(physical_lines(b"\n\n"), 2)
        self.assertEqual(physical_lines("a\u2028b\u0085c\rd".encode()), 1)


class RepositoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Size fixture")
        self.git("config", "user.email", "size@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "core.autocrlf", "false")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], stderr=subprocess.PIPE)

    def write(self, name, count, *, stage=True, ending=b"\n", final=True):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        data = ending.join([b"// @generated; DO NOT EDIT"] * count)
        if count and final:
            data += ending
        path.write_bytes(data)
        if stage:
            self.git("add", "-f", "--", name)

    def check(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = main(["--root", str(self.root), *args])
        return code, out.getvalue(), err.getvalue()

    def test_empty_tree_and_empty_file(self):
        self.assertEqual(self.check()[0], 0)
        self.assertEqual(self.check("--staged")[0], 0)
        self.write("empty.rs", 0)
        self.assertEqual(dict(inventory(self.root, staged=True)), {"empty.rs": 0})
        self.assertEqual(self.check("--staged")[0], 0)

    def test_cli_thresholds_all_line_endings(self):
        self.assertEqual((WARNING_LINES, HARD_LIMIT), (300, 500))
        for count in (299, 300, 499, 500, 501):
            for ending in (b"\n", b"\r\n"):
                for final in (True, False):
                    with self.subTest(count=count, ending=ending, final=final):
                        self.write("fixture.rs", count, ending=ending, final=final)
                        for mode in ((), ("--staged",)):
                            code, out, err = self.check(*mode)
                            self.assertEqual(code, int(count > 500))
                            self.assertEqual(err, "")
                            if count >= 300:
                                self.assertIn(f"{count} physical lines", out)
                                self.assertIn("warning at 300; hard limit 500", out)
                                self.assertIn("decompose", out)
                                self.assertIn("error:" if count > 500 else "warning:", out)
                            else:
                                self.assertNotIn("'fixture.rs'", out)

    def test_discovery_everywhere_no_generated_vendor_exemptions(self):
        names = ["build.rs", "tests/check.rs", "examples/demo.rs", "crates/other/lib.rs",
                 "odd dir/ü nicode.rs", "generated/output.rs", "vendor/library.rs",
                 "deep/vendor/generated/code.rs", "target/tracked.rs", "src/unit.rs"]
        # Even ignored, force-added sources must be checked.
        (self.root / ".gitignore").write_text("vendor/\ngenerated/\ntarget/\n")
        for name in names:
            self.write(name, 501)
        self.write("target/untracked.rs", 999, stage=False)
        self.write("untracked.rs", 999, stage=False)
        self.write("other.py", 999)
        self.assertEqual(set(dict(inventory(self.root))), set(names))
        code, out, _ = self.check("--staged")
        self.assertEqual(code, 1)
        self.assertIn(f"{len(names)} errors", out)
        for name in names:
            self.assertIn(repr(name), out)

    def test_staged_and_working_contents_are_independent_full_trees(self):
        self.write("unchanged.rs", 501)
        self.git("commit", "-qm", "oversized base")
        self.write("changed.rs", 299)
        self.write("unchanged.rs", 1, stage=False)
        self.assertEqual(self.check()[0], 0)
        self.assertEqual(self.check("--staged")[0], 1)
        self.write("unchanged.rs", 1)
        self.write("unchanged.rs", 501, stage=False)
        self.assertEqual(self.check()[0], 1)
        self.assertEqual(self.check("--staged")[0], 0)
        self.assertEqual(self.check("--revision", "HEAD")[0], 1)

    def test_deletions_and_renames(self):
        self.write("old.rs", 501)
        self.write("removed.rs", 501)
        self.git("commit", "-qm", "base")
        (self.root / "removed.rs").unlink()
        self.assertNotIn("removed.rs", dict(inventory(self.root)))
        self.assertIn("removed.rs", dict(inventory(self.root, staged=True)))
        self.git("add", "-u")
        self.git("mv", "old.rs", "renamed ü file.rs")
        self.assertEqual(dict(inventory(self.root, staged=True)), {"renamed ü file.rs": 501})
        self.git("mv", "renamed ü file.rs", "no-longer-rust.txt")
        self.assertEqual(self.check("--staged")[0], 0)
        self.git("mv", "no-longer-rust.txt", "new.rs")
        self.assertEqual(self.check("--staged")[0], 1)
        self.assertEqual(len(inventory(self.root, revision="HEAD")), 2)

    def test_revision_uses_blobs_ignoring_worktree_and_index(self):
        self.write("candidate.rs", 501)
        self.git("commit", "-qm", "candidate")
        candidate = self.git("rev-parse", "HEAD").decode().strip()
        self.write("candidate.rs", 1)
        self.git("commit", "-qm", "small")
        self.assertEqual(self.check("--revision", candidate)[0], 1)
        self.assertEqual(self.check("--revision", "HEAD")[0], 0)
        self.assertEqual(self.check("--revision", "missing-revision")[0], 2)
        self.assertEqual(self.check("--revision=--all")[0], 2)

    @unittest.skipIf(os.name == "nt", "Git symlink fixture requires Unix symlinks")
    def test_symlink_counts_git_link_content_without_following_target(self):
        self.write("outside.txt", 501, stage=False)
        (self.root / "link.rs").symlink_to("outside.txt")
        self.git("add", "link.rs")
        self.assertEqual(dict(inventory(self.root)), {"link.rs": 1})
        self.assertEqual(dict(inventory(self.root, staged=True)), {"link.rs": 1})

    def test_policy_reader_rejects_snapshot_symlink_modes_independently_of_working_file(self):
        self.write("policy.toml", 1)
        self.write("link-name.txt", 1, stage=False)
        (self.root / "link-name.txt").write_bytes(b"[env]")
        blob = self.git("hash-object", "-w", "link-name.txt").decode().strip()
        self.git("update-index", "--cacheinfo", f"120000,{blob},policy.toml")
        self.git("commit", "-qm", "symlink policy snapshot")
        for mode in ({"staged": True}, {"revision": "HEAD"}):
            with self.subTest(mode=mode):
                self.assertEqual(dict(tracked_contents(self.root, suffix=b".toml", **mode)),
                                 {"policy.toml": b"[env]"})
                with self.assertRaisesRegex(ValueError, "symlink policy input.*policy.toml"):
                    tracked_contents(self.root, suffix=b".toml", reject_symlinks=True, **mode)
        # Working policy is about its actual regular file, even when the index is a link.
        self.assertEqual(dict(tracked_contents(self.root, suffix=b".toml", reject_symlinks=True)),
                         {"policy.toml": (self.root / "policy.toml").read_bytes()})
        (self.root / "policy.toml").unlink()
        self.assertEqual(tracked_contents(self.root, suffix=b".toml", reject_symlinks=True), [])

    @unittest.skipIf(os.name == "nt", "working symlink fixture requires Unix symlinks")
    def test_policy_reader_rejects_actual_working_links_including_dangling_targets(self):
        self.write("outside.txt", 501, stage=False)
        for name, target in (("linked.toml", "outside.txt"), ("dangling.toml", "missing.txt")):
            with self.subTest(target=target):
                self.write(name, 1)
                self.git("commit", "-qm", "regular policy snapshot")
                path = self.root / name
                regular = path.read_bytes()
                path.unlink()
                path.symlink_to(target)
                options = {"suffix": name.encode(), "reject_symlinks": True}
                with self.assertRaisesRegex(ValueError, "symlink policy input"):
                    tracked_contents(self.root, **options)
                self.assertEqual(dict(tracked_contents(self.root, suffix=name.encode())),
                                 {name: target.encode()})
                for mode in ({"staged": True}, {"revision": "HEAD"}):
                    self.assertEqual(dict(tracked_contents(self.root, **options, **mode)),
                                     {name: regular})
                path.unlink()
                path.write_bytes(regular)

    @unittest.skipIf(os.name == "nt", "newline filenames unavailable on Windows")
    def test_nul_delimited_paths_and_subdirectory_invocation(self):
        name = "nested/line\nbreak\tü.rs"
        self.write(name, 501)
        self.assertEqual(dict(inventory(self.root, staged=True)), {name: 501})
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = main(["--root", str(self.root / "nested"), "--staged"])
        self.assertEqual(code, 1)
        self.assertIn(repr(name), out.getvalue())

    @unittest.skipIf(os.name == "nt", "newline and arbitrary byte names require Unix")
    def test_root_discovery_preserves_trailing_newlines_and_filesystem_bytes(self):
        parent = self.root
        for suffix in ("", os.fsdecode(b"\xff")):
            with self.subTest(suffix=suffix):
                # Trimming all LFs would silently check the small sibling's index.
                sibling = parent / ("candidate" + suffix)
                self.root = sibling
                self.root.mkdir()
                self.git("init", "-q")
                self.write("fixture.rs", 1)
                self.assertEqual(self.check("--staged")[0], 0)
                for newlines in ("\n", "\n\n"):
                    with self.subTest(newlines=newlines):
                        self.root = parent / (sibling.name + newlines)
                        self.root.mkdir()
                        self.git("init", "-q")
                        self.write("fixture.rs", 501)
                        for mode in ((), ("--staged",)):
                            code, out, err = self.check(*mode)
                            self.assertEqual(code, 1)
                            self.assertEqual(err, "")
                            self.assertIn("'fixture.rs': 501 physical lines", out)
                            self.assertIn("1 errors", out)

    def test_gitlink_with_rust_extension_fails_closed(self):
        self.write("base.rs", 1)
        self.git("commit", "-qm", "base")
        commit = self.git("rev-parse", "HEAD").decode().strip()
        self.git("update-index", "--add", "--cacheinfo", f"160000,{commit},module.rs")
        self.assertEqual(self.check("--staged")[0], 2)
        self.git("commit", "-qm", "gitlink")
        self.assertEqual(self.check("--revision", "HEAD")[0], 2)

    def test_unmerged_source_fails_closed(self):
        self.write("conflict.rs", 1)
        blob = self.git("hash-object", "conflict.rs").strip()
        self.git("update-index", "--force-remove", "conflict.rs")
        subprocess.run(["git", "-C", str(self.root), "update-index", "--index-info"],
                       input=b"100644 " + blob + b" 1\tconflict.rs\n", check=True)
        for args in ((), ("--staged",)):
            code, _, err = self.check(*args)
            self.assertEqual(code, 2)
            self.assertIn("unmerged Rust path", err)


if __name__ == "__main__":
    unittest.main()
