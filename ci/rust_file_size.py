#!/usr/bin/env python3
"""Enforce physical Rust file sizes for the full Git-tracked tree (issue #198).

Only LF bytes delimit lines; CRLF counts identically. A nonempty final fragment
is a line. Comments, blanks, generated and vendored files all count.
"""

import argparse
import os
from pathlib import Path
import subprocess
import sys

WARNING_LINES = 300
HARD_LIMIT = 500


def physical_lines(content):
    return content.count(b"\n") + int(bool(content) and not content.endswith(b"\n"))


def git(root, *args, input=None):
    return subprocess.run(
        ["git", "-C", os.fspath(root), *args], input=input,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
    ).stdout


def entries(root, revision=None, *, suffix=b".rs", ignore_case=False):
    """Return (path, mode, blob) without decoding Git's NUL-delimited paths."""
    if revision is not None:
        tree = git(root, "rev-parse", "--verify", "--end-of-options", revision + "^{tree}").strip()
        records = git(root, "ls-tree", "-rz", "--full-tree", tree.decode("ascii"))
    else:
        records = git(root, "ls-files", "--stage", "-z")
    result = []
    for record in records.split(b"\0"):
        if not record:
            continue
        metadata, path = record.split(b"\t", 1)
        mode, second, third = metadata.split()
        candidate = path.lower() if ignore_case else path
        if not candidate.endswith(suffix):
            continue
        if revision is None:
            if third != b"0":
                raise ValueError(f"unmerged Rust path: {os.fsdecode(path)!r}")
            blob = second
        else:
            if second != b"blob":
                raise ValueError(f"Rust path is not a blob: {os.fsdecode(path)!r}")
            blob = third
        if mode not in (b"100644", b"100755", b"120000"):
            raise ValueError(f"unsupported Rust path mode: {os.fsdecode(path)!r}")
        result.append((path, mode, blob))
    return result


def blob_contents(root, tracked):
    """Read exact index/tree blobs in one Git invocation, without text filters."""
    if not tracked:
        return []
    data = git(root, "cat-file", "--batch", input=b"".join(blob + b"\n" for _, _, blob in tracked))
    contents = []
    offset = 0
    for _, _, blob in tracked:
        end = data.index(b"\n", offset)
        actual, kind, size = data[offset:end].split()
        if actual != blob or kind != b"blob":
            raise ValueError("Git returned an unexpected blob")
        offset = end + 1
        length = int(size)
        contents.append(data[offset:offset + length])
        offset += length + 1
    return contents


def tracked_contents(root, *, staged=False, revision=None, suffix=b".rs", ignore_case=False,
                     reject_symlinks=False):
    """Read complete snapshots; policy callers may require regular source inputs.

    Size inventory counts Git's link contents. Configuration policy must instead
    reject links, since the compiler may follow their targets as active settings.
    """
    tracked = entries(root, revision, suffix=suffix, ignore_case=ignore_case)
    if staged or revision is not None:
        if reject_symlinks:
            for path, mode, _ in tracked:
                if mode == b"120000":
                    raise ValueError(f"symlink policy input is forbidden: {os.fsdecode(path)!r}")
        return [(os.fsdecode(path), content)
                for (path, _, _), content in zip(tracked, blob_contents(root, tracked))]
    result = []
    for path, _, _ in tracked:
        filename = root / os.fsdecode(path)
        try:
            linked = filename.is_symlink()
            if reject_symlinks and linked:
                raise ValueError(f"symlink policy input is forbidden: {os.fsdecode(path)!r}")
            # Read the link itself, as Git does, never an unrelated target.
            content = os.fsencode(os.readlink(filename)) if linked else filename.read_bytes()
        except FileNotFoundError:
            # An unstaged deletion is absent from the working candidate tree.
            # --staged and --revision still inspect its committed/index blob.
            continue
        result.append((os.fsdecode(path), content))
    return result


def inventory(root, *, staged=False, revision=None):
    return [(path, physical_lines(content))
            for path, content in tracked_contents(root, staged=staged, revision=revision)]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd(), help="repository or subdirectory")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--staged", action="store_true", help="check full index contents, ignoring working edits")
    mode.add_argument("--revision", help="check a complete committed tree (CI uses HEAD)")
    args = parser.parse_args(argv)
    try:
        # Git appends one LF; any earlier LF belongs to the directory name.
        # Decode as a filesystem path only after removing that byte terminator.
        top = git(args.root, "rev-parse", "--show-toplevel").removesuffix(b"\n")
        root = Path(os.fsdecode(top))
        files = inventory(root, staged=args.staged, revision=args.revision)
        warnings = errors = 0
        for path, lines in files:
            if lines < WARNING_LINES:
                continue
            severity = "error" if lines > HARD_LIMIT else "warning"
            errors += severity == "error"
            warnings += severity == "warning"
            print(f"{severity}: {path!r}: {lines} physical lines "
                  f"(warning at {WARNING_LINES}; hard limit {HARD_LIMIT}); "
                  "decompose into cohesive owners/helpers")
        print(f"Rust file size: {len(files)} tracked files checked; {warnings} warnings; "
              f"{errors} errors (warning at {WARNING_LINES}; hard limit {HARD_LIMIT}).")
        return int(errors > 0)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Rust file size: cannot check complete tree: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
