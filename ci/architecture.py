#!/usr/bin/env python3
"""Reproducible, advisory architecture metrics using only Python and Git.

This is a lexical survey, not a Rust parser. See docs/architecture.md for scope
and limitations. Never use its size/branch counts as correctness or CI gates.
"""

import argparse
import collections
import io
import json
import re
import statistics
import subprocess
import tarfile
from pathlib import Path


def git(*args):
    return subprocess.check_output(["git", *args])


def mask(source):
    """Blank comments and literals, preserving offsets/newlines (including raw strings)."""
    pattern = re.compile(
        r'//[^\n]*|/\*|(?:br|cr|r)(\#*)"|(?:b|c)?"|'
        r"(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\n])|[^'\\\n])'"
    )
    out = list(source)
    at = 0
    while match := pattern.search(source, at):
        start, end = match.span()
        text = match.group()
        if text == "/*":
            depth = 1
            while depth and end < len(source):
                nested = re.search(r"/\*|\*/", source[end:])
                if not nested:
                    end = len(source)
                    break
                depth += 1 if nested.group() == "/*" else -1
                end += nested.end()
        elif text.endswith('"'):
            if match.group(1) is not None:
                closing = '"' + match.group(1)
                close = source.find(closing, end)
                end = len(source) if close < 0 else close + len(closing)
            else:
                literal = re.match(r'(?:\\[\s\S]|[^"\\])*"', source[end:])
                end = end + literal.end() if literal else len(source)
        for i in range(start, end):
            if out[i] != "\n":
                out[i] = " "
        at = end
    return "".join(out)


def block_end(code, start):
    depth = 0
    for i in range(start, len(code)):
        if code[i] == "{":
            depth += 1
        elif code[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(code)


def declarations(code, kind):
    for match in re.finditer(r"\b" + kind + r"\s+(\w+)", code):
        start = match.start()
        # A trait declaration ends at ';'; don't assign it the next method's body.
        boundary = re.search(r"[;{]", code[match.end():])
        if boundary is None or boundary.group() == ";":
            continue
        body = match.end() + boundary.start()
        end = block_end(code, body)
        yield {
            "name": match.group(1),
            "line": code.count("\n", 0, start) + 1,
            "lines": code.count("\n", start, end) + 1,
            "body": code[body + 1:end - 1],
        }


def named_fields(body):
    """Top-level named fields; generic/type braces and attributes may nest."""
    fields = []
    depth = 0
    for line in body.splitlines():
        if depth == 0:
            field = re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:(?!:)", line)
            if field:
                fields.append(field.group(1))
        depth += sum(line.count(c) for c in "{([")
        depth -= sum(line.count(c) for c in "})]")
    return fields


def source_metrics(path, source):
    code = mask(source)
    functions = []
    for decl in declarations(code, "fn"):
        body = decl.pop("body")
        # A deliberately simple branching indicator, not cyclomatic/cognitive complexity.
        decl["branch_tokens"] = len(re.findall(r"\b(?:if|for|while|loop|match)\b|&&|\|\||\?", body))
        functions.append({"path": path, **decl})
    structs = []
    for decl in declarations(code, "struct"):
        body = decl.pop("body")
        structs.append({"path": path, **decl, "fields": named_fields(body)})
    return code, functions, structs


def sources_at(revision):
    archive = git("archive", "--format=tar", revision, "src", "crates")
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        return {
            entry.name: tar.extractfile(entry).read().decode("utf-8")
            for entry in tar
            if entry.isfile() and entry.name.endswith(".rs")
        }


def history(revision, count):
    log = git("log", "--first-parent", "--diff-merges=first-parent", f"-{count}",
              "--format=COMMIT %H", "--numstat", revision).decode("utf-8")
    churn = collections.defaultdict(lambda: {"commits": 0, "added": 0, "deleted": 0})
    commits = []
    sizes = []
    for line in log.splitlines():
        if line.startswith("COMMIT "):
            commits.append(line.split()[1])
            sizes.append(0)
        elif match := re.fullmatch(r"(\d+)\t(\d+)\t(.+)", line):
            added, deleted, path = match.groups()
            item = churn[path]
            item["commits"] += 1
            item["added"] += int(added)
            item["deleted"] += int(deleted)
            sizes[-1] += 1
    return {
        "commits_newest_first": commits,
        "median_text_files_per_commit": statistics.median(sizes) if sizes else 0,
        "files": dict(sorted(churn.items())),
    }


def report(revision, count):
    revision = git("rev-parse", "--verify", revision + "^{commit}").decode().strip()
    sources = sources_at(revision)
    codes = {}
    functions = []
    structs = []
    for path, source in sorted(sources.items()):
        code, found_functions, found_structs = source_metrics(path, source)
        codes[path] = code
        functions.extend(found_functions)
        structs.extend(found_structs)
    tab = next(s for s in structs if s["path"] == "src/repo/mod.rs" and s["name"] == "RepoTab")
    fields = tab["fields"]
    constructor = next(f for f in functions if f["path"] == "src/repo/mod.rs" and f["name"] == "new_deferred")
    dispatch = next(f for f in functions if f["path"] == "src/repo/apply.rs" and f["name"] == "apply")
    action_code = next(declarations(codes["src/repo/actions.rs"], "enum"))["body"]
    action_variants = re.findall(r"^    (\w+)(?:\s*[,({])", action_code, re.M)
    # References identify collaborators for a manual writes/invariants audit; they aren't writes.
    references = {field: {} for field in fields}
    access = re.compile(r"\b(?:self|tab)\s*\.\s*(\w+)\b")
    for path, code in codes.items():
        counts = collections.Counter(access.findall(code))
        for field in fields:
            if counts[field]:
                references[field][path] = counts[field]
    lengths = sorted(f["lines"] for f in functions)
    return {
        "schema": 1,
        "revision": revision,
        "tree": git("rev-parse", revision + "^{tree}").decode().strip(),
        "method": "Lexical, all tracked Rust in src/ and crates/, including tests and cfg branches; see docs/architecture.md",
        "summary": {
            "rust_files": len(sources),
            "rust_lines": sum(len(s.splitlines()) for s in sources.values()),
            "functions": len(functions),
            "largest_function_lines": max(lengths, default=0),
            "function_lines_p95": lengths[(len(lengths) - 1) * 95 // 100] if lengths else 0,
            "repo_tab_fields": len(fields),
            "constructor_lines": constructor["lines"],
            "dispatcher_lines": dispatch["lines"],
            "root_action_variants": len(action_variants),
        },
        "largest_functions": sorted(functions, key=lambda f: (-f["lines"], f["path"], f["line"]))[:25],
        "largest_structs": sorted(structs, key=lambda s: (-len(s["fields"]), s["path"], s["line"]))[:15],
        "repo_tab_field_references": references,
        "root_action_variants": action_variants,
        "history": history(revision, count),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", default="HEAD", help="immutable source/history endpoint")
    parser.add_argument("--commits", type=int, default=207, help="first-parent commits to survey")
    parser.add_argument("--output", type=Path, help="write full JSON (otherwise stdout)")
    parser.add_argument("--compare", type=Path, help="print summary deltas against a saved report")
    args = parser.parse_args()
    if args.commits < 1:
        parser.error("--commits must be positive")
    result = report(args.revision, args.commits)
    encoded = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(encoded)
    else:
        print(encoded, end="")
    if args.compare:
        before = json.loads(args.compare.read_text())
        print(json.dumps({key: {"before": before["summary"][key], "after": value,
                               "delta": value - before["summary"][key]}
                          for key, value in result["summary"].items()}, indent=2))


if __name__ == "__main__":
    main()
