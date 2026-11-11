"""Exercise checkout rules that keep embedded Unix scripts executable."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def git(root, *args):
    env = {key: value for key, value in os.environ.items()
           if key not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR")}
    env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
    return subprocess.check_output(
        ["git", "-c", "user.name=Checkout fixture", "-c", "user.email=fixture@example.invalid",
         "-c", "commit.gpgsign=false", *args], cwd=root, env=env, stderr=subprocess.PIPE,
    )


class CheckoutAssetTests(unittest.TestCase):
    def test_all_unix_scripts_keep_lf_bytes_under_windows_checkouts(self):
        paths = [os.fsdecode(path) for path in git(ROOT, "ls-files", "-z", "*.sh").split(b"\0") if path]
        self.assertTrue(paths, "no tracked Unix scripts found")
        sources = {path: git(ROOT, "show", f"HEAD:{path}") for path in paths}
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            git(repo, "init", "-q")
            for path, content in sources.items():
                self.assertNotIn(b"\r", content, path)
                target = repo / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(content)
            git(repo, "-c", "core.autocrlf=false", "add", "--", *paths)
            git(repo, "commit", "-qm", "LF script blobs without checkout policy")
            # Negative control: identical blobs become broken CRLF shell assets.
            for path in paths:
                (repo / path).unlink()
            git(repo, "-c", "core.autocrlf=true", "checkout-index", "--all", "--force")
            self.assertTrue(all(b"\r\n" in (repo / path).read_bytes() for path in paths))
            (repo / ".gitattributes").write_bytes((ROOT / ".gitattributes").read_bytes())
            git(repo, "add", ".gitattributes")
            git(repo, "commit", "-qm", "Apply actual repository checkout policy")
            for autocrlf in ("true", "false", "input"):
                for path in paths:
                    (repo / path).unlink()
                git(repo, "-c", f"core.autocrlf={autocrlf}", "checkout-index", "--all", "--force")
                for path, expected in sources.items():
                    with self.subTest(autocrlf=autocrlf, path=path):
                        self.assertEqual((repo / path).read_bytes(), expected)


if __name__ == "__main__":
    unittest.main()
