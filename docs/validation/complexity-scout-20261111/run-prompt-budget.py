#!/usr/bin/env python3
"""Reproduce the pure parser boundary check; this is not an application test run."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

REVISION = "84a15b24af458c14e81f68068b1dce13b0d90999"
SOURCES = {
    "src/git/prompt_session/parse.rs": "src/parse.rs",
    "src/git/prompt_session/parse/records.rs": "src/parse/records.rs",
    "src/git/prompt_session/tests/budget.rs": "src/tests/budget.rs",
}
HARNESS = r'''#![allow(dead_code)]
use std::path::{Path, PathBuf};
mod parse;
#[derive(Debug, Default)]
struct Step {kind:String,text:String,at:Option<i64>,path:Option<String>,before:String,after:String,exact:bool}
impl Step {fn new(kind:&str,text:String,at:Option<i64>)->Self{Self{kind:kind.into(),text,at,..Default::default()}}}
fn redact(s:&str)->String{s.into()}
mod i18n {
 pub fn text(s:&str)->&str{match s {
 "backend-prompt-5c3912"=>"Prompt",
 "backend-agent-summary-8351fe"=>"Agent summary",
 "backend-tool-2e53bd"=>"Tool",
 _=>s}}
 pub struct Arg; impl From<String> for Arg {fn from(_:String)->Self{Self}}
 pub fn message(s:&str,_args:&[(&str,Arg)])->String{s.into()}
}
#[cfg(test)] mod tests {
 use super::*;
 fn fixture_path(s:&str)->PathBuf{
  if cfg!(windows){PathBuf::from(format!("C:{s}"))}else{PathBuf::from(s)}
 }
 fn fixture_records(b:&[u8])->Vec<u8>{
  if cfg!(windows){String::from_utf8_lossy(b).replace("\"/", "\"C:/").into_bytes()}
  else{b.to_vec()}
 }
 mod budget;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repository", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    crate = Path(tempfile.mkdtemp(prefix="gitcraft-prompt-budget-"))
    hashes = {}
    for source, target in SOURCES.items():
        data = subprocess.check_output(
            ["git", "show", f"{REVISION}:{source}"], cwd=args.repository
        )
        hashes[source] = hashlib.sha256(data).hexdigest()
        destination = crate / target
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    (crate / "src/lib.rs").write_text(HARNESS, encoding="utf-8")
    (crate / "Cargo.toml").write_text(
        '[package]\nname="gitcraft-prompt-budget-scout"\nversion="0.0.0"\n'
        'edition="2024"\n[dependencies]\nserde_json="1"\n', encoding="utf-8"
    )
    (crate / "clippy.toml").write_text(
        "cognitive-complexity-threshold = 25\n", encoding="utf-8"
    )
    environment = dict(os.environ, CARGO_TARGET_DIR=str(crate / "target"))
    results = []

    def run(name, arguments, expected):
        separator = arguments.index("--") if "--" in arguments else len(arguments)
        command = [args.cargo, *arguments[:separator], "--manifest-path",
                   str(crate / "Cargo.toml"), *arguments[separator:]]
        result = subprocess.run(command, env=environment, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, check=False)
        (args.output / f"{name}.log.gz").write_bytes(gzip.compress(result.stdout, mtime=0))
        results.append({"name": name, "command": command,
                        "exit_code": result.returncode, "expected": expected})
        if result.returncode != expected:
            raise RuntimeError(f"{name}: {result.returncode}, expected {expected}")

    run("fixed", ["test", "--offline", "--quiet", "--lib"], 0)
    run("clippy", ["clippy", "--offline", "--quiet", "--all-targets", "--",
                   "-F", "clippy::cognitive_complexity", "-D", "warnings"], 0)
    records = crate / "src/parse/records.rs"
    fixed = records.read_text(encoding="utf-8")
    assert fixed.count("steps.truncate(2000);") == 1
    records.write_text(fixed.replace("steps.truncate(2000);", ""), encoding="utf-8")
    run("negative-control", ["test", "--offline", "--quiet", "--lib"], 101)
    records.write_text(fixed, encoding="utf-8")
    run("restored", ["test", "--offline", "--quiet", "--lib"], 0)
    metadata = {"revision": REVISION, "source_sha256": hashes, "results": results,
                "cargo_version": subprocess.check_output([args.cargo, "--version"], text=True).strip(),
                "harness_sha256": hashlib.sha256(HARNESS.encode()).hexdigest(),
                "limitations": "Copied parser and real regression; identity redaction, minimal Step and i18n stubs. No application runtime claim."}
    (args.output / "results.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(json.dumps({"revision": REVISION, "results": results}, indent=2))


if __name__ == "__main__":
    main()
