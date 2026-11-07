#!/usr/bin/env python3
"""Partition Cargo's default test suite without dropping targets or doctests."""

import argparse
from collections import Counter
import subprocess
import sys


# libtest filters are substrings. Each later partition excludes earlier ones,
# so the partition also covers new tests and names in integration test targets.
REPO_FIRST = [f"repo::{letter}" for letter in "abcdefghijklm"]
SHARDS = {
    "git": ["git::"],
    "repo-a-m": [*REPO_FIRST, "--skip", "git::"],
    "repo-rest": [
        "repo::", "--skip", "git::",
        *[arg for prefix in REPO_FIRST for arg in ("--skip", prefix)],
    ],
    "core": ["--skip", "git::", "--skip", "repo::"],
}


def inventory(filters):
    result = subprocess.run(
        ["cargo", "test", "--locked", "--", "--list", *filters],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    # Keep multiplicities: separate test binaries may use the same test name.
    return Counter(
        line.rsplit(": ", 1)[0]
        for line in result.stdout.splitlines()
        if line.endswith((": test", ": benchmark"))
    )


def verify_partition():
    full = inventory([])
    if not full:
        raise RuntimeError("Cargo listed no tests; refusing to run an empty partition")
    assigned = Counter()
    counts = {}
    for name, filters in SHARDS.items():
        tests = inventory(filters)
        if not tests:
            raise RuntimeError(f"The {name} shard has no tests")
        overlap = assigned & tests
        if overlap:
            raise RuntimeError(f"Tests assigned to multiple shards: {dict(overlap)}")
        assigned.update(tests)
        counts[name] = tests.total()
    if assigned != full:
        raise RuntimeError(
            f"Test partition differs from the full suite: "
            f"missing={dict(full - assigned)}, extra={dict(assigned - full)}"
        )
    print(f"Verified complete, disjoint test partition: {counts}; total={full.total()}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shard", nargs="?", choices=SHARDS)
    parser.add_argument("--check-only", action="store_true")
    args = parser.parse_args()
    if not args.check_only and args.shard is None:
        parser.error("choose a shard or use --check-only")
    verify_partition()
    if args.check_only:
        return 0
    print(f"Running the {args.shard} test shard", flush=True)
    return subprocess.run(
        ["cargo", "test", "--locked", "--", "--nocapture", *SHARDS[args.shard]]
    ).returncode


if __name__ == "__main__":
    sys.exit(main())
