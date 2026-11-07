#!/usr/bin/env python3
"""Compile Windows test executables once and record the exact Cargo test targets."""

import json
from pathlib import Path
import subprocess
import sys


def main():
    root = Path.cwd().resolve()
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            text=True,
            encoding="utf-8",
        )
    )
    package_roots = {
        package["id"]: Path(package["manifest_path"]).resolve().parent.relative_to(root).as_posix()
        for package in metadata["packages"]
    }

    print("Compiling all Windows test targets once", flush=True)
    proc = subprocess.Popen(
        ["cargo", "test", "--locked", "--no-run", "--message-format=json"],
        stdout=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    binaries = {}
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
        # Keep Cargo's per-package working directory, including integration tests.
        path = binary.relative_to(root).as_posix()
        binaries[path] = {
            "path": path,
            "cwd": package_roots[message["package_id"]],
        }

    status = proc.wait()
    if status:
        return status
    if not binaries:
        raise RuntimeError("Cargo reported no Windows test executables")

    for name in binaries:
        if not (root / name).is_file():
            raise RuntimeError(f"Cargo test executable does not exist: {name}")

    output = root / "ci" / "test-binaries.json"
    output.write_text(
        json.dumps({"binaries": [binaries[name] for name in sorted(binaries)]}, indent=2) + "\n"
    )
    print(f"Built {len(binaries)} Windows test executables: {sorted(binaries)}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
