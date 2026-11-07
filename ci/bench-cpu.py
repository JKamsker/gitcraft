#!/usr/bin/env python3
"""Freeze real GitCraft inputs, then benchmark release code. No network or repository writes.

python3 ci/bench-cpu.py --label before
python3 ci/bench-cpu.py --label after
python3 ci/bench-cpu.py --label profile --binary target/cpu-bench/before-bench --filter highlight --sample-ms 1500

Keep target/cpu-bench/inputs.json unchanged for comparable runs; delete it to refresh inputs.
Saved binaries allow interleaving before/after runs without rebuilding.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--build-dir", type=Path, default=ROOT, help="Build a checkout containing the benchmark module")
    parser.add_argument("--no-default-features", action="store_true")
    parser.add_argument("--filter", default="")
    parser.add_argument("--samples", type=int, default=9)
    parser.add_argument("--sample-ms", type=int, default=50)
    parser.add_argument("--compare", type=Path, help="Check text/colour fingerprints against an earlier result")
    args = parser.parse_args()
    if args.samples < 1 or args.sample_ms < 1:
        parser.error("--samples and --sample-ms must be positive")
    out = ROOT / "target/cpu-bench"
    out.mkdir(parents=True, exist_ok=True)
    inputs = out / "inputs.json"
    if not inputs.exists():
        # Commits and source bytes remain immutable even while the worktree is edited.
        revision = git("rev-parse", "HEAD")
        revisions = sorted(set(git("for-each-ref", "--format=%(objectname)", "refs/heads", "refs/remotes").splitlines()))
        patches = []
        for rev in git("log", "--no-merges", "-n", "30", "--format=%H", revision, "--", "src").splitlines():
            paths = git("diff-tree", "--no-commit-id", "--name-only", "-r", rev).splitlines()
            candidates = [(len(git("show", "--format=", rev, "--", p)), p) for p in paths if p.endswith(".rs")]
            if candidates:
                size, path = max(candidates)
                if size > 2000:
                    patches.append({"revision": rev, "path": path})
            if len(patches) == 3:
                break
        inputs.write_text(json.dumps({"repo": str(ROOT), "revision": revision, "revisions": revisions,
            "sources": ["src/app/mod.rs", "src/repo/load.rs", "Cargo.lock"], "diffs": patches}, indent=2))
    if args.binary:
        binary = args.binary.resolve()
    else:
        command = ["cargo", "+1.95.0", "test", "--release", "--locked", "--lib", "--no-run", "--message-format=json"]
        if args.no_default_features:
            command.append("--no-default-features")
        build_env = os.environ.copy()
        build_env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
        build = subprocess.run(command,
                               cwd=args.build_dir, env=build_env, capture_output=True, text=True)
        (out / f"{args.label}-build.log").write_text(build.stderr)
        if build.returncode:
            print(build.stderr)
            for line in build.stdout.splitlines():
                if line.startswith("{"):
                    message = json.loads(line).get("message", {})
                    if message.get("rendered"):
                        print(message["rendered"])
            raise SystemExit(build.returncode)
        artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
        executables = [a["executable"] for a in artifacts if a.get("reason") == "compiler-artifact"
                       and a.get("executable") and a["target"]["name"] == "gitcraft"]
        if not executables:
            raise SystemExit("Cargo did not produce a library test executable")
        binary = out / f"{args.label}-bench"
        shutil.copy2(executables[-1], binary)
    (out / f"{args.label}-metadata.json").write_text(json.dumps({
        "platform": platform.platform(), "machine": platform.machine(),
        "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "inputs_sha256": hashlib.sha256(inputs.read_bytes()).hexdigest(),
        "samples": args.samples, "sample_ms": args.sample_ms, "filter": args.filter,
    }, indent=2))
    env = os.environ.copy()
    env.update(GITCRAFT_BENCH_INPUTS=str(inputs), GITCRAFT_BENCH_OUTPUT=str(out / f"{args.label}.json"),
               GITCRAFT_BENCH_FILTER=args.filter, GITCRAFT_BENCH_SAMPLES=str(args.samples),
               GITCRAFT_BENCH_SAMPLE_MS=str(args.sample_ms))
    with (out / f"{args.label}.log").open("w") as log:
        process = subprocess.Popen([str(binary), "repo::cpu_benchmarks::real_world_cpu", "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                                   cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (out / f"{args.label}.pid").write_text(str(process.pid))
        print(f"Benchmark PID: {process.pid}", flush=True)
        for line in process.stdout:
            print(line, end="", flush=True)
            log.write(line)
        result = process.wait()
    if result == 0 and args.compare:
        reference = json.loads(args.compare.read_text())["highlight_fingerprints"]
        current = json.loads((out / f"{args.label}.json").read_text())["highlight_fingerprints"]
        if current != reference:
            raise SystemExit("Highlighted text/colour fingerprints changed")
        print("Highlighted text/colour fingerprints match.")
    raise SystemExit(result)


if __name__ == "__main__":
    main()
