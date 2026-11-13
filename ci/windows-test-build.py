#!/usr/bin/env python3
"""Compile Windows test executables once and record the exact Cargo test targets.

Cargo decides where test executables live: target/debug/deps/ in the classic
layout, target/debug/build/<package>/<hash>/out/ in the build-dir layout of newer
toolchains. The manifest records Cargo's reported paths, and the same list goes to
the step output `upload-paths`, so the uploaded artifact holds exactly the
executables the shards load, wherever Cargo put them.
"""

import json
import os
from pathlib import Path
import subprocess
import sys


def write_upload_paths(paths):
    """Hand the artifact step the exact files to upload (a no-op outside Actions)."""
    github_output = os.environ.get("GITHUB_OUTPUT")
    if not github_output:
        return
    delimiter = "GITCRAFT_UPLOAD_PATHS"
    if any(delimiter in path for path in paths):
        raise RuntimeError("upload path collides with the step output delimiter")
    with open(github_output, "a", encoding="utf-8") as handle:
        handle.write(f"upload-paths<<{delimiter}\n" + "".join(f"{path}\n" for path in paths)
                     + f"{delimiter}\n")


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
        if message.get("reason") == "compiler-message":
            rendered = message.get("message", {}).get("rendered")
            print(rendered.rstrip() if rendered else line.rstrip(), flush=True)
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
    write_upload_paths([*sorted(binaries), output.relative_to(root).as_posix()])
    print(f"Built {len(binaries)} Windows test executables: {sorted(binaries)}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
