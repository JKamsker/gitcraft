#!/usr/bin/env python3
"""Check baseline growth against a Git base; declarations are committed, never PR labels."""
import argparse
import json
import os
import re
from pathlib import Path
import subprocess
import sys

BASELINE = "ci/arch-fitness/baseline.json"
APPROVALS = "ci/arch-fitness/increases.json"


def growth(old, new):
    """Stable identities, not entry counts: removing debt cannot offset new debt."""
    result = set()
    for domain, budget in new["budgets"].items():
        before = old["budgets"].get(domain, {})
        for key in ("domains", "symbols", "exported"):
            value, prior = budget.get(key), before.get(key)
            if value is None:
                if prior is not None:
                    result.add(f"budget/{domain}/{key}=unlimited")
            elif (prior is not None and value > prior) or (not before and value > 0):
                result.add(f"budget/{domain}/{key}={value}")
    identities = {
        "forbidden_edges": ("from", "to"),
        "repo_tab_mutations": ("domain", "field"),
        "reexports": ("module", "path", "target_domain"),
    }
    for kind, keys in identities.items():
        def entries(baseline):
            return {json.dumps([e[k] for k in keys], separators=(",", ":")) for e in baseline[kind]}
        result.update(f"{kind}/{entry}" for entry in entries(new) - entries(old))
    for key in ("domains", "edges"):
        def members(baseline):
            return {json.dumps(member, separators=(",", ":"))
                    for cycle in baseline["cycles"] for member in cycle[key]}
        result.update(f"cycles/{key}/{entry}" for entry in members(new) - members(old))
    return result


def validate(old, new, prior, declarations):
    covered = set()
    for name, entry in declarations.items():
        if name in prior:
            if entry != prior[name]:
                raise ValueError(f"published increase declaration {name} must remain unchanged")
            continue
        if not all(isinstance(entry.get(k), str) and entry[k].strip()
                   for k in ("reason", "owner", "remediation")):
            raise ValueError(f"increase declaration {name} needs reason, owner and remediation")
        changes = entry.get("changes")
        if not isinstance(changes, list) or not all(isinstance(c, str) for c in changes):
            raise ValueError(f"increase declaration {name} needs a list of changes")
        covered.update(changes)
    if prior.keys() - declarations.keys():
        raise ValueError("published increase declarations must be preserved")
    missing = growth(old, new) - covered
    if missing:
        raise ValueError("undeclared baseline growth:\n" + "\n".join(sorted(missing)))


def read(root, revision, path, optional=False):
    exists = subprocess.run(["git", "cat-file", "-e", f"{revision}:{path}"], cwd=root,
                            capture_output=True)
    if exists.returncode:
        # Only the declarations file may be absent in older revisions.
        if optional:
            subprocess.run(["git", "rev-parse", "--verify", f"{revision}^{{commit}}"],
                           cwd=root, check=True, capture_output=True)
            return {}
        raise ValueError(f"cannot read {revision}:{path}")
    return json.loads(subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=root))


def commit(root, revision):
    return subprocess.check_output(["git", "rev-parse", "--verify", f"{revision}^{{commit}}"],
                                   cwd=root, text=True, stderr=subprocess.PIPE).strip()


def event_base(root, revision, event, before, ref_type="branch"):
    if event == "push" and ref_type != "tag":
        if not re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", before) or not before.strip("0"):
            raise ValueError("push before SHA missing or all-zero (new branch); cannot establish baseline range")
        # Compare the full push, even if before is not an ancestor after a force push.
        return commit(root, before)
    return commit(root, f"{revision}^1")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--base", help="target revision (compared via merge-base)")
    selection.add_argument("--event-range", action="store_true", help="CI event range from EVENT_NAME and BEFORE_SHA")
    parser.add_argument("--revision", default="HEAD")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args(argv)
    try:
        if args.event_range:
            base = event_base(args.root, args.revision, os.environ.get("EVENT_NAME", ""), os.environ.get("BEFORE_SHA", ""), os.environ.get("REF_TYPE", "branch"))
        else:
            base = subprocess.check_output(["git", "merge-base", args.base, args.revision], cwd=args.root, text=True).strip()
        validate(read(args.root, base, BASELINE), read(args.root, args.revision, BASELINE),
                 read(args.root, base, APPROVALS, True), read(args.root, args.revision, APPROVALS, True))
        print(f"Architecture baseline growth: declared or absent (base {base}).")
        return 0
    except (ValueError, KeyError, TypeError, OSError, subprocess.CalledProcessError) as error:
        print(f"Architecture baseline: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
