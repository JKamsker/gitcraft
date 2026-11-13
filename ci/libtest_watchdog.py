#!/usr/bin/env python3
"""Run libtest (or `cargo test`) under a watchdog and record per-test reports (issue #211).

Each process gets a bounded deadline and an optional no-progress limit. While it
runs, a heartbeat names the finished/total count and the tests libtest reports as
long-running. On a timeout the owned process tree is captured, then stopped, and
the report names the unfinished tests. A JSON report and JUnit XML are rewritten
after every process, so a report exists even if the CI step itself is killed.

    python3 ci/libtest_watchdog.py --name linux-main --report-dir ci-reports \
        --deadline 1680 --stall 600 -- cargo test --locked -- --nocapture
"""

import argparse
from collections import Counter, deque
import json
import os
from pathlib import Path
import platform
import queue
import re
import subprocess
import sys
import threading
import time
from xml.sax.saxutils import escape, quoteattr

from process_tree import OwnedProcess

SCHEMA = "gitcraft-test-report/1"
LOG_CAP = 8 * 1024 * 1024
TAIL_LINES = 400
# After the test process exits, descendants may hold its output open this long.
OUTPUT_GRACE_SECONDS = 10
METADATA_ENV = ("GITHUB_REPOSITORY", "GITHUB_SHA", "GITHUB_REF", "GITHUB_EVENT_NAME", "GITHUB_WORKFLOW",
                "GITHUB_JOB", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "RUNNER_OS", "RUNNER_ARCH",
                "RUNNER_NAME", "RUNNER_ENVIRONMENT", "ImageOS", "ImageVersion")
SECRETS = [re.compile(pattern) for pattern in (
    r"gh[pousr]_[A-Za-z0-9]{20,}", r"github_pat_[A-Za-z0-9_]{20,}",
    r"(?i)(authorization:\s*(?:bearer|token|basic)\s+)\S+", r"(?i)(://[^/\s:@]+:)[^@\s/]+@")]

START = re.compile(r"^test (?P<name>.+?) \.\.\. ?(?P<rest>.*)$")
OUTCOME = re.compile(r"^(?P<result>ok|FAILED|ignored(?:, .*)?)$")
LONG = re.compile(r"^test (?P<name>.+) has been running for over (?P<secs>\d+) seconds$")
CARGO_RUN = re.compile(r"^\s+Running (?:\S+ )?(?P<source>\S+) \((?P<path>[^)]+)\)$")
DOCTESTS = re.compile(r"^\s+Doc-tests (?P<name>\S+)$")
PANIC = re.compile(r"^thread '(?P<name>[^']+)' panicked at (?P<rest>.*?):?$")
OLD_PANIC = re.compile(r"^'(?P<message>.*)', (?P<location>\S+:\d+:\d+)$")


def redact(text):
    for pattern in SECRETS:
        text = pattern.sub(lambda match: (match.group(1) if match.groups() else "") + "[REDACTED]", text)
    return text


def utc(timestamp):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(timestamp))


def outcome_of(result):
    return {"ok": "passed", "FAILED": "failed"}.get(result, "ignored")


class LibtestParser:
    """Incremental reader of libtest's pretty output, tolerant of interleaved --nocapture output."""

    def __init__(self, binary=""):
        self.binary = binary
        self.tests = {}
        self.pending = None
        self.long_running = {}
        self.messages = {}
        self.panic = None

    def key(self, name):
        return (self.binary, name)

    def feed(self, line, at):
        """Returns True when the line shows progress (a test started or finished)."""
        line = line.rstrip("\r\n")
        if self.panic is not None:
            name, location = self.panic
            self.panic = None
            if line.strip():
                self._failure(name, location, line.strip())
        match = CARGO_RUN.match(line)
        if match:
            self.binary = Path(match.group("path")).name
            return True
        match = DOCTESTS.match(line)
        if match:
            self.binary = f"doctests:{match.group('name')}"
            return True
        match = LONG.match(line)
        if match:
            self.long_running[self.key(match.group("name"))] = int(match.group("secs"))
            return False
        match = PANIC.match(line)
        if match:
            rest = match.group("rest")
            old = OLD_PANIC.match(rest)
            if old:
                self._failure(match.group("name"), old.group("location"), old.group("message"))
            else:
                self.panic = (match.group("name"), rest)
            return False
        match = START.match(line)
        if match:
            name, rest = match.group("name"), match.group("rest").strip()
            if OUTCOME.match(rest):
                self._finish(name, rest, at)
            else:
                self.tests.setdefault(self.key(name), {"outcome": "running"})["started_at"] = at
                self.pending = name
            return True
        match = OUTCOME.match(line.strip())
        if match and self.pending is not None:
            self._finish(self.pending, match.group("result"), at)
            return True
        return False

    def _finish(self, name, result, at):
        entry = self.tests.setdefault(self.key(name), {})
        entry["outcome"] = outcome_of(result)
        entry["finished_at"] = at
        if "started_at" in entry:
            entry["duration"] = round(at - entry["started_at"], 3)
        if self.pending == name:
            self.pending = None

    def _failure(self, name, location, message):
        # Threads a test spawns have other names; only test-named threads are attributed.
        self.messages.setdefault(self.key(name), (location, redact(message)[:500]))

    def finished(self):
        return {key for key, entry in self.tests.items() if entry.get("outcome") in ("passed", "failed", "ignored")}

    def running(self):
        """Tests known to be in progress: a serial start line or a long-running notice."""
        done = self.finished()
        started = {key for key, entry in self.tests.items() if entry.get("outcome") == "running"}
        return sorted((started | set(self.long_running)) - done)


class Report:
    """The shard/job report; rewritten atomically after every change."""

    def __init__(self, name, directory, clock=time.time):
        self.directory = Path(directory)
        (self.directory / "logs").mkdir(parents=True, exist_ok=True)
        self.clock = clock
        self.data = {"schema": SCHEMA, "name": name, "started": utc(clock()), "finished": None,
                     "outcome": "running", "metadata": metadata(), "processes": []}

    def add(self, process):
        self.data["processes"].append(process)
        self.write()

    def finish(self, outcome):
        self.data["outcome"] = outcome
        self.data["finished"] = utc(self.clock())
        self.write()

    def write(self):
        for name, text in (("report.json", json.dumps(self.data, indent=2, sort_keys=True)),
                           ("junit.xml", junit(self.data))):
            temporary = self.directory / (name + ".tmp")
            temporary.write_text(text + "\n", encoding="utf-8")
            os.replace(temporary, self.directory / name)


def command_version(command):
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, errors="replace", timeout=30)
        return result.stdout.strip() or None
    except (OSError, subprocess.SubprocessError):
        return None


def metadata():
    return {"platform": platform.platform(), "python": platform.python_version(), "cpus": os.cpu_count(),
            "git": command_version(["git", "--version"]), "rustc": command_version(["rustc", "--version"]),
            "env": {name: os.environ[name] for name in METADATA_ENV if name in os.environ}}


def junit(data):
    cases, failures, errors, skipped = [], 0, 0, 0
    for process in data["processes"]:
        for test in process["tests"]:
            name = quoteattr(test["name"])
            classname = quoteattr(f"{data['name']}.{test['binary']}")
            seconds = test.get("duration", 0)
            body = ""
            if test["outcome"] == "failed":
                failures += 1
                body = (f"<failure message={quoteattr(test.get('message', 'test failed'))}>"
                        f"{escape(test.get('location', ''))}</failure>")
            elif test["outcome"] in ("timeout", "not-finished"):
                errors += 1
                body = f"<error message={quoteattr(test['outcome'] + ': ' + process['outcome'])}/>"
            elif test["outcome"] == "ignored":
                skipped += 1
                body = "<skipped/>"
            cases.append(f'    <testcase classname={classname} name={name} time="{seconds}">{body}</testcase>')
    head = (f'  <testsuite name={quoteattr(data["name"])} tests="{len(cases)}" failures="{failures}" '
            f'errors="{errors}" skipped="{skipped}" timestamp="{data["started"]}">')
    return "\n".join(['<?xml version="1.0" encoding="UTF-8"?>', "<testsuites>", head, *cases,
                      "  </testsuite>", "</testsuites>"])


class LogFile:
    """A bounded per-process log: the head up to LOG_CAP bytes, then the last TAIL_LINES lines."""

    def __init__(self, path):
        self.path = path
        self.handle = open(path, "wb")
        self.written = 0
        self.tail = deque(maxlen=TAIL_LINES)
        self.truncated = False

    def write(self, line):
        self.tail.append(line)
        if self.written + len(line) <= LOG_CAP:
            self.handle.write(line)
            self.written += len(line)
        else:
            self.truncated = True

    def close(self):
        if self.truncated:
            self.handle.write(b"\n[watchdog] log capped; last lines follow\n" + b"".join(self.tail))
        self.handle.close()


def slug(text):
    return re.sub(r"[^A-Za-z0-9._-]+", "-", text).strip("-")[:80] or "process"


def run_process(report, label, command, cwd=None, *, binary="", kind="parallel", expected=None,
                deadline=None, stall=None, heartbeat=60.0, echo=None, env=None):
    """Run one process under the watchdog and add its record to the report.

    `deadline` and `stall` are seconds. `expected` is the libtest inventory of
    test names this process should run; it names unfinished tests on a timeout.
    """
    echo = echo or sys.stdout.buffer
    index = len(report.data["processes"])
    log = LogFile(report.directory / "logs" / f"{index:02d}-{slug(label)}.log")
    parser = LibtestParser(binary)
    record = {"label": label, "kind": kind, "binary": binary, "command": [str(part) for part in command],
              "cwd": str(cwd) if cwd else None, "started": utc(time.time()), "deadline": deadline,
              "stall_limit": stall, "log": str(log.path.relative_to(report.directory)), "warnings": []}
    began = last_progress = last_beat = time.monotonic()
    try:
        child = OwnedProcess([str(part) for part in command], cwd=cwd, env=env)
    except OSError as error:
        log.close()
        record.update(outcome="launch-error", error=str(error), duration=0, tests=[], exit_code=None)
        report.add(record)
        return record
    record["pid"] = child.pid
    record["warnings"].extend(child.warnings)
    lines = queue.Queue()

    def read():
        for raw in iter(child.stdout.readline, b""):
            lines.put((time.monotonic(), raw))
        lines.put(None)

    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    eof = False
    reason = None
    exited_at = None
    while True:
        try:
            item = lines.get(timeout=0.5)
        except queue.Empty:
            item = False
        now = time.monotonic()
        if item is None:
            eof = True
        elif item:
            at, raw = item
            echo.write(raw)
            log.write(redact(raw.decode("utf-8", "replace")).encode("utf-8"))
            if parser.feed(raw.decode("utf-8", "replace"), round(at - began, 3)):
                last_progress = at
        if not lines.qsize():
            echo.flush()
        status = child.poll()
        if status is not None and exited_at is None:
            exited_at = now
        if eof and status is not None:
            break
        if exited_at is not None and now - exited_at > OUTPUT_GRACE_SECONDS:
            reason = "output-held-open"
            break
        if now - last_beat >= heartbeat:
            last_beat = now
            report.data["active"] = beat(label, parser, expected, now - began, now - last_progress, deadline)
            report.write()
        if deadline is not None and now - began >= deadline:
            reason = "deadline"
            break
        if stall is not None and now - last_progress >= stall:
            reason = "stall"
            break

    elapsed = time.monotonic() - began
    if reason is not None or child.poll() is None:
        record["snapshot"] = child.snapshot()
        record["killed"] = child.kill_tree()
    else:
        leftover = child.snapshot()
        if leftover["processes"]:
            record["leaked"] = leftover
            record["killed"] = child.kill_tree()
            record["warnings"].append("descendants outlived the test process; they were stopped")
    try:
        exit_code = child.wait(timeout=30)
    except subprocess.TimeoutExpired:
        exit_code = None
        record["warnings"].append("process did not exit within 30 s of being stopped")
    reader.join(timeout=10)
    while True:
        try:
            item = lines.get_nowait()
        except queue.Empty:
            break
        if item:
            echo.write(item[1])
            log.write(redact(item[1].decode("utf-8", "replace")).encode("utf-8"))
            parser.feed(item[1].decode("utf-8", "replace"), round(item[0] - began, 3))
    echo.flush()
    child.close()
    log.close()
    record.update(exit_code=exit_code, duration=round(elapsed, 3), ended=utc(time.time()),
                  tests=tests_of(parser, expected, reason, exit_code))
    record["outcome"] = process_outcome(reason, exit_code, record["tests"])
    if reason == "output-held-open":
        record["warnings"].append("the test process exited but descendants held its output open; they were stopped")
    if reason is not None:
        record["running_at_timeout"] = [name for _, name in parser.running()]
        record["output_tail"] = [redact(line.decode("utf-8", "replace")).rstrip() for line in list(log.tail)[-60:]]
    report.data.pop("active", None)
    report.add(record)
    announce(record)
    return record


def tests_of(parser, expected, reason, exit_code):
    """Per-test outcomes; libtest's exit code stays authoritative for lines it could not parse."""
    running = set(parser.running())
    seen = {name for _, name in parser.tests} | {name for _, name in parser.long_running}
    keys = set(parser.tests) | set(parser.long_running)
    keys |= {(parser.binary, name) for name in (expected or Counter()).keys() - seen}
    if expected and len(expected) == 1:
        # An isolated process runs exactly one test: if it is unfinished, it is the one running.
        running.add((parser.binary, next(iter(expected))))
    tests = []
    for key in sorted(keys):
        entry = parser.tests.get(key, {})
        outcome = entry.get("outcome", "running")
        if outcome == "running":
            if reason in ("deadline", "stall"):
                outcome = "timeout" if key in running else "not-finished"
            elif exit_code == 0:
                outcome = "unknown"
            else:
                outcome = "failed" if key in parser.messages else "not-finished"
        item = {"binary": key[0], "name": key[1], **{k: v for k, v in entry.items() if k != "outcome"},
                "outcome": outcome}
        if key in parser.long_running:
            item["long_running_notice"] = parser.long_running[key]
        if key in parser.messages and outcome != "passed":
            item["location"], item["message"] = parser.messages[key]
        tests.append(item)
    return tests


def process_outcome(reason, exit_code, tests):
    if reason == "deadline":
        return "timeout"
    if reason == "stall":
        return "stalled"
    if exit_code == 0:
        return "passed"
    if any(test["outcome"] == "failed" for test in tests):
        return "failed"
    return "crashed"


def beat(label, parser, expected, elapsed, idle, deadline):
    total = sum((expected or Counter()).values()) or "?"
    running = ", ".join(name for _, name in parser.running()[:5]) or "unknown (parallel)"
    limit = f"/{deadline:.0f}s" if deadline else ""
    print(f"[watchdog {utc(time.time())} +{elapsed:.0f}s{limit}] {label}: {len(parser.finished())}/{total} "
          f"finished, last progress {idle:.0f}s ago; running: {running}", flush=True)
    return {"label": label, "elapsed": round(elapsed), "idle": round(idle), "finished": len(parser.finished()),
            "total": total, "running": [name for _, name in parser.running()], "at": utc(time.time())}


def announce(record):
    """GitHub annotations for every failing test and stopped process."""
    for test in record["tests"]:
        if test["outcome"] in ("failed", "timeout", "not-finished"):
            detail = test.get("message") or test.get("location") or record["outcome"]
            print(f"::error title={test['outcome']}: {test['name']}::{record['label']}: {detail}", flush=True)
    for warning in record["warnings"]:
        print(f"::warning title=watchdog::{record['label']}: {warning}", flush=True)
    if record["outcome"] not in ("passed", "failed"):
        running = ", ".join(record.get("running_at_timeout", [])) or "none reported"
        print(f"::error title=watchdog: {record['outcome']}::{record['label']} after {record['duration']:.0f}s; "
              f"exit {record['exit_code']}; running: {running}; {record.get('killed', '')}", flush=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--name", required=True, help="report name, e.g. linux-main")
    parser.add_argument("--report-dir", type=Path, required=True)
    parser.add_argument("--deadline", type=float, required=True, help="seconds before the process tree is stopped")
    parser.add_argument("--stall", type=float, help="seconds without a test starting or finishing")
    parser.add_argument("--heartbeat", type=float, default=60.0)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("give the command after --")
    report = Report(args.name, args.report_dir)
    record = run_process(report, args.name, command, deadline=args.deadline, stall=args.stall,
                         heartbeat=args.heartbeat)
    report.finish(record["outcome"])
    return 0 if record["outcome"] == "passed" else (record["exit_code"] or 1)


if __name__ == "__main__":
    sys.exit(main())
