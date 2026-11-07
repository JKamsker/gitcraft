#!/usr/bin/env python3
"""Measure source lines separately from CPU timing; retain reports even when tests fail.

Requires cargo-llvm-cov and llvm-tools-preview for Rust 1.95.0.
python3 ci/coverage.py --workflows --html
python3 ci/coverage.py --append --filter git::status::tests
Start a fresh run after changing production source; --append accumulates additional tests.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--append", action="store_true")
    parser.add_argument("--filter", help="Run matching library tests instead of all workspace targets")
    parser.add_argument("--workflows", action="store_true", help="Also exercise the opt-in offline workload suite")
    parser.add_argument("--html", action="store_true")
    args = parser.parse_args()
    out = ROOT / "target/coverage"
    out.mkdir(parents=True, exist_ok=True)
    cargo = ["cargo", "+1.95.0", "llvm-cov"]
    command = cargo + ["--locked", "--no-report", "--no-fail-fast"]
    command += ["--package", "gitcraft", "--lib"] if args.filter else ["--workspace", "--all-targets"]
    command += ["--"] + ([args.filter] if args.filter else []) + ["--test-threads=8"]
    env = os.environ.copy()
    for name in ["GITCRAFT_BENCH_FILTER", "GITCRAFT_BENCH_EXACT"]:
        env.pop(name, None)
    records = []

    def run(label, command):
        with (out / f"{label}.log").open("w") as log:
            print("Running:", " ".join(command), flush=True)
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        records.append({"label": label, "command": command, "exit": result.returncode})
        return result.returncode

    # --no-report already preserves profiles in cargo-llvm-cov. Start explicitly fresh
    # unless accumulating tests; --no-report and --no-clean cannot be combined.
    status = 0 if args.append else run("clean", cargo + ["clean", "--workspace"])
    status |= run("tests", command)
    if args.workflows:
        inputs = ROOT / "target/cpu-bench/inputs.json"
        if not inputs.exists():
            parser.error("Run ci/bench-cpu.py first to freeze workload inputs")
        env.update(GITCRAFT_BENCH_INPUTS=str(inputs), GITCRAFT_BENCH_OUTPUT=str(out / "workflows.json"),
                   GITCRAFT_BENCH_SAMPLES="1", GITCRAFT_BENCH_SAMPLE_MS="1")
        status |= run("workflows", cargo + ["--package", "gitcraft", "--lib", "--locked", "--no-report", "--",
            "repo::cpu_benchmarks::real_world_cpu", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
    report = out / "coverage.json"
    status |= run("report", cargo + ["report", "--json", "--output-path", str(report)])
    if args.html:
        status |= run("html", cargo + ["report", "--html", "--output-dir", str(out / "html")])
    (out / "run.json").write_text(json.dumps({"runs": records}, indent=2))
    if report.exists():
        data = json.loads(report.read_text())["data"][0]
        lines = data["totals"]["lines"]
        print(f"Instrumented source lines: {lines['covered']}/{lines['count']} ({lines['percent']:.2f}%)")
        print("Raw denominator includes inline tests; conditional platform/feature code needs separate runs.")
        missed = sorted(data["files"], key=lambda f: f["summary"]["lines"]["count"] - f["summary"]["lines"]["covered"], reverse=True)
        print("Largest remaining files:")
        for file in missed[:15]:
            line = file["summary"]["lines"]
            print(f"  {Path(file['filename']).relative_to(ROOT)}: {line['covered']}/{line['count']} ({line['percent']:.1f}%)")
    raise SystemExit(status)


if __name__ == "__main__":
    main()
