#!/usr/bin/env python3
"""Compile the production typed plan and pure helpers without app dependencies."""
from pathlib import Path
import json
import subprocess
import tempfile

owner = Path(__file__).resolve().parent
repo_source = owner.parent

def function(source, name):
    marker = source.index(f"fn {name}(")
    start = source.rfind("\n", 0, marker) + 1
    return source[start:source.index("\n}", marker) + 2]

def constant(source, name):
    return next(line for line in source.splitlines() if f"const {name}:" in line)

expectations = (owner / "expectations.rs").read_text()
dispatch = (owner / "dispatch.rs").read_text()
plan = (owner / "model.rs").read_text()
plan = plan[:plan.index("#[cfg(test)]")].replace("use super::*;", "", 1).replace("//!", "//")
helpers = "\n".join(function(expectations, name) for name in
    ["expected_ref", "expected_symref", "expected_tree", "is_expectation"])
helpers += "\n" + function(dispatch, "retain_unfinished")
constants = "\n".join(constant(expectations, name) for name in
    ["EXPECT", "EXPECT_SYMREF", "EXPECT_TREE", "TRY", "EXPECT_FOLD_SAFE", "EXPECT_FOLD_IGNORED"])
constants += "\n" + constant((repo_source / "operations.rs").read_text(), "EXPECT_HTTPS_ROUTE")
fork_source = (repo_source / "forks/guard.rs").read_text()
fork = "\n".join([constant(fork_source, "MARK"), constant(fork_source, "CHECKOUT_MARK"), function(fork_source, "is_guard")])
gerrit_source = (repo_source.parent / "gerrit.rs").read_text()
gerrit = "\n".join(constant(gerrit_source, name) for name in ["AMEND", "CHECK"])
push_source = (repo_source.parent / "push_preflight.rs").read_text()
push = "\n".join(constant(push_source, name) for name in ["APPROVED", "FIX"])
ops = function((repo_source / "mod.rs").read_text(), "ops")
harness = f'''
#![allow(dead_code)]
mod gerrit {{ {gerrit} }}
mod push_preflight {{ {push} }}
mod repo {{
mod forks {{ pub mod guard {{ {fork} }} }}
mod operations {{
{ops}
{constants}
{helpers}
{plan}
#[path = {json.dumps(str(owner / 'model_pure_tests.rs'))}] mod pure_tests;
}}
}}
'''
with tempfile.TemporaryDirectory(prefix="gitcraft-typed-plan-") as temporary:
    source = Path(temporary) / "harness.rs"
    binary = Path(temporary) / "tests"
    source.write_text(harness)
    subprocess.run(["rustc", "--edition", "2024", "--test", str(source), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
