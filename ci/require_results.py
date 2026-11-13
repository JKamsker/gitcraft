#!/usr/bin/env python3
"""Fail unless every needed job succeeded: the one required check of CI (issue #211).

Branch protection and merge queues treat a *skipped* required check as passing,
so the aggregate job runs with `if: always()` and decides here instead:

- success passes;
- skipped passes only for jobs named with --allow-skipped (conditional lanes);
- failure fails;
- cancelled fails too, but is reported as cancelled (superseded or stopped),
  never as a test failure or as green.

    NEEDS='${{ toJSON(needs) }}' python3 ci/require_results.py --allow-skipped forges
"""

import argparse
import json
import os
import sys


def evaluate(needs, allow_skipped=()):
    """Returns (passed, lines) for a `toJSON(needs)` mapping."""
    if not isinstance(needs, dict) or not needs:
        return False, ["no needed jobs: refusing an empty aggregate"]
    unknown = sorted(set(allow_skipped) - needs.keys())
    if unknown:
        return False, [f"--allow-skipped names jobs that are not needed: {', '.join(unknown)}"]
    lines, passed = [], True
    for name in sorted(needs):
        result = (needs[name] or {}).get("result")
        if result == "success":
            lines.append(f"{name}: success")
        elif result == "skipped" and name in allow_skipped:
            lines.append(f"{name}: skipped (conditional lane, allowed)")
        elif result == "skipped":
            passed = False
            lines.append(f"{name}: skipped, but it is required: a skipped check must not count as passing")
        elif result == "cancelled":
            passed = False
            lines.append(f"{name}: CANCELLED (superseded or stopped; not a test failure, and not a pass)")
        elif result == "failure":
            passed = False
            lines.append(f"{name}: FAILED")
        else:
            passed = False
            lines.append(f"{name}: unknown result {result!r}")
    return passed, lines


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--allow-skipped", action="append", default=[])
    args = parser.parse_args(argv)
    try:
        needs = json.loads(os.environ.get("NEEDS", ""))
    except json.JSONDecodeError as error:
        print(f"::error::NEEDS is not JSON: {error}")
        return 1
    passed, lines = evaluate(needs, args.allow_skipped)
    for line in lines:
        print(line)
    if not passed:
        cancelled = any("CANCELLED" in line for line in lines)
        failed = any("FAILED" in line or "skipped, but" in line or "unknown" in line for line in lines)
        kind = "failed" if failed else ("was cancelled" if cancelled else "failed")
        print(f"::error title=Required checks {kind}::" + "; ".join(line for line in lines if not line.endswith(("success", "allowed)"))))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
