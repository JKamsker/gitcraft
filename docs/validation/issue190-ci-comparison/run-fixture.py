"""Replay the archived, network-disabled CI comparison source fixture."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="fresh fixture/build directory on E:")
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    evidence = Path(__file__).resolve().parent
    archive = evidence / "fixture.zip"
    binding = json.loads((evidence / "source-binding.json").read_text())
    assert digest(archive.read_bytes()) == binding["archive_sha256"]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixture = output / "fixture"
    with zipfile.ZipFile(archive) as bundle:
        for entry in bundle.infolist():
            assert not entry.filename.startswith("/")
            assert ".." not in Path(entry.filename).parts
        bundle.extractall(fixture)
    for name, expected in binding["archive_files_sha256"].items():
        assert digest((fixture / name).read_bytes()) == expected, name
    env = os.environ.copy()
    env.pop("GIT_DIR", None)
    env.pop("GIT_WORK_TREE", None)
    env["CARGO_TARGET_DIR"] = str(output / "target")
    env["TMPDIR"] = str(output / "tmp")
    Path(env["TMPDIR"]).mkdir()
    results = []

    def run(name, command, failure=False):
        result = subprocess.run(command, cwd=fixture, env=env, capture_output=True)
        log = result.stdout + result.stderr
        (output / f"{name}.log.gz").write_bytes(gzip.compress(log, mtime=0))
        results.append({"name": name, "command": command,
                        "exit_code": result.returncode, "log_sha256": digest(log)})
        if failure:
            assert result.returncode != 0 and b"test result: FAILED" in log, name
        else:
            assert result.returncode == 0, log.decode(errors="replace")

    selectors = [
        ("history", "ci::timing::history::tests::"),
        ("alignment", "insights::ci_path::alignment::"),
        ("diagnostics", "insights::ci_path::diagnostics::tests::"),
        ("load", "insights::ci_path::load::tests::"),
        ("workflow", "insights::ci_path::workflow::tests::"),
    ]

    def suite(prefix):
        for name, selector in selectors:
            run(f"{prefix}-{name}", [args.cargo, "test", "--locked", "--offline", selector])

    suite("fixed")
    mutations = [
        ("matrix-axis-identity", "src/insights/ci_path/alignment.rs",
         'format!("{bindings:?}")', '{ let _ = bindings; suffix.to_owned() }',
         "insights::ci_path::alignment::matrix_tests::"),
        ("missing-sha", "src/insights/ci_path/diagnostics.rs",
         '!current.info.sha.is_empty()\n                        && ', '',
         "insights::ci_path::diagnostics::tests::missing_revision"),
    ]
    for name, path, before, after, selector in mutations:
        source = fixture / path
        original = source.read_text()
        assert original.count(before) == 1, name
        try:
            source.write_text(original.replace(before, after))
            run(f"negative-{name}", [args.cargo, "test", "--locked", "--offline", selector], True)
        finally:
            source.write_text(original)

    source = fixture / "src/insights/ci_path/load.rs"
    original = source.read_text()
    start = original.index("fn earlier(")
    end = original.index("\nfn github_history", start)
    try:
        source.write_text(original[:start] + """fn earlier(candidate: &RunInfo, current: &RunInfo) -> bool {
    candidate.id != current.id
        && candidate.created.zip(current.created).is_none_or(|(a, b)| a <= b)
}
""" + original[end:])
        run("negative-prior-order", [args.cargo, "test", "--locked", "--offline",
                                    "insights::ci_path::load::tests::previous_run"], True)
    finally:
        source.write_text(original)
    suite("restored")
    run("clippy", [args.cargo, "clippy", "--locked", "--offline", "--all-targets", "--", "-D", "warnings"])
    metadata = {
        "source_commit": binding["source_commit"],
        "archive_sha256": binding["archive_sha256"],
        "runner_sha256": digest(Path(__file__).read_bytes()),
        "output": str(output),
        "cargo_version": subprocess.check_output([args.cargo, "--version"], text=True).strip(),
        "rustc_version": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "results": results,
        "limitations": "Copied-source model fixture only; network, Git and timestamps stubbed. No application/UI/runtime/API integration claim.",
    }
    (output / "results.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    main()
