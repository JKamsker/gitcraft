#!/usr/bin/env python3
"""Type-check real endpoint bodies with a recording reader, without Cargo or Git execution.

Only serde derives are removed for this std-only build. Application services (profile,
URL scope, translations) are stubs: assertions about those services are out of scope.
Run from any directory with Python 3 and rustc installed.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile

source = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix="gitcraft-route-recording-") as directory:
    root = Path(directory)
    shutil.copytree(source, root / "endpoints")
    body = source.with_suffix(".rs").read_text()
    body = body.replace("serde::Serialize, serde::Deserialize, ", "")
    (root / "endpoints.rs").write_text(body)
    shutil.copy2(source.parents[1] / "global_options.rs", root / "global_options.rs")
    shutil.copytree(source.parents[1] / "global_options", root / "global_options")
    (root / "harness.rs").write_text(r'''
#![allow(dead_code)]
use std::path::Path;
mod i18n { pub fn text(key: &str) -> String { key.into() } }
mod connections {
    #[derive(Clone)] pub struct Context;
    pub fn current() -> Context { Context }
}
mod git {
    #[path = "../global_options.rs"] mod global_options;
    pub use global_options::parse_command_envelope;
    pub struct CredentialScope(String);
    impl CredentialScope {
        pub fn for_url(url: &str, _: &str) -> Result<Self, String> { Ok(Self(url.into())) }
        pub fn matches_url(&self, url: &str) -> bool { self.0 == url }
    }
}
fn connection_read(_: &Path, _: &[&str]) -> Result<String, String> {
    panic!("production process boundary reached by recording-only harness")
}
fn after_options<'a>(args: &'a [&'a str]) -> &'a [&'a str] {
    git::parse_command_envelope(args).map(|(at, _)| &args[at..]).unwrap_or(&[])
}
fn is_auth_error(_: &str) -> bool { false }
mod endpoints;
''')
    # Rust resolves a module's relative #[path] against its containing directory.
    (root / "git").mkdir()
    binary = root / "recording-tests"
    subprocess.run(["rustc", "--edition=2024", "--test", str(root / "harness.rs"),
                    "-o", str(binary)], check=True)
    for selection in ["recording_tests::", "global_options::tests::", "literal::tests::"]:
        subprocess.run([str(binary), selection, "--nocapture"], check=True)
