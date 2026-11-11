#!/usr/bin/env python3
"""Protect the compiler-backed Clippy gate from local threshold overrides.

This checks policy configuration, not lexical complexity. Clippy does the
complexity analysis during compilation. Requires Python 3.11+.
"""

import argparse
from pathlib import Path
import re
import subprocess
import sys
import tomllib

from architecture import mask
from rust_file_size import git, tracked_contents

THRESHOLD = 25
OVERRIDE = re.compile(r"\b(?:r#)?clippy\s*::\s*(?:r#)?cognitive_complexity\s*=")


def policy_errors(sources):
    errors = []
    config_found = False
    packages = 0
    for path, content in sources:
        text = content.decode("utf-8")
        name = Path(path).name
        if path.endswith(".rs"):
            if OVERRIDE.search(mask(text)):
                errors.append(f"{path}: local Clippy complexity threshold overrides are forbidden")
        elif name in ("clippy.toml", ".clippy.toml"):
            if path != "clippy.toml":
                errors.append(f"{path}: use the repository's single clippy.toml")
                continue
            config_found = True
            limit = tomllib.loads(text).get("cognitive-complexity-threshold")
            if type(limit) is not int or limit != THRESHOLD:
                errors.append(f"{path}: cognitive-complexity-threshold must be {THRESHOLD}")
        elif name == "Cargo.toml":
            manifest = tomllib.loads(text)
            if "package" not in manifest:
                continue
            packages += 1
            lint = manifest.get("lints", {}).get("clippy", {}).get("cognitive_complexity")
            if isinstance(lint, dict):
                lint = lint.get("level")
            if lint != "forbid":
                errors.append(f"{path}: package must forbid clippy::cognitive_complexity")
    if not config_found:
        errors.append("missing tracked clippy.toml")
    if not packages:
        errors.append("no tracked Cargo packages checked")
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--staged", action="store_true")
    mode.add_argument("--revision")
    args = parser.parse_args(argv)
    try:
        root = Path(git(args.root, "rev-parse", "--show-toplevel").removesuffix(b"\n").decode())
        sources = []
        for suffix in (b".rs", b".toml"):
            sources.extend(tracked_contents(root, staged=args.staged, revision=args.revision, suffix=suffix))
        errors = policy_errors(sources)
        for error in errors:
            print(f"error: {error}")
        print(f"Clippy complexity policy: {len(errors)} errors; compiler threshold {THRESHOLD}.")
        return int(bool(errors))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Clippy complexity policy: cannot check complete tree: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
