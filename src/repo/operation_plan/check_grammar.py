#!/usr/bin/env python3
"""Pure classifier gate against the Git owner's shared source (no Git processes).

Usage: python3 check_grammar.py /path/to/Git-owner/src/git/global_options.rs
The caller integrates that owner's commit separately; this script reads it only.
"""
from pathlib import Path
import json
import subprocess
import sys
import tempfile

owner = Path(__file__).resolve().parent
repo_source = owner.parent
shared = Path(sys.argv[1]).resolve()
dispatch = (owner / "dispatch.rs").read_text()
start = dispatch.index("pub(crate) fn reaches_a_remote(")
classifier = dispatch[start:dispatch.index("\n}", start) + 2]
envelope = (owner / "envelope.rs").read_text()
fixtures = envelope[envelope.index("#[cfg(test)]"):]

def constant(path, name):
    return next(line for line in path.read_text().splitlines()
                if f"const {name}:" in line).replace("pub(in crate::repo)", "pub(crate)")

harness = f'''
#![allow(dead_code)]
#[path = {json.dumps(str(shared))}] mod global_options;
mod git {{
    pub use crate::global_options::*;
    pub mod dvc {{ {constant(repo_source.parent / 'git/dvc.rs', 'PULL_ALIAS')} }}
}}
mod push_preflight {{ {constant(repo_source.parent / 'push_preflight.rs', 'APPROVED')} }}
#[path = {json.dumps(str(owner / 'grammar_adapter.rs'))}] mod grammar_adapter;
use grammar_adapter::command_envelope;
{constant(owner / 'expectations.rs', 'EXPECT')}
{constant(owner / 'expectations.rs', 'TRY')}
fn ops(list: &[&[&str]]) -> Vec<Vec<String>> {{
    list.iter().map(|op| op.iter().map(|s| s.to_string()).collect()).collect()
}}
{classifier}
{fixtures}
'''
with tempfile.TemporaryDirectory(prefix="gitcraft-operation-grammar-") as temporary:
    source = Path(temporary) / "harness.rs"
    binary = Path(temporary) / "tests"
    source.write_text(harness)
    subprocess.run(["rustc", "--edition", "2024", "--test", str(source), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
