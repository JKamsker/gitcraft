#!/usr/bin/env python3
"""Compile Windows test executables once and record the exact Cargo test targets."""

import json
from pathlib import Path
import subprocess
import sys


def main():
    root = Path.cwd().resolve()
    print("Compiling all Windows test targets once", flush=True)
    proc = subprocess.Popen(
        ["cargo", "test", "--locked", "--no-run", "--message-format=json"],
        stdout=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    binaries = set()
    assert proc.stdout is not None
    for line in proc.stdout:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            print(line.rstrip(), flush=True)
            continue
        if message.get("reason") != "compiler-artifact":
            continue
        if not message.get("profile", {}).get("test"):
            continue
        executable = message.get("executable")
        if not executable:
            continue
        binary = Path(executable).resolve()
        if binary.suffix.lower() != ".exe":
            continue
        # Avoid accidentally packaging an executable outside this checkout.
        binaries.add(binary.relative_to(root).as_posix())

    status = proc.wait()
    if status:
        return status
    if not binaries:
        raise RuntimeError("Cargo reported no Windows test executables")

    for name in binaries:
        if not (root / name).is_file():
            raise RuntimeError(f"Cargo test executable does not exist: {name}")

    output = root / "ci" / "test-binaries.json"
    output.write_text(json.dumps({"binaries": sorted(binaries)}, indent=2) + "\n")
    print(f"Built {len(binaries)} test executables: {sorted(binaries)}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
