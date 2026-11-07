#!/usr/bin/env python3
"""Run Cargo's precompiled Windows test binaries in complete, disjoint shards.

Cargo builds all targets once in ci/windows-test-build.py. Every shard downloads
that same artifact and uses the original libtest harness without recompiling.
"""

import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
import sys


GIT_FIRST = [f"git::{letter}" for letter in "abcdefghijklm"]
REPO_A_F = [f"repo::{letter}" for letter in "abcdef"]
REPO_G_M = [f"repo::{letter}" for letter in "ghijklm"]
REPO_N_S = [f"repo::{letter}" for letter in "nopqrs"]
REPO_A_S = REPO_A_F + REPO_G_M + REPO_N_S

# These Git-process-heavy tests need their own libtest invocation.
ISOLATED = {
    "repo-n-s": ["repo::reflog::tests::a_commit_lost_in_the_terminal_comes_back_as_a_branch"],
    "core": ["parallel_bisect::tests::finds_the_first_bad_commit_with_any_parallelism"],
}

# libtest accepts multiple substring includes and --skip exclusions.
# Catch-all shards ensure new or unusually named tests cannot silently disappear.
SHARDS = {
    "git-a-m": GIT_FIRST,
    "git-rest": ["git::", *[arg for p in GIT_FIRST for arg in ("--skip", p)]],
    "repo-a-f": [*REPO_A_F, "--skip", "git::"],
    "repo-g-m": [*REPO_G_M, "--skip", "git::"],
    "repo-n-s": [*REPO_N_S, "--skip", "git::", "--skip", ISOLATED["repo-n-s"][0]],
    "repo-rest": [
        "repo::",
        "--skip", "git::",
        *[arg for p in REPO_A_S for arg in ("--skip", p)],
    ],
    "core": [
        "--skip", "git::",
        "--skip", "repo::",
        "--skip", ISOLATED["core"][0],
    ],
}


def load_binaries():
    root = Path.cwd().resolve()
    manifest = json.loads((root / "ci" / "test-binaries.json").read_text())
    binaries = []
    for entry in manifest["binaries"]:
        path = (root / entry["path"]).resolve()
        cwd = (root / entry["cwd"]).resolve()
        if not path.is_relative_to(root) or not path.is_file():
            raise RuntimeError(f"Missing or unexpected test executable: {path}")
        if not cwd.is_relative_to(root) or not cwd.is_dir():
            raise RuntimeError(f"Missing or unexpected test working directory: {cwd}")
        binaries.append((path, cwd))
    if not binaries:
        raise RuntimeError("No test binaries in the Windows build artifact")
    return binaries


def inventory(binaries, filters):
    tests = Counter()
    for path, cwd in binaries:
        result = subprocess.run(
            [str(path), "--list", *filters],
            cwd=cwd,
            check=True,
            stdout=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        tests.update(
            line.rsplit(": ", 1)[0]
            for line in result.stdout.splitlines()
            if line.endswith((": test", ": benchmark"))
        )
    return tests


def isolated_inventory(binaries, shard):
    tests = Counter()
    for name in ISOLATED.get(shard, []):
        selected = inventory(binaries, [name, "--exact"])
        if selected != Counter({name: 1}):
            raise RuntimeError(f"Isolated test not found exactly once: {name}: {selected}")
        tests.update(selected)
    return tests


def verify_partition(binaries):
    full = inventory(binaries, [])
    if not full:
        raise RuntimeError("No tests found; refusing an empty partition")
    assigned = Counter()
    counts = {}
    for shard, filters in SHARDS.items():
        tests = inventory(binaries, filters)
        tests.update(isolated_inventory(binaries, shard))
        if not tests:
            raise RuntimeError(f"Shard {shard} has no tests")
        overlap = assigned & tests
        if overlap:
            raise RuntimeError(f"Tests assigned to multiple shards: {dict(overlap)}")
        assigned.update(tests)
        counts[shard] = tests.total()
    if assigned != full:
        raise RuntimeError(
            f"Partition differs from full Windows test suite: "
            f"missing={dict(full - assigned)}, extra={dict(assigned - full)}"
        )
    print(f"Verified complete, disjoint partition: {counts}; total={full.total()}", flush=True)


def run_shard(binaries, shard):
    filters = SHARDS[shard]
    print(f"Running precompiled Windows test shard {shard}", flush=True)
    ran = 0
    for path, cwd in binaries:
        selected = inventory([(path, cwd)], filters)
        if not selected:
            continue
        print(f"{path.name}: {selected.total()} tests", flush=True)
        result = subprocess.run([str(path), "--nocapture", *filters], cwd=cwd)
        if result.returncode:
            return result.returncode
        ran += selected.total()

    for name in ISOLATED.get(shard, []):
        for path, cwd in binaries:
            if inventory([(path, cwd)], [name, "--exact"]):
                print(f"Running isolated test: {name}", flush=True)
                result = subprocess.run([str(path), "--nocapture", name, "--exact"], cwd=cwd)
                if result.returncode:
                    return result.returncode
                ran += 1

    if not ran:
        raise RuntimeError(f"Refusing empty shard {shard}")
    print(f"Passed {ran} tests in {shard}", flush=True)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shard", nargs="?", choices=SHARDS)
    parser.add_argument("--check-only", action="store_true")
    args = parser.parse_args()
    if not args.check_only and args.shard is None:
        parser.error("choose a shard or use --check-only")

    binaries = load_binaries()
    if args.check_only:
        verify_partition(binaries)
        return 0
    return run_shard(binaries, args.shard)


if __name__ == "__main__":
    sys.exit(main())
