#!/usr/bin/env python3
"""Run actual owner transitions with an inert, recording-only read transport.

This is a pure state gate, not an application build or a test of Reader itself.
No worker, Git command, mutation, or child cancellation is executed. The harness
copies the production State and its implementation verbatim and includes the
production Requests module and lifecycle tests by path.
"""
from pathlib import Path
import subprocess
import tempfile

owner = Path(__file__).resolve().parent
source = (owner.parent / "cochanges.rs").read_text()
state = source[source.index("#[derive(Default)]\npub(super) struct State"):source.index("impl RepoTab {")]
scaffold = r'''
#![allow(dead_code)]
mod egui { pub struct Context; }
mod owner {
use crate::egui;
use std::{collections::BTreeSet, path::PathBuf};
struct History { records: Vec<()>, notes: Vec<String>, skipped: usize, limited: bool }
type Hint = ();
struct Completion { request: Request, result: Result<History, String> }
enum Effect { Read { request: Request, path: PathBuf, head: String }, None }
struct Reader<T>(std::marker::PhantomData<T>);
impl<T> Default for Reader<T> { fn default() -> Self { Self(Default::default()) } }
impl<T> Reader<T> {
    fn cancel(&mut self) {}
    fn queue(&mut self, _: impl FnOnce(&dyn Fn() -> bool) -> Result<T, String>) {
        panic!("transition tests must not submit reads");
    }
    fn poll(&mut self, _: &egui::Context) -> Option<Result<T, String>> {
        panic!("transition tests must not poll a transport");
    }
}
mod cochange {
    use super::*;
    pub fn load(_: &std::path::Path, _: &str, _: &dyn Fn() -> bool) -> Result<History, String> {
        panic!("transition tests must not load Git history");
    }
    pub fn rank(_: &History, _: &BTreeSet<String>, _: &BTreeSet<String>) -> Vec<Hint> {
        panic!("transition tests must not substitute ranking behavior");
    }
}
'''
# Use the production oid validator too: validity changes must affect the gate.
git_source = (owner.parents[1] / "git/investigations/mod.rs").read_text()
start = git_source.index("pub(super) fn oid(") if "pub(super) fn oid(" in git_source else git_source.index("pub(crate) fn oid(")
end = git_source.index("\n}", start) + 2
validator = git_source[start:end].replace("pub(super)", "pub").replace("pub(crate)", "pub")
scaffold = scaffold.replace("mod owner {", "mod git { pub mod investigations {\n" + validator + "\n} }\nmod owner {", 1)
scaffold += '#[path = ' + repr(str(owner / 'requests.rs')).replace("'", '"') + '] mod requests;\nuse requests::{Request, Requests};\n'
scaffold += state
scaffold += '#[path = ' + repr(str(owner / 'lifecycle_tests.rs')).replace("'", '"') + '] mod lifecycle_tests;\n}\n'
with tempfile.TemporaryDirectory(prefix="gitcraft-cochange-transitions-") as temporary:
    harness = Path(temporary) / "harness.rs"
    binary = Path(temporary) / "tests"
    harness.write_text(scaffold)
    subprocess.run(["rustc", "--edition", "2024", "--test", str(harness), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
