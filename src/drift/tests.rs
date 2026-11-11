use std::path::{Path, PathBuf};

use super::deps::{added, is_manifest};
use super::signatures::{changed, is_public};
use super::task::{issue_number, keywords, related};
use super::{Report, measure};

#[test]
fn dependencies_added_in_each_manifest() {
    let cargo_old = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n";
    let cargo_new = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\nrand = \"0.8\"\n\n[dev-dependencies.proptest]\nversion = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n";
    assert_eq!(
        added("Cargo.toml", cargo_old, cargo_new),
        vec!["libc", "proptest", "rand"]
    );
    let pkg_old = r#"{"dependencies": {"react": "18"}}"#;
    let pkg_new =
        r#"{"dependencies": {"react": "18", "left-pad": "1"}, "devDependencies": {"jest": "29"}}"#;
    assert_eq!(
        added("web/package.json", pkg_old, pkg_new),
        vec!["jest", "left-pad"]
    );
    assert_eq!(
        added(
            "requirements-dev.txt",
            "requests==2\n",
            "requests==2\n# c\n-r base.txt\nNumPy>=1.2; python_version>'3'\n"
        ),
        vec!["numpy"]
    );
    let py = "[project]\ndependencies = [\n  \"httpx>=0.2\",\n  'rich',\n]\n";
    assert_eq!(added("pyproject.toml", "", py), vec!["httpx", "rich"]);
    let go = "module x\n\nrequire (\n\tgithub.com/a/b v1.0.0\n\tgolang.org/x/y v0.1.0 // indirect\n)\nrequire github.com/c/d v2\n";
    assert_eq!(
        added("go.mod", "module x\n", go),
        vec!["github.com/a/b", "github.com/c/d", "golang.org/x/y"]
    );
    assert_eq!(
        added(
            "Gemfile",
            "gem 'rails'\n",
            "gem 'rails'\ngem \"pry\", group: :dev\n"
        ),
        vec!["pry"]
    );
    assert!(added("package.json", "not json", "{also not").is_empty());
    assert!(
        is_manifest("a/b/Cargo.toml")
            && is_manifest("requirements.txt")
            && !is_manifest("Cargo.lock")
    );
}

#[test]
fn public_signatures_by_language() {
    for (path, line) in [
        ("src/a.rs", "pub fn login(user: &str) -> bool {"),
        ("src/a.rs", "    pub async fn go() {"),
        ("src/a.rs", "pub struct Session {"),
        ("web/a.ts", "export function login(u: string) {"),
        ("web/a.js", "export default class X {}"),
        ("lib/a.py", "def login(user):"),
        ("lib/a.py", "class Session:"),
        ("pkg/a.go", "func Login(u string) bool {"),
        ("pkg/a.go", "func (s *S) Close() error {"),
        ("A.java", "    public void login(String u) {"),
    ] {
        assert!(is_public(path, line), "{path}: {line}");
    }
    for (path, line) in [
        ("src/a.rs", "fn login() {"),
        ("src/a.rs", "pub(crate) fn login() {"),
        ("web/a.ts", "function local() {}"),
        ("lib/a.py", "    def method(self):"),
        ("lib/a.py", "def _private():"),
        ("pkg/a.go", "func login() {"),
        ("README.md", "pub fn x"),
    ] {
        assert!(!is_public(path, line), "{path}: {line}");
    }
}

#[test]
fn signatures_changed_but_not_moved() {
    let patch = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -10 +10 @@\n-pub fn login(user: &str) -> bool {\n+pub fn login(user: &str, pass: &str) -> bool {\n@@ -20,2 +20,0 @@\n-pub fn moved() {}\n-pub fn gone() {}\n@@ -40,0 +39 @@\n+pub fn moved() {}\n";
    let sigs = changed(patch);
    let names: Vec<(&str, u32)> = sigs.iter().map(|s| (s.text.as_str(), s.line)).collect();
    assert_eq!(
        names,
        vec![
            ("pub fn login(user: &str) -> bool {", 10),
            ("pub fn gone() {}", 21)
        ]
    );
    // A removed SQL comment line (`-- x` shown as `--- x`) is no header.
    let sql = "diff --git a/q.sql b/q.sql\n--- a/q.sql\n+++ b/q.sql\n@@ -1 +0,0 @@\n--- note\n";
    assert!(changed(sql).is_empty());
}

#[test]
fn task_words_and_folders() {
    let words = keywords("Fix the login form: logins fail when the password has spaces (#42)");
    assert!(words.contains(&"login".to_owned()) && words.contains(&"password".to_owned()));
    assert!(!words.contains(&"the".to_owned()) && !words.contains(&"fix".to_owned()));
    assert!(related("src/auth/login", &words));
    assert!(related("web/user-login", &words));
    assert!(related("src/passwords", &words));
    assert!(!related("docs/billing", &words));
    assert!(!related("", &words));
    assert!(keywords("").is_empty() && keywords("fix it #12").is_empty());
}

#[test]
fn issue_numbers_from_branches_and_tasks() {
    assert_eq!(issue_number("fix/123-login", ""), Some(123));
    assert_eq!(issue_number("agent/issue-77", ""), Some(77));
    assert_eq!(issue_number("gh-9-x", ""), Some(9));
    assert_eq!(issue_number("feat/issues/31", ""), Some(31));
    assert_eq!(issue_number("42", ""), Some(42));
    assert_eq!(
        issue_number("agent/login", "This fixes #508 for good"),
        Some(508)
    );
    // Dates, stamps, words that only contain a marker, colours.
    assert_eq!(issue_number("agent/2024-11-06-cleanup", ""), None);
    assert_eq!(issue_number("wt/20261106-1530", ""), None);
    assert_eq!(issue_number("fix/high-5-dpi", ""), None);
    assert_eq!(
        issue_number("agent/button", "make the button #333 gray"),
        None
    );
    assert_eq!(issue_number("agent/v2-login", ""), None);
    assert_eq!(issue_number("", ""), None);
}

#[test]
fn the_headline_reads_like_the_issue_says() {
    let report = Report {
        files: (0..14).map(|i| format!("f{i}")).collect(),
        folders: vec![
            super::Folder {
                path: "a".into(),
                files: vec!["a/1".into(), "a/2".into()],
                related: false,
            },
            super::Folder {
                path: "b".into(),
                files: vec!["b/1".into()],
                related: true,
            },
        ],
        changed_lines: 10,
        formatting_lines: 4,
        judged: true,
        ..Report::default()
    };
    assert_eq!(
        report.headline(),
        "14 files changed, 2 files in 1 folder the task doesn't mention, added 0 dependencies, changed 0 public signatures, 40% formatting only"
    );
    assert_eq!(Report::default().formatting_percent(), 0);
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

pub(crate) fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-drift-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    std::fs::create_dir_all(dir.join("src/auth")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("docs")).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(&dir, args)?;
    }
    let files = [
        (
            "src/auth/login.rs",
            "pub fn login(user: &str) -> bool {\n    true\n}\n",
        ),
        ("docs/guide ü.md", "# Guide\nSome text\n"),
        (
            "Cargo.toml",
            "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n",
        ),
        ("src/fmt.rs", "fn a() {\n    b();\n}\n"),
    ];
    for (path, text) in files {
        std::fs::write(dir.join(path), text).map_err(|e| e.to_string())?;
    }
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "base"])?;
    Ok(dir)
}

/// A worktree of `dir` on a new branch, its path.
pub(crate) fn agent_worktree(dir: &Path, branch: &str) -> Result<PathBuf, String> {
    let wt = dir.with_extension("wt").join(branch.replace('/', "-"));
    git(
        dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            branch,
            &wt.to_string_lossy(),
            "main",
        ],
    )?;
    std::fs::write(
        wt.join("src/auth/login.rs"),
        "pub fn login(user: &str, pass: &str) -> bool {\n    !pass.is_empty()\n}\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        wt.join("Cargo.toml"),
        "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\nleft-pad = \"1\"\n",
    )
    .map_err(|e| e.to_string())?;
    git(&wt, &["commit", "-qam", "login"])?;
    // Uncommitted: a doc rewritten, a file reformatted; untracked: a note.
    std::fs::write(wt.join("docs/guide ü.md"), "# Guide\nOther text\n")
        .map_err(|e| e.to_string())?;
    std::fs::write(wt.join("src/fmt.rs"), "fn a() {\n\tb();\n}\n").map_err(|e| e.to_string())?;
    std::fs::write(wt.join("notes.txt"), "todo\n").map_err(|e| e.to_string())?;
    Ok(wt)
}

#[test]
fn a_real_worktree_measured_against_its_task() -> Result<(), String> {
    let dir = repo("measure")?;
    let base = git(&dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    let wt = agent_worktree(&dir, "agent/12-login")?;
    let report = measure(&wt, &base, "Fix the login: it must check the password")?;
    assert_eq!(report.fork, base);
    assert_eq!(
        report.files,
        vec![
            "Cargo.toml",
            "docs/guide ü.md",
            "notes.txt",
            "src/auth/login.rs",
            "src/fmt.rs"
        ]
    );
    let unrelated: Vec<&str> = report.unrelated().map(|f| f.path.as_str()).collect();
    assert_eq!(unrelated, vec!["", "docs", "src"], "{:?}", report.folders);
    assert_eq!(report.dependencies.len(), 1);
    assert_eq!(report.dependencies[0].name, "left-pad");
    assert_eq!(report.signatures.len(), 1);
    assert_eq!(report.signatures[0].path, "src/auth/login.rs");
    assert_eq!(report.formatting_files, vec!["src/fmt.rs"]);
    assert!(report.formatting_lines >= 2 && report.formatting_percent() > 0);
    // No task: nothing is told apart.
    let blind = measure(&wt, &base, "")?;
    assert!(!blind.judged && blind.unrelated().next().is_none());
    assert!(
        blind
            .headline()
            .starts_with("5 files changed, added 1 dependency")
    );
    // The task kept with the worktree, never in its files.
    super::task::write(&wt, "Fix the login")?;
    assert_eq!(super::task::read(&wt)?, "Fix the login");
    assert!(
        git(&wt, &["status", "--porcelain"])?
            .lines()
            .all(|l| !l.contains("gitcraft-task"))
    );
    // No main commit known (a bare layout): compared with main.
    assert_eq!(measure(&wt, "", "x")?.fork, base);
    assert!(measure(&wt, "--x", "x").is_err());
    // What only the worktree's own working tree shows.
    assert_eq!(
        report.uncommitted,
        vec!["docs/guide ü.md", "notes.txt", "src/fmt.rs"]
    );
    assert_eq!(report.untracked, vec!["notes.txt"]);
    // With prefixes configured away, signatures are still found.
    git(&wt, &["config", "diff.mnemonicPrefix", "true"])?;
    git(&wt, &["config", "diff.noprefix", "true"])?;
    assert_eq!(measure(&wt, &base, "")?.signatures.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    Ok(())
}

#[test]
fn the_question_for_a_coding_agent_has_paths_not_contents() {
    let report = Report {
        files: vec!["src/a.rs".into()],
        ..Report::default()
    };
    let q = super::llm::prompt("Fix login", &report);
    assert!(q.contains("Fix login") && q.contains("src/a.rs") && q.contains("NONE"));
}

#[test]
fn review_cases_for_signatures() {
    assert!(is_public("src/a.rs", "pub const MAX: u32 = 1;"));
    assert!(is_public("src/a.rs", "pub const fn f() {}"));
    assert!(is_public("src/a.rs", "pub const unsafe fn f() {}"));
    assert!(!is_public("web/a.ts", "export const TIMEOUT = 5000;"));
    assert!(is_public("web/a.ts", "export const go = (x: number) => x;"));
    assert!(is_public("web/a.ts", "export interface User {"));
    assert!(!is_public("A.java", "public static final int X = foo();"));
    let quoted = "diff --git \"a/src/t\\tx.rs\" \"b/src/t\\tx.rs\"\n--- \"a/src/t\\tx.rs\"\n+++ \"b/src/t\\tx.rs\"\n@@ -1 +1 @@\n-pub fn f() {}\n+pub fn f(a: u8) {}\n";
    assert_eq!(changed(quoted).len(), 1);
}

#[test]
fn review_cases_for_folders() {
    let auth = keywords("Fix authentication for the parser");
    assert!(related("src/auth", &auth));
    assert!(related("src/parse", &auth), "{auth:?}");
    assert!(related("src/parsing", &auth), "{auth:?}");
    let deps = keywords("Update the dependencies");
    assert!(related("src/deps", &deps));
    let speed = keywords("Speed up the parser");
    assert!(speed.contains(&"speed".to_owned()), "{speed:?}");
    // An issue's links and template words don't make every folder related.
    let issue = keywords(
        "Login fails\n\nSteps to reproduce: see https://github.com/x/y/issues/1\nExpected behavior: it works",
    );
    assert!(!related(".github/workflows", &issue), "{issue:?}");
    assert!(related("src/login", &issue));
}

#[test]
fn review_cases_for_dependencies() {
    let before = "[dependencies]\nserde = \"1\"\n";
    let workspace = "[dependencies] # ours\nserde.workspace = true\ntokio = { version = \"1\",\n  features = [\"full\"] }\n";
    assert_eq!(added("Cargo.toml", before, workspace), vec!["tokio"]);
    let extras = "[project]\nname = \"x\"\n\n[project.optional-dependencies]\ndev = [\"pytest>=7\", \"ruff\"]\nlint = [\n  \"mypy\", # don't use 'flake8'\n]\n\n[dependency-groups]\ntest = [\"coverage\"]\n";
    assert_eq!(
        added("pyproject.toml", "", extras),
        vec!["coverage", "mypy", "pytest", "ruff"]
    );
    let reqs = "git+https://github.com/a/b.git#egg=bee\nhttps://example.com/x.whl\nflask # web\n";
    assert_eq!(
        added("requirements/dev.txt", "", reqs),
        vec!["bee", "flask"]
    );
    assert!(is_manifest("requirements/dev.txt"));
}
