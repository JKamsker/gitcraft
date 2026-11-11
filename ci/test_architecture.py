"""Small adversarial fixtures for the lexical survey; no third-party dependencies."""

import unittest
import subprocess
import tempfile
from pathlib import Path
from unittest.mock import patch

from architecture import central_declaration, history, mask, named_fields, report, source_metrics, sources_at


class MetricsTests(unittest.TestCase):
    def test_central_declaration_fails_on_missing_or_ambiguous_owners(self):
        paths = ("src/repo/mod.rs", "src/repo/tab_owner/model.rs")
        with self.assertRaisesRegex(ValueError, "found 0"):
            central_declaration([], "RepoTab", paths)
        with self.assertRaisesRegex(ValueError, "found 2"):
            central_declaration([{"name": "RepoTab", "path": p} for p in paths], "RepoTab", paths)

    def test_literals_comments_lifetimes_and_nested_blocks(self):
        source = '''
// fn misleading() { if true {} }
/* nested /* fn bad() {} */ comment */
fn actual<'a>(s: &'a str) -> &'a str {
    let a = r##" } fn fake() { // "# still string "##;
    let b = "escaped \\" } fn fake() {}";
    let c = b'{';
    let d = '\\u{1f600}';
    if s.is_empty() { return "}"; }
    s
}
'''
        masked = mask(source)
        self.assertEqual(len(masked), len(source))
        self.assertEqual(masked.count("\n"), source.count("\n"))
        _, functions, _ = source_metrics("fixture.rs", source)
        self.assertEqual([f["name"] for f in functions], ["actual"])
        self.assertEqual(functions[0]["branch_tokens"], 1)
        self.assertEqual(functions[0]["lines"], 8)

    def test_trait_methods_and_named_fields(self):
        source = '''
trait T { fn declaration(&self); fn implemented(&self) { if true {} } }
pub struct State<'a> {
    #[allow(dead_code)]
    field: Vec<(String, usize)>,
    pub(crate) callback: fn() -> bool,
    reference: &'a str,
}
'''
        _, functions, structs = source_metrics("fixture.rs", source)
        self.assertEqual([f["name"] for f in functions], ["implemented"])
        self.assertEqual(structs[0]["fields"], ["field", "callback", "reference"])

    def test_nested_field_types_are_not_struct_fields(self):
        self.assertEqual(named_fields('''
    value: Something<{
        nested: false
    }>,
    after: bool,
'''), ["value", "after"])


class GitMetricsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Architecture fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "commit.gpgsign", "false")

    def git(self, *args, input=None):
        return subprocess.check_output(["git", "-C", str(self.root), *args], input=input)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def test_history_pins_rename_algorithm_and_path_quoting(self):
        path = self.root / "before ü.rs"
        path.write_text("\n".join(f"line {i}" for i in range(100)) + "\n")
        self.commit()
        path.rename(self.root / "after ü.rs")
        self.commit()
        with patch("architecture.git", side_effect=self.git):
            self.git("config", "diff.renames", "true")
            self.git("config", "diff.algorithm", "histogram")
            self.git("config", "core.quotePath", "false")
            before = history("HEAD", 1)
            self.git("config", "diff.renames", "false")
            self.git("config", "diff.algorithm", "patience")
            self.git("config", "core.quotePath", "true")
            after = history("HEAD", 1)
        self.assertEqual(before, after)
        self.assertEqual(len(after["files"]), 2)
        self.assertEqual(sum(f["added"] for f in after["files"].values()), 100)
        self.assertEqual(sum(f["deleted"] for f in after["files"].values()), 100)

    def test_sources_are_raw_tracked_blobs_despite_export_attributes(self):
        src = self.root / "src"
        src.mkdir()
        (src / "hidden.rs").write_text("fn hidden() {}\n")
        (src / "subst.rs").write_text('// $Format:%H$\nfn raw() {}\n')
        (src / "spaces ü.rs").write_text("fn spaced() {}\n")
        (self.root / ".gitattributes").write_text("src/hidden.rs export-ignore\nsrc/subst.rs export-subst\n")
        self.commit()
        (src / "hidden.rs").write_text("uncommitted replacement")
        with patch("architecture.git", side_effect=self.git):
            result = sources_at("HEAD")
        self.assertEqual(result["src/hidden.rs"], "fn hidden() {}\n")
        self.assertIn("$Format:%H$", result["src/subst.rs"])
        self.assertEqual(result["src/spaces ü.rs"], "fn spaced() {}\n")

    def test_report_selects_action_by_name_after_another_enum(self):
        repo = self.root / "src/repo"
        repo.mkdir(parents=True)
        (repo / "mod.rs").write_text('''
pub struct RepoTab {
    path: String,
}
impl RepoTab {
    pub fn new_deferred() -> Self { Self { path: String::new() } }
}
''')
        (repo / "apply.rs").write_text("impl RepoTab { fn apply(&mut self) {} }\n")
        (repo / "actions.rs").write_text('''
enum Earlier {
    Decoy,
}
pub(crate) enum Action {
    Select(usize),
    Reload,
}
enum Later {
    AnotherDecoy,
}
''')
        self.commit()
        with patch("architecture.git", side_effect=self.git):
            result = report("HEAD", 1)
        self.assertEqual(result["root_action_variants"], ["Select", "Reload"])
        self.assertEqual(result["summary"]["root_action_variants"], 2)

        # PR #200 moved ownership out of mod.rs. Survey both layouts so old
        # baselines remain reproducible and current candidates don't crash.
        owner = repo / "tab_owner"
        owner.mkdir()
        (owner / "model.rs").write_text("pub struct RepoTab {\n    path: String,\n}\n")
        (owner / "construction.rs").write_text(
            "impl RepoTab { pub fn new_deferred() -> Self { Self { path: String::new() } } }\n")
        (repo / "mod.rs").write_text("mod tab_owner;\n")
        self.commit()
        with patch("architecture.git", side_effect=self.git):
            moved = report("HEAD", 1)
        for key in ("repo_tab_fields", "constructor_lines", "dispatcher_lines", "root_action_variants"):
            self.assertEqual(moved["summary"][key], result["summary"][key])


if __name__ == "__main__":
    unittest.main()
