#!/usr/bin/env python3
"""The release version, shared by Release and the PR packaging smoke (issue #211).

    compute   the version and file label a run builds (GITHUB_OUTPUT format)
    apply     write a version into Cargo.toml and Cargo.lock (LF or CRLF)
    rpm       the RPM form of a version: rpm forbids `-`, so `0.3.4-dev.6`
              becomes `0.3.4~dev.6`, which sorts before 0.3.4 as it should

A dev build is the next patch after the last release tag, as a prerelease
`X.Y.Z-dev.N` (N is the run number); Cargo.toml's version itself while it has no
tag yet. A tag or a run by hand builds Cargo.toml's version.
"""

import argparse
from pathlib import Path
import re
import subprocess
import sys

SEMVER = re.compile(r"^(?P<core>\d+\.\d+\.\d+)(?:-(?P<pre>[0-9A-Za-z.]+))?$")
RPM_VERSION = re.compile(r"^[0-9A-Za-z._+~^]+$")
PACKAGE_VERSION = re.compile(r'(?m)^version = "([^"]*)"')
LOCK_ENTRY = re.compile(r'(name = "gitcraft"\r?\nversion = ")[^"]*')


def cargo_version(root):
    match = PACKAGE_VERSION.search((Path(root) / "Cargo.toml").read_text(encoding="utf-8"))
    if not match:
        raise ValueError("Cargo.toml has no package version")
    return match.group(1)


def tag_exists(version, remote="origin"):
    result = subprocess.run(["git", "ls-remote", "--exit-code", "--tags", remote, f"refs/tags/v{version}"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
    if result.returncode not in (0, 2):
        raise RuntimeError(f"git ls-remote failed with {result.returncode}")
    return result.returncode == 0


def compute(version, dev, ref_name, run_number, released=tag_exists):
    if not SEMVER.match(version):
        raise ValueError(f"not a release version: {version!r}")
    if dev:
        if released(version):
            major_minor, patch = version.rsplit(".", 1)
            version = f"{major_minor}.{int(patch) + 1}"
        version = f"{version}-dev.{int(run_number)}"
        label = f"v{version}"
    else:
        # Made safe for a file name (a branch like `feature/x` has a slash).
        label = re.sub(r"[^A-Za-z0-9._-]", "-", ref_name)
    return {"version": version, "label": label, "prerelease": "true" if dev else "false"}


def rpm_version(version):
    match = SEMVER.match(version)
    if not match:
        raise ValueError(f"not a release version: {version!r}")
    converted = version.replace("-", "~")
    validate_rpm(converted)
    return converted


def validate_rpm(version):
    """The rule rpmbuild applies: no `-`, and only these characters."""
    if not version or "-" in version or not RPM_VERSION.match(version):
        raise ValueError(f'invalid RPM version "{version}"')
    return version


def apply(root, version):
    if not SEMVER.match(version):
        raise ValueError(f"not a release version: {version!r}")
    root = Path(root)
    manifest, lock = root / "Cargo.toml", root / "Cargo.lock"
    text = manifest.read_bytes().decode("utf-8")
    text, count = PACKAGE_VERSION.subn(f'version = "{version}"', text, count=1)
    if count != 1:
        raise ValueError("Cargo.toml has no package version")
    manifest.write_bytes(text.encode("utf-8"))
    text = lock.read_bytes().decode("utf-8")
    text, count = LOCK_ENTRY.subn(lambda match: match.group(1) + version, text, count=1)
    if count != 1:
        raise ValueError("Cargo.lock has no gitcraft package")
    lock.write_bytes(text.encode("utf-8"))
    if cargo_version(root) != version or f'version = "{version}"' not in lock.read_text(encoding="utf-8"):
        raise ValueError("version was not applied")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    computing = commands.add_parser("compute")
    computing.add_argument("--dev", choices=("true", "false"), required=True)
    computing.add_argument("--ref-name", default="")
    computing.add_argument("--run-number", default="0")
    computing.add_argument("--output", type=Path, help="append name=value lines here (GITHUB_OUTPUT)")
    applying = commands.add_parser("apply")
    applying.add_argument("version")
    rpm = commands.add_parser("rpm")
    rpm.add_argument("version")
    args = parser.parse_args(argv)
    try:
        if args.command == "compute":
            result = compute(cargo_version("."), args.dev == "true", args.ref_name, args.run_number)
            lines = "".join(f"{key}={value}\n" for key, value in result.items())
            print(f"{result['version']}, files named {result['label']}")
            if args.output:
                with open(args.output, "a", encoding="utf-8") as output:
                    output.write(lines)
        elif args.command == "apply":
            apply(".", args.version)
            print(f"Cargo.toml and Cargo.lock carry {args.version}")
        else:
            print(rpm_version(args.version))
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"release_version: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
