#!/usr/bin/env python3
"""Summarize CI test reports and classify every failure with its evidence (issue #211).

Reads every report.json (ci/libtest_watchdog.py) below a directory, including
reports of earlier attempts of the same run, and clusters failures by test
identity and normalized failure signature:

- timeout-or-hang: the watchdog stopped the test's process (deadline/stall);
- possible-flaky: the same test failed and passed for the same SHA (a passing
  re-run does not erase the failure, and does not prove it harmless);
- deterministic-regression: failed with a panic and never passed for this SHA;
  a defect until a same-SHA pass proves otherwise;
- infrastructure-or-environment: the process failed to launch or crashed with
  no test failure attributed (missing binary, runner/OS failure);
- unknown: anything else, e.g. a failing test without a recorded panic.

Job results from `toJSON(needs)` (NEEDS) are listed too, so cancelled jobs show
as cancelled, never as failures or as passes. Writes Markdown to
GITHUB_STEP_SUMMARY when set and JSON with --json. Always exits 0.
"""

import argparse
import json
import os
from pathlib import Path
import re
import sys

CLASSES = ("deterministic-regression", "timeout-or-hang", "infrastructure-or-environment",
           "possible-flaky", "unknown")
FAILING = ("failed", "timeout", "not-finished")


def signature(test, process):
    if test["outcome"] in ("timeout", "not-finished") and process["outcome"] in ("timeout", "stalled"):
        return f"watchdog {process['outcome']}"
    message = test.get("message") or f"{test['outcome']} without a recorded panic"
    message = re.sub(r"0x[0-9a-fA-F]+", "0x…", message)
    message = re.sub(r"\d+(?:\.\d+)?", "N", message)
    message = re.sub(r"(?:[A-Za-z]:)?[\\/][^\s'\"`]+", "<path>", message)
    return message[:200]


def load(directory):
    reports = []
    for path in sorted(Path(directory).rglob("report.json")):
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if data.get("schema", "").startswith("gitcraft-test-report/"):
            data["_path"] = str(path)
            reports.append(data)
    return reports


def evidence(report, process):
    env = report.get("metadata", {}).get("env", {})
    return {"report": report["name"], "attempt": env.get("GITHUB_RUN_ATTEMPT"), "sha": env.get("GITHUB_SHA"),
            "runner": env.get("ImageOS") or env.get("RUNNER_OS") or report.get("metadata", {}).get("platform"),
            "process": process["label"], "process_outcome": process["outcome"], "log": process.get("log")}


def classify(reports):
    outcomes, clusters, process_failures = {}, {}, []
    for report in reports:
        sha = report.get("metadata", {}).get("env", {}).get("GITHUB_SHA")
        for process in report.get("processes", []):
            tests = process.get("tests", [])
            for test in tests:
                identity = (sha, test["binary"], test["name"])
                outcomes.setdefault(identity, set()).add("failed" if test["outcome"] in FAILING else test["outcome"])
                if test["outcome"] not in FAILING:
                    continue
                if process["outcome"] in ("timeout", "stalled") and test["outcome"] == "not-finished":
                    continue  # Queued behind the hang; the running tests carry the cluster.
                key = (identity, signature(test, process))
                cluster = clusters.setdefault(key, {"sha": sha, "binary": test["binary"], "test": test["name"],
                                                    "signature": key[1], "evidence": []})
                cluster["evidence"].append({**evidence(report, process), "outcome": test["outcome"],
                                            "location": test.get("location")})
            attributed = any(test["outcome"] in ("failed", "timeout") for test in tests)
            if process["outcome"] not in ("passed",) and not attributed:
                process_failures.append({**evidence(report, process),
                                         "class": classify_process(process),
                                         "error": process.get("error"), "exit_code": process.get("exit_code"),
                                         "running": process.get("running_at_timeout", [])})
    results = []
    for (identity, _), cluster in sorted(clusters.items(), key=lambda item: str(item[0])):
        seen = outcomes[identity]
        outcomes_here = {item["outcome"] for item in cluster["evidence"]}
        if "timeout" in outcomes_here:
            kind, why = "timeout-or-hang", "the watchdog stopped this test's process while it ran"
        elif "passed" in seen:
            kind, why = "possible-flaky", "the same test also passed for this SHA"
        elif "failed" in outcomes_here and any(item["location"] for item in cluster["evidence"]):
            kind, why = "deterministic-regression", (
                f"failed with a panic in {len(cluster['evidence'])} observation(s), never passed for this SHA")
        else:
            kind, why = "unknown", "failed without a recorded panic"
        results.append({**cluster, "class": kind, "why": why})
    return results, process_failures


def classify_process(process):
    if process["outcome"] in ("timeout", "stalled"):
        return "timeout-or-hang"
    if process["outcome"] in ("launch-error", "crashed"):
        return "infrastructure-or-environment"
    return "unknown"


def markdown(results, process_failures, needs, reports):
    lines = ["## CI failure summary", ""]
    if needs:
        lines += ["| Job | Result |", "| --- | --- |"]
        for name in sorted(needs):
            result = (needs[name] or {}).get("result")
            label = {"cancelled": "cancelled (superseded or stopped; not a failure)"}.get(result, result)
            lines.append(f"| {name} | {label} |")
        lines.append("")
    total = sum(len(process.get("tests", [])) for report in reports for process in report.get("processes", []))
    lines.append(f"{len(reports)} test report(s), {total} recorded test outcome(s).")
    if not results and not process_failures:
        lines += ["", "No test failures recorded."]
        return "\n".join(lines) + "\n"
    lines += ["", "| Class | Test | Signature | Evidence |", "| --- | --- | --- | --- |"]
    for item in results:
        where = "; ".join(f"{e['report']} attempt {e['attempt']} ({e['runner']}): {e['outcome']}"
                          for e in item["evidence"])
        lines.append(f"| {item['class']} | `{item['test']}` ({item['binary']}) | {item['signature']} | "
                     f"{where}. {item['why']} |")
    for item in process_failures:
        lines.append(f"| {item['class']} | process `{item['process']}` | {item['process_outcome']}, exit "
                     f"{item['exit_code']} {item.get('error') or ''} | {item['report']} attempt {item['attempt']} "
                     f"({item['runner']}); running: {', '.join(item['running']) or 'unknown'} |")
    shas = sorted({str(item["sha"]) for item in results})
    lines += ["", f"SHA(s): {', '.join(shas) or 'unknown'}. Logs and process snapshots are in the "
              "`test-report-*` artifacts."]
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("reports", type=Path)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args(argv)
    try:
        needs = json.loads(os.environ.get("NEEDS") or "{}")
    except ValueError:
        needs = {}
    reports = load(args.reports) if args.reports.is_dir() else []
    results, process_failures = classify(reports)
    text = markdown(results, process_failures, needs, reports)
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(text)
    if args.json:
        args.json.write_text(json.dumps({"failures": results, "process_failures": process_failures,
                                         "jobs": needs}, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
