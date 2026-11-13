#!/usr/bin/env python3
"""Decide whether a CI run must exercise the live Forgejo/Gitea matrix (issue #211).

The live forge tests are slow, so pull requests run them only when they touch
the forge adapters/transport or what the lane itself depends on. When the base
is unknown (a new branch, a shallow history, an all-zero `before`), the lane
runs: an unknown diff never skips verification.

    python3 ci/changed_paths.py --base <sha|HEAD^1> --output "$GITHUB_OUTPUT"
"""

import argparse
import fnmatch
import subprocess
import sys

FORGE_PATHS = (
    "src/forge/*",              # every forge adapter, its HTTP transport and the live tests
    "ci/forge-server.sh",       # the servers the lane starts
    "ci/changed_paths.py",      # this filter
    ".github/workflows/ci.yml",  # the lane's own definition
    "Cargo.lock",               # HTTP/TLS dependency changes reach the transport
)


def forge_relevant(paths, patterns=FORGE_PATHS):
    return sorted(path for path in paths if any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns))


def changed(base, head="HEAD"):
    if not base or set(base) == {"0"}:
        return None
    try:
        result = subprocess.run(["git", "diff", "--name-only", "--no-renames", base, head, "--"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=120)
    except (OSError, subprocess.SubprocessError):
        return None
    if result.returncode:
        return None
    return [line for line in result.stdout.splitlines() if line]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--base", default="")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--output", help="append forge=true|false here (GITHUB_OUTPUT)")
    args = parser.parse_args(argv)
    paths = changed(args.base, args.head)
    if paths is None:
        relevant, reason = True, f"cannot diff against {args.base or 'an unknown base'}; running the lane"
    else:
        matches = forge_relevant(paths)
        relevant = bool(matches)
        reason = f"forge-relevant changes: {', '.join(matches)}" if matches else "no forge-relevant changes"
    print(f"forge={'true' if relevant else 'false'}: {reason}")
    if args.output:
        with open(args.output, "a", encoding="utf-8") as output:
            output.write(f"forge={'true' if relevant else 'false'}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
