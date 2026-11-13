#!/usr/bin/env python3
"""The pre-push check: CI's fast gates, run locally before a push or a pull request (issue #211).

    python3 ci/check.py                         # gates + compile every test target
    python3 ci/check.py --test repo::stack      # gates + those tests (repeatable)
    python3 ci/check.py --full                  # gates + the whole Linux/macOS test suite
    python3 ci/check.py --dry-run               # print the commands only

The gates are the exact commands CI runs: formatting, the tooling tests, the Rust
file-size and complexity-policy checks of the committed tree (`--revision HEAD`,
so commit first), and strict all-target Clippy for every package. Cargo runs at
the lowest OS priority. Windows shards: after `python ci/windows-test-build.py`,
`python ci/test-shards.py --check-only` and `python ci/test-shards.py <shard>`
run the CI shards natively. It stops at the first failure.
"""

import argparse
import os
import shlex
import subprocess
import sys

WINDOWS = sys.platform == "win32"
PYTHON = "python" if WINDOWS else "python3"

# The same commands as .github/workflows (ci/test_workflow_gates.py pins them).
GATES = (
    ("formatting", ["cargo", "fmt", "--check"]),
    ("tooling tests", [PYTHON, "-m", "unittest", "discover", "-s", "ci", "-p", "test_*.py"]),
    ("Rust file size", [PYTHON, "ci/rust_file_size.py", "--revision", "HEAD"]),
    ("complexity policy", [PYTHON, "ci/complexity_policy.py", "--revision", "HEAD"]),
    ("clippy and complexity gate", [PYTHON, "ci/clippy_packages.py"]),
)


def plan(tests=(), full=False):
    steps = list(GATES)
    if full:
        steps.append(("all tests", ["cargo", "test", "--locked"]))
    elif tests:
        steps += [(f"tests: {name}", ["cargo", "test", "--locked", name]) for name in tests]
    else:
        steps.append(("compile every test target", ["cargo", "test", "--locked", "--no-run"]))
    return steps


def low_priority(command):
    """Cargo and the compilers it starts yield to interactive work (AGENTS.md)."""
    if command[0] != "cargo":
        return command, {}
    if WINDOWS:
        return command, {"creationflags": 0x40}  # IDLE_PRIORITY_CLASS, inherited by rustc
    return ["nice", "-n", "19", *command], {}


def dirty():
    status = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"],
                            stdout=subprocess.PIPE, text=True)
    return bool(status.stdout.strip())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--test", action="append", default=[], help="a libtest filter to run (repeatable)")
    parser.add_argument("--full", action="store_true", help="run the whole test suite")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    steps = plan(args.test, args.full)
    if args.dry_run:
        for name, command in steps:
            print(f"{name}: {shlex.join(command)}")
        return 0
    if dirty():
        print("note: uncommitted changes; the size and policy gates check the committed HEAD, as CI does.")
    for name, command in steps:
        command, extra = low_priority(command)
        print(f"==> {name}: {shlex.join(command)}", flush=True)
        status = subprocess.run(command, **extra).returncode
        if status:
            print(f"FAILED: {name} (exit {status}). Fix it before pushing; CI runs the same command.")
            return status
    print("All pre-push checks passed.")
    return 0


if __name__ == "__main__":
    os.chdir(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    sys.exit(main())
