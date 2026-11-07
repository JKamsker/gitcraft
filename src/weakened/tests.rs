use super::patch::{self, Mark};
use super::{Reason, scan_patch};

/// A diff of `path` from `old` lines to `new` lines in one hunk, as `git diff` writes it.
fn diff(path: &str, body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let old = lines.iter().filter(|l| !l.starts_with('+')).count();
    let new = lines.iter().filter(|l| !l.starts_with('-')).count();
    format!(
        "diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,{old} +1,{new} @@\n{body}\n"
    )
}

fn reasons(text: &str) -> Vec<Reason> {
    let mut found: Vec<Reason> = scan_patch(text)
        .iter()
        .flat_map(|f| f.flags.iter().map(|g| g.reason))
        .collect();
    found.sort();
    found.dedup();
    found
}

#[test]
fn removed_assertions_are_flagged_with_their_line() {
    let text = diff(
        "tests/login.rs",
        " fn logs_in() {\n     let user = login();\n-    assert_eq!(user.name, \"ana\");\n-    assert!(user.active);\n }",
    );
    let files = scan_patch(&text);
    assert_eq!(files.len(), 1);
    let flags = &files[0].flags;
    assert_eq!(flags.len(), 2);
    assert!(flags.iter().all(|f| f.reason == Reason::RemovedAssertions));
    assert_eq!(flags[0].line, Some(3));
    assert!(flags[0].evidence.contains("assert_eq!(user.name"));
    assert_eq!(flags[0].hunk, Some(0));
}

#[test]
fn assertions_rewritten_or_moved_are_not_flagged() {
    let rewritten = diff(
        "tests/a.py",
        " def test_x():\n-    assert f(1) == 2\n+    assert f(1) == 3",
    );
    assert!(reasons(&rewritten).is_empty());
}

#[test]
fn a_commented_out_assertion_counts_as_removed() {
    let text = diff(
        "src/lib.test.ts",
        " it('adds', () => {\n-  expect(add(1, 2)).toBe(3);\n+  // expect(add(1, 2)).toBe(3);\n });",
    );
    assert_eq!(reasons(&text), vec![Reason::RemovedAssertions]);
}

#[test]
fn removed_tests_and_deleted_test_files() {
    let removed = diff(
        "src/parse.rs",
        " mod tests {\n-    #[test]\n-    fn empty() {}\n }",
    );
    assert_eq!(reasons(&removed), vec![Reason::RemovedTests]);
    let deleted = "diff --git a/tests/test_login.py b/tests/test_login.py\ndeleted file mode 100644\nindex 1111111..0000000\n--- a/tests/test_login.py\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-def test_login():\n-    assert login()\n";
    let files = scan_patch(deleted);
    assert_eq!(files[0].path, "tests/test_login.py");
    // The file going is the one flag: not each assertion in it as well.
    assert_eq!(reasons(deleted), vec![Reason::DeletedTestFile]);
}

#[test]
fn skips_in_each_language() {
    let cases = [
        ("src/a.rs", "+#[ignore]"),
        ("src/a.rs", "+#[ignore = \"flaky\"]"),
        ("tests/test_a.py", "+@pytest.mark.skip(reason=\"later\")"),
        ("tests/test_a.py", "+    self.skipTest(\"no\")"),
        ("web/a.spec.ts", "+it.skip('works', () => {"),
        ("web/a.spec.ts", "+xit('works', () => {"),
        ("web/a.spec.ts", "+describe.only('focus', () => {"),
        ("src/test/java/ATest.java", "+    @Disabled"),
        ("tests/ATests.cs", "+    [Fact(Skip = \"broken\")]"),
        ("tests/ATests.cs", "+    [Ignore]"),
        ("pkg/a_test.go", "+\tt.Skip(\"later\")"),
        ("spec/a_spec.rb", "+  skip \"later\""),
    ];
    for (path, line) in cases {
        let text = diff(path, &format!(" context\n{line}"));
        assert_eq!(reasons(&text), vec![Reason::SkipsTests], "{path}: {line}");
    }
}

#[test]
fn skip_like_words_elsewhere_are_not_flagged() {
    let cases = [
        ("src/a.rs", "+let skip = 3;"),
        ("src/a.rs", "+// #[ignore]"),
        ("src/iter.rs", "+it.skip(2).next()"),
        ("lib/a.rb", "+skip \"not a test\""),
        ("src/a.py", "+fit(model, data)"),
        ("src/a.js", "+offset.skip(1)"),
        ("src/a.rs", "+let x = v[ignore_idx];"),
    ];
    for (path, line) in cases {
        let text = diff(path, &format!(" context\n{line}"));
        assert!(
            !reasons(&text).contains(&Reason::SkipsTests),
            "{path}: {line}"
        );
    }
}

#[test]
fn assertions_that_cannot_fail() {
    for line in [
        "+    assert True",
        "+    assert!(true);",
        "+  expect(true).toBe(true);",
        "+        self.assertTrue(True)",
        "+    Assert.True(true);",
    ] {
        let text = diff("tests/a.py", &format!(" def test_a():\n{line}"));
        assert!(reasons(&text).contains(&Reason::TrivialAssertion), "{line}");
    }
    let real = diff(
        "tests/a.py",
        " def test_a():\n+    assert True is not None or x",
    );
    assert!(!reasons(&real).contains(&Reason::TrivialAssertion));
}

#[test]
fn loosened_and_tightened_tolerances() {
    let looser = diff(
        "tests/test_num.py",
        "-    assert np.allclose(a, b, rtol=1e-6)\n+    assert np.allclose(a, b, rtol=1e-2)",
    );
    let files = scan_patch(&looser);
    let flag = &files[0].flags[0];
    assert_eq!(flag.reason, Reason::LoosenedTolerance);
    assert_eq!(
        flag.evidence,
        "line 1 (rtol 1e-6 → 1e-2): assert np.allclose(a, b, rtol=1e-2)"
    );
    let tighter = diff(
        "tests/test_num.py",
        "-    assert np.allclose(a, b, rtol=1e-2)\n+    assert np.allclose(a, b, rtol=1e-6)",
    );
    assert!(reasons(&tighter).is_empty());
    let digits = diff(
        "web/num.test.js",
        "-  expect(x).toBeCloseTo(0.3, 5);\n+  expect(x).toBeCloseTo(0.3, 1);",
    );
    assert_eq!(reasons(&digits), vec![Reason::LoosenedTolerance]);
    // One argument: no digits given on either side, only the value changed.
    let value = diff(
        "web/num.test.js",
        "-  expect(x).toBeCloseTo(0.3);\n+  expect(x).toBeCloseTo(0.29);",
    );
    assert!(reasons(&value).is_empty());
    let places = diff(
        "tests/test_a.py",
        "-        self.assertAlmostEqual(a, b, places=7)\n+        self.assertAlmostEqual(a, b, places=2)",
    );
    assert_eq!(reasons(&places), vec![Reason::LoosenedTolerance]);
}

#[test]
fn snapshot_rewritten_wholesale_but_not_touched_up() {
    let removed: String = (0..12).map(|i| format!("-old {i}\n")).collect();
    let added: String = (0..12).map(|i| format!("+new {i}\n")).collect();
    let whole = diff(
        "src/__snapshots__/view.test.ts.snap",
        &format!("{removed}{}", added.trim_end()),
    );
    assert_eq!(reasons(&whole), vec![Reason::SnapshotRewrite]);
    let touched = diff("tests/snapshots/a.snap", " a\n b\n c\n-d\n+e\n f\n g");
    assert!(reasons(&touched).is_empty());
}

#[test]
fn ci_allowed_to_fail_and_jobs_removed() {
    let fail = diff(
        ".github/workflows/ci.yml",
        "   test:\n     runs-on: ubuntu-latest\n+    continue-on-error: true",
    );
    assert_eq!(reasons(&fail), vec![Reason::CiAllowsFailure]);
    let job = diff(
        ".github/workflows/ci.yml",
        " jobs:\n-  lint:\n-    runs-on: ubuntu-latest\n-    steps:\n-      - run: cargo clippy\n   test:",
    );
    let files = scan_patch(&job);
    assert_eq!(files[0].flags.len(), 1);
    assert_eq!(
        files[0].flags[0].evidence,
        "old line 2 (the job `lint`): lint:"
    );
    let trigger = diff(
        ".github/workflows/ci.yml",
        " on:\n-  pull_request:\n   push:",
    );
    assert!(
        scan_patch(&trigger)[0].flags[0]
            .evidence
            .contains("`pull_request` trigger")
    );
    let renamed = diff(
        ".github/workflows/ci.yml",
        " jobs:\n-  lint:\n+  clippy:\n     runs-on: ubuntu-latest",
    );
    // A job renamed is a job removed (`lint` is gone), but one moved is not.
    assert_eq!(reasons(&renamed), vec![Reason::CiRemovedJob]);
    let moved = diff(
        ".github/workflows/ci.yml",
        " jobs:\n-  lint:\n+  lint:\n     runs-on: ubuntu-latest",
    );
    assert!(reasons(&moved).is_empty());
    let gitlab = diff(
        ".gitlab-ci.yml",
        " stages: [test]\n-unit:\n-  script: make test\n+allowed:\n+  allow_failure: true",
    );
    assert_eq!(
        reasons(&gitlab),
        vec![Reason::CiAllowsFailure, Reason::CiRemovedJob]
    );
    let env = diff(".github/workflows/ci.yml", "-  env:\n-    A: 1");
    assert!(reasons(&env).is_empty());
}

#[test]
fn no_verify_anywhere_but_in_comments() {
    let script = diff(
        "scripts/release.sh",
        " set -e\n+git commit --no-verify -m release",
    );
    assert_eq!(reasons(&script), vec![Reason::SkipsHooks]);
    let comment = diff("scripts/release.sh", " set -e\n+# never use --no-verify");
    assert!(reasons(&comment).is_empty());
}

#[test]
fn paths_with_spaces_unicode_and_quotes() {
    let spaced = diff("tests/my tests/ünï cödé_test.go", " x\n+\tt.Skip()");
    assert_eq!(
        scan_patch(&spaced)[0].path,
        "tests/my tests/ünï cödé_test.go"
    );
    let quoted = "diff --git \"a/tests/t\\303\\251st\\tx.py\" \"b/tests/t\\303\\251st\\tx.py\"\n--- \"a/tests/t\\303\\251st\\tx.py\"\n+++ \"b/tests/t\\303\\251st\\tx.py\"\n@@ -1 +1,2 @@\n x\n+@pytest.mark.skip\n";
    assert_eq!(scan_patch(quoted)[0].path, "tests/tést\tx.py");
}

#[test]
fn several_files_and_hunks_are_kept_apart() {
    let text = format!(
        "{}{}",
        diff("src/a.rs", " a\n+#[ignore]"),
        "diff --git a/src/b.rs b/src/b.rs\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -1,2 +1,2 @@\n a\n-b\n+c\n@@ -10,2 +10,3 @@\n x\n+#[ignore]\n y\n"
    );
    let files = scan_patch(&text);
    assert_eq!(files.len(), 2);
    assert_eq!(files[1].path, "src/b.rs");
    assert_eq!(files[1].flags[0].hunk, Some(1));
    assert_eq!(files[1].flags[0].line, Some(11));
    assert_eq!(
        files[1].summary(),
        "Skips, disables or focuses tests:\n  line 11: #[ignore]"
    );
}

#[test]
fn empty_and_odd_input() {
    assert!(scan_patch("").is_empty());
    assert!(scan_patch("not a diff\n@@ nonsense\n+assert True").is_empty());
    let files =
        patch::parse("diff --git a/x b/x\n@@ -0,0 +1 @@\n+line\n\\ No newline at end of file\n");
    assert_eq!(files[0].hunks[0].lines.len(), 1);
    assert_eq!(files[0].hunks[0].lines[0].mark, Mark::Added);
}

#[test]
fn a_huge_patch_is_read_in_one_pass() {
    let body: String = (0..50_000).map(|i| format!("+line {i}\n")).collect();
    let text = diff("src/big.rs", &format!(" a\n{}", body.trim_end()));
    let started = std::time::Instant::now();
    assert!(scan_patch(&text).is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn removed_lines_are_numbered_in_the_old_file_and_listed_first() {
    let text = diff(
        "tests/a.rs",
        "-    assert!(a);\n-    assert!(b);\n+    #[ignore]",
    );
    let summary = scan_patch(&text)[0].summary();
    assert_eq!(
        summary,
        "Removes assertions:\n  old line 1: assert!(a);\n  old line 2: assert!(b);\nSkips, disables or focuses tests:\n  line 1: #[ignore]"
    );
}

#[test]
fn long_lists_are_cut_in_the_hover() {
    let body: String = (0..9).map(|i| format!("-    assert!(x{i});\n")).collect();
    let text = diff("tests/a.rs", body.trim_end());
    let summary = scan_patch(&text)[0].summary();
    assert!(summary.ends_with("and 4 more"), "{summary}");
    assert_eq!(summary.lines().count(), 7);
}

#[test]
fn prose_strings_and_look_alikes_are_left_alone() {
    let cases = [
        // Documentation about the feature itself.
        (
            "README.md",
            "+Adds `--no-verify` and `t.Skip()` and #[ignore]",
        ),
        ("docs/notes.txt", "+git commit --no-verify"),
        // Markers inside strings: a scanner's own patterns, test fixtures.
        ("src/rules.rs", "+    \"it.skip(\","),
        ("src/rules.rs", "+const S: &str = \"#[ignore]\";"),
        ("tests/rules_test.rs", "+    let s = \"@pytest.mark.skip\";"),
        // Attributes that only start the same.
        ("src/Dto.cs", "+    [IgnoreDataMember]"),
        ("app/src/main/User.kt", "+    @IgnoredOnParcel"),
        ("src/a.py", "+model.fit(\"train.csv\")"),
        ("tests/q.js", "+pending = []"),
    ];
    for (path, line) in cases {
        let text = diff(path, &format!(" context\n{line}"));
        assert!(
            reasons(&text).is_empty(),
            "{path}: {line}: {:?}",
            reasons(&text)
        );
    }
}

#[test]
fn non_assertions_removed_are_not_flagged() {
    let cases = [
        (
            "src/lib.rs",
            "-    let x = foo().expect(\"has foo\");\n+    let x = foo()?;",
        ),
        ("src/auth.js", "-  jwt.verify(token, key);"),
        ("src/load.js", "-  const p = require.resolve('x');"),
        // `[TestCategory]` is no test; removing it removes none.
        ("tests/ATests.cs", "-    [TestCategory(\"slow\")]"),
    ];
    for (path, body) in cases {
        let text = diff(path, &format!(" context\n{body}"));
        assert!(
            reasons(&text).is_empty(),
            "{path}: {body}: {:?}",
            reasons(&text)
        );
    }
    // Still an assertion, as a method or a debug one.
    let debug = diff("src/a.rs", " fn f() {\n-    debug_assert!(ok);\n }");
    assert_eq!(reasons(&debug), vec![Reason::RemovedAssertions]);
    let method = diff(
        "tests/test_a.py",
        " def test_a(self):\n-        self.assertEqual(a, b)",
    );
    assert_eq!(reasons(&method), vec![Reason::RemovedAssertions]);
}

#[test]
fn margins_outside_tests_and_tighter_ones_are_not_loosened() {
    let css = diff("web/site.css", "-  margin: 4px;\n+  margin: 8px;");
    assert!(reasons(&css).is_empty());
    let egui = diff(
        "src/ui.rs",
        "-    .inner_margin(Margin::same(4))\n+    .inner_margin(Margin::same(8))",
    );
    assert!(reasons(&egui).is_empty());
    let coverage = diff("ci/cov.toml", "-threshold = 70\n+threshold = 80");
    assert!(reasons(&coverage).is_empty());
    // In a non-test file, on a line that asserts: it counts.
    let assert_line = diff(
        "src/num.rs",
        "-    assert!(approx(a, b, epsilon = 1e-9));\n+    assert!(approx(a, b, epsilon = 1e-3));",
    );
    assert_eq!(reasons(&assert_line), vec![Reason::LoosenedTolerance]);
}

#[test]
fn every_margin_of_a_line_and_lines_in_pairs() {
    let both = diff(
        "tests/test_a.py",
        "-    np.testing.assert_allclose(a, b, rtol=1e-5, atol=1e-8)\n+    np.testing.assert_allclose(a, b, rtol=1e-7, atol=1e-1)",
    );
    let files = scan_patch(&both);
    assert_eq!(files[0].flags.len(), 1);
    assert!(files[0].flags[0].evidence.contains("atol 1e-8 → 1e-1"));
    // Two lines renamed, values the same: compared in pairs, nothing loosened.
    let renamed = diff(
        "tests/test_a.py",
        "-    check(a, rtol=1e-9)\n-    check(b, rtol=1e-2)\n+    check(x, rtol=1e-9)\n+    check(y, rtol=1e-2)",
    );
    assert!(reasons(&renamed).is_empty());
    // Another word's number is not this one's.
    let other = diff(
        "tests/test_a.py",
        "-    close(a, rtol=tol, atol=1e-8)\n+    close(a, rtol=tol, atol=1e-8, x=5)",
    );
    assert!(reasons(&other).is_empty());
    // Nested calls in the arguments.
    let nested = diff(
        "web/a.test.js",
        "-  expect(f(x)).toBeCloseTo(sum(c, d), 5);\n+  expect(f(x)).toBeCloseTo(sum(c, d), 1);",
    );
    assert_eq!(reasons(&nested), vec![Reason::LoosenedTolerance]);
    let python = diff(
        "tests/test_a.py",
        "-        self.assertAlmostEqual(f(x), 1.0, 3)\n+        self.assertAlmostEqual(f(x), 1.0, 1)",
    );
    assert_eq!(reasons(&python), vec![Reason::LoosenedTolerance]);
}

#[test]
fn a_test_moved_to_another_file_is_no_removal() {
    let text = format!(
        "{}{}",
        diff(
            "src/a.rs",
            " mod tests {\n-    #[test]\n-    fn t() { assert_eq!(f(), 1); }\n }"
        ),
        diff(
            "tests/a.rs",
            " use x;\n+    #[test]\n+    fn t() { assert_eq!(f(), 1); }"
        ),
    );
    assert!(reasons(&text).is_empty(), "{:?}", reasons(&text));
}

#[test]
fn snapshot_rewrite_needs_most_of_the_file() {
    // 60 lines rewritten at the top of a longer snapshot: three lines of context follow.
    let removed: String = (0..60).map(|i| format!("-old {i}\n")).collect();
    let added: String = (0..60).map(|i| format!("+new {i}\n")).collect();
    let head = format!("{removed}{added} keep 1\n keep 2\n keep 3");
    let text = format!(
        "diff --git a/s.snap b/s.snap\n--- a/s.snap\n+++ b/s.snap\n@@ -1,63 +1,63 @@\n{head}\n"
    );
    assert!(reasons(&text).is_empty());
    let whole = format!(
        "diff --git a/s.snap b/s.snap\n--- a/s.snap\n+++ b/s.snap\n@@ -1,60 +1,60 @@\n{removed}{}\n",
        added.trim_end()
    );
    let files = scan_patch(&whole);
    assert_eq!(files[0].flags[0].evidence, "replaces 60 of its 60 lines");
}

#[test]
fn ci_jobs_four_spaces_in_and_hooks_on_continuation_lines() {
    let four = diff(
        ".github/workflows/ci.yml",
        " jobs:\n-    test:\n-        runs-on: x\n     lint:",
    );
    assert_eq!(reasons(&four), vec![Reason::CiRemovedJob]);
    // A nested key of the same name added elsewhere doesn't hide the job's removal.
    let nested = diff(
        ".github/workflows/ci.yml",
        " jobs:\n-  test:\n-    runs-on: x\n   lint:\n+    test:\n+      x: 1",
    );
    assert_eq!(reasons(&nested), vec![Reason::CiRemovedJob]);
    let continued = diff(
        ".github/workflows/release.yml",
        "       run: |\n         git commit -m x \\\n+          --no-verify",
    );
    assert_eq!(reasons(&continued), vec![Reason::SkipsHooks]);
    let shell = diff("scripts/ship.sh", " git commit -m x \\\n+  --no-verify");
    // `--` starts no comment in a shell script.
    assert_eq!(reasons(&shell), vec![Reason::SkipsHooks]);
}

#[test]
fn trivial_assertions_with_a_message_and_rust_ignore_attributes() {
    let message = diff(
        "tests/test_a.py",
        " def test_a():\n+    assert True, \"todo\"",
    );
    assert_eq!(reasons(&message), vec![Reason::TrivialAssertion]);
    let cfg = diff(
        "src/a.rs",
        " #[test]\n+#[cfg_attr(not(feature = \"slow\"), ignore)]",
    );
    assert_eq!(reasons(&cfg), vec![Reason::SkipsTests]);
    let pointer = diff("src/a.c", " void f() {\n+    *p = assert_ok(x);\n }");
    // `*p = …` is code, not a block comment: no flag, but read.
    assert!(reasons(&pointer).is_empty());
}
