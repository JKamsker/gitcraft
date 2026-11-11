#!/usr/bin/env python3
"""Run strict all-target Clippy for every Git-tracked Cargo package (Python 3.11+).

Explicit package selection includes workspace members omitted from defaults and
independent workspaces. Only the selected platform/features/cfg are compiled.
"""

import argparse
import os
from pathlib import Path
import subprocess
import sys
import tomllib

from rust_file_size import git, tracked_contents

RECORDING_MANIFEST = "src/resolver/park/checks/Cargo.toml"


def packages(root):
    # Read index paths first: a missing working manifest must fail, not disappear
    # from the size reader's deliberately permissive working-tree inventory.
    tracked = tracked_contents(root, staged=True, suffix=b"cargo.toml",
                               ignore_case=True, reject_symlinks=True)
    result = []
    for relative, _ in sorted(tracked):
        if Path(relative).name.casefold() != "cargo.toml":
            continue
        manifest = root / relative
        if any((root / parent).is_symlink() for parent in Path(relative).parents) or manifest.is_symlink():
            raise ValueError(f"symlink Cargo manifest is forbidden: {relative}")
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        if "package" not in data:
            continue  # A virtual workspace has no compilation target of its own.
        package = data["package"]
        name = package.get("name") if isinstance(package, dict) else None
        if not isinstance(name, str) or not name:
            raise ValueError(f"Cargo package needs a name: {relative}")
        result.append((relative, name))
    if not result:
        raise ValueError("no tracked Cargo packages found")
    return result


def lint_packages(root, run=subprocess.run):
    inventory = packages(root)  # Validate the complete inventory before compiling.
    for manifest, name in inventory:
        command = ["cargo", "clippy", "--locked", "--manifest-path", manifest,
                   "--package", name, "--all-targets", "--", "-D", "warnings"]
        if manifest == RECORDING_MANIFEST:
            # This isolated harness replaces Git boundaries and is designed for
            # this cfg; pass it to the selected crate without changing dependency flags.
            command.extend(["--cfg", "recovery_recording"])
        print(f"Clippy package: {name} ({manifest})", flush=True)
        run(command, cwd=root, check=True)
    print(f"Clippy packages: {len(inventory)} checked.")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args(argv)
    try:
        root = Path(os.fsdecode(git(args.root, "rev-parse", "--show-toplevel").removesuffix(b"\n")))
        lint_packages(root)
        return 0
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Clippy packages: cannot validate every tracked package: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
