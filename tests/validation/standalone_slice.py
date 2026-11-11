#!/usr/bin/env python3
"""Static regression evidence for the issue-198 standalone slice; no processes run.

Run from the repository root. Uses the immutable assigned base, not live CI data.
This is an extraction audit, not a replacement for compilation or native tests.
"""
import hashlib
import importlib.util
import re
import subprocess
from pathlib import Path

BASE = "a4d841713af3d31178a3b9212fd3c5adcb4172dc"
spec = importlib.util.spec_from_file_location("architecture", "ci/architecture.py")
architecture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(architecture)


def original(path):
    return subprocess.check_output(["git", "show", f"{BASE}:{path}"], text=True)


def functions(source):
    """Read complete function bodies using the existing literal-aware brace scanner."""
    masked = architecture.mask(source)
    found = {}
    for match in re.finditer(r"\bfn\s+(\w+)\b", masked):
        start = masked.index("{", match.end())
        end = architecture.block_end(masked, start)
        found[match[1]] = source[match.start():end]
    return found


def normalized(source):
    # rustfmt changes indentation and may remove trailing commas. Source assets
    # and protocol literals receive an additional exact-byte comparison below.
    return re.sub(r"\s+|,(?=\s*[)\]])", "", source)


def check_functions(before, after, replacements=()):
    old = functions(before)
    new = functions(after)
    for name, body in old.items():
        assert name in new, f"lost function: {name}"
        for source, target in replacements:
            body = body.replace(source, target)
        assert normalized(body) == normalized(new[name]), f"changed function: {name}"
    return len(old)


root = Path("src/standalone")
# Includes keep these test functions in the original save::tests namespace.
save_tests = original(root / "save.rs").split("mod tests {", 1)[1]
current_tests = (root / "save/tests.rs").read_text()
includes = re.findall(r'include!\("([^"]+)"\)', current_tests)
assert len(includes) == 5
current_tests += "\n".join((root / "save" / p).read_text() for p in includes)
count = check_functions(save_tests, current_tests, [
    ("&lock.name", "lock.name()"),
    ("lock.recovery.dir()?.path.clone()", "lock.recovery_path()?"),
])
for path, modules in [
    ("ui", ["tests", "localization_tests"]),
    ("windows_metadata", ["tests", "fixture_recording_tests",
                          "full_policy_fixture_recording_tests", "diagnostic_localization_tests"]),
]:
    before = original(root / f"{path}.rs")
    for module in modules:
        match = re.search(rf"mod {module}\s*\{{", before)
        start = before.index("{", match.start())
        end = architecture.block_end(architecture.mask(before), start)
        body = before[start + 1:end - 1]
        after = (root / path / f"{module}.rs").read_text()
        count += check_functions(body, after)
        # External module declarations preserve qualified Windows shard names.
        parent = (root / f"{path}.rs").read_text()
        assert f'#[path = "{path}/{module}.rs"]\nmod {module};' in parent

# MCP assertion names stay at the integration-test root, transport remains private.
mcp = Path("tests/mcp_stdio.rs")
count += check_functions(original(mcp), mcp.read_text() + Path("tests/support/mcp_stdio.rs").read_text(), [
    ("server.stdin = None;", "server.close_stdin();"),
    ("if let Ok(Some(_)) = server.child.try_wait() {", "if server.has_exited() {"),
])
protocol = original(root / "windows_metadata.rs").split('const SCRIPT: &str = r#"', 1)[1].split('"#;', 1)[0]
asset = (root / "windows_metadata/policy.ps1").read_text()
assert protocol == asset, "PowerShell native protocol bytes changed"
print(f"PASS: {count} existing functions retained with equivalent bodies")
print(f"PASS: native protocol SHA256 {hashlib.sha256(asset.encode()).hexdigest()}")

paths = set(subprocess.check_output(["git", "ls-files", "src/standalone", "tests"], text=True).splitlines())
paths.update(str(p) for directory in [root, Path("tests")] for p in directory.rglob("*.rs"))
counts = {p: len(Path(p).read_bytes().splitlines()) for p in paths if p.endswith(".rs")}
assert all(n <= 500 for n in counts.values()), counts
print(f"PASS: {len(counts)} owned Rust files <=500 lines; maximum {max(counts.values())}")
