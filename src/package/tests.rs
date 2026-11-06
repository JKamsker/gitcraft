//! Packages of real repositories: a Cargo workspace and an npm project, listed by their own tools,
//! with what is left out, what looks shipped by accident, and a published version to compare.

use std::path::{Path, PathBuf};

use super::*;

fn folder(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gitcraft package-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn write(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(file, text);
}

fn git(dir: &Path, args: &[&str]) {
    let out = crate::git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
}

fn repo(name: &str) -> PathBuf {
    let dir = folder(name);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "t"]);
    git(&dir, &["config", "user.email", "t@t"]);
    git(&dir, &["config", "commit.gpgSign", "false"]);
    dir
}

/// A workspace: `app` at the top, `tool` in `crates/tool`, `internal` never published.
fn cargo_project() -> PathBuf {
    let dir = repo("cargo");
    write(
        &dir,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/tool\", \"internal\"]\n\n[package]\nname = \"gitcraft-test-app\"\nversion = \"1.2.3\"\nedition = \"2021\"\nlicense = \"MIT\"\nreadme = \"README.md\"\nexclude = [\"src/extra.rs\"]\n",
    );
    write(&dir, "README.md", "# app\n");
    write(&dir, "LICENSE", "MIT\n");
    write(&dir, "src/lib.rs", "pub fn a() {}\n");
    write(&dir, "src/extra.rs", "pub fn b() {}\n");
    write(&dir, ".env", "TOKEN=x\n");
    write(
        &dir,
        "crates/tool/Cargo.toml",
        "[package]\nname = \"gitcraft-test-tool\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\n",
    );
    write(&dir, "crates/tool/src/lib.rs", "\n");
    write(
        &dir,
        "internal/Cargo.toml",
        "[package]\nname = \"internal\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n",
    );
    write(&dir, "internal/src/lib.rs", "\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "start"]);
    // Left in the folder, not tracked.
    write(&dir, "notes ü.txt", "mine\n");
    dir
}

#[test]
fn a_cargo_workspace_is_listed_with_what_is_left_out_and_shipped_by_accident() {
    let dir = cargo_project();
    let packages = packages(&dir);
    let app = packages
        .as_ref()
        .ok()
        .and_then(|p| p.iter().find(|p| p.name == "gitcraft-test-app").cloned());
    let report = app.as_ref().map(|p| report(&dir, p, false));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(packages.is_ok(), "{packages:?}");
    let names: Vec<(String, String)> = packages
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.name, p.dir))
        .collect();
    assert!(names.contains(&("gitcraft-test-app".into(), String::new())));
    assert!(names.contains(&("gitcraft-test-tool".into(), "crates/tool".into())));
    assert!(!names.iter().any(|(n, _)| n == "internal"));
    assert!(matches!(report, Some(Ok(_))), "{report:?}");
    let Some(Ok(report)) = report else { return };
    let shipped: Vec<&str> = report.files.iter().map(|(f, _)| f.as_str()).collect();
    assert!(shipped.contains(&"src/lib.rs"), "{shipped:?}");
    assert!(!shipped.contains(&"src/extra.rs"));
    let missing: Vec<&str> = report.missing.iter().map(|n| n.path.as_str()).collect();
    assert!(missing.contains(&"src/extra.rs"), "{missing:?}");
    let accidental: Vec<&str> = report.accidental.iter().map(|n| n.path.as_str()).collect();
    assert!(accidental.contains(&".env"), "{accidental:?}");
    assert!(accidental.contains(&"notes ü.txt"), "{accidental:?}");
    assert_eq!(report.published, Ok(None));
}

#[test]
fn an_npm_project_is_listed_when_npm_is_installed() {
    let dir = repo("npm");
    write(
        &dir,
        "package.json",
        "{\n  \"name\": \"gitcraft-test-pkg\",\n  \"version\": \"1.0.0\",\n  \"main\": \"dist/index.js\",\n  \"types\": \"dist/index.d.ts\",\n  \"files\": [\"dist\"]\n}\n",
    );
    write(&dir, "dist/index.js", "module.exports = 1;\n");
    write(&dir, "dist/debug.log", "oops\n");
    write(&dir, "README.md", "# x\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "start"]);
    if super::run::run(&dir, "npm", &["--version"]).is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("npm isn't installed: skipped");
        return;
    }
    let pkg = packages(&dir).ok().and_then(|p| p.into_iter().next());
    let report = pkg.as_ref().map(|p| report(&dir, p, false));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(report, Some(Ok(_))), "{report:?}");
    let Some(Ok(report)) = report else { return };
    let shipped: Vec<&str> = report.files.iter().map(|(f, _)| f.as_str()).collect();
    assert!(
        shipped.contains(&"dist/index.js") && shipped.contains(&"package.json"),
        "{shipped:?}"
    );
    let missing: Vec<&str> = report.missing.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(missing, ["dist/index.d.ts"]);
    let accidental: Vec<&str> = report.accidental.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(accidental, ["dist/debug.log"]);
}

#[test]
fn a_published_archive_is_compared_file_by_file() {
    let bytes = archive::make(
        "gitcraft-test-app-1.2.2",
        &[
            ("Cargo.toml", b"x"),
            ("Cargo.toml.orig", b"x"),
            ("src/lib.rs", b"x"),
            ("src/old.rs", b"x"),
        ],
    );
    let before = archive::list(&bytes).unwrap_or_default();
    let now = vec![
        ("Cargo.toml".to_owned(), None),
        ("src/lib.rs".to_owned(), None),
        ("src/new.rs".to_owned(), None),
    ];
    let (added, gone) = check::compare(Ecosystem::Cargo, &now, &before);
    assert_eq!(added, ["src/new.rs"]);
    assert_eq!(gone, ["src/old.rs"]);
}

#[test]
fn a_repository_without_manifests_says_so() {
    let dir = repo("none");
    write(&dir, "a.txt", "a\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "a"]);
    let found = packages(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(found.is_err_and(|e| e.contains("No Cargo.toml or package.json")));
}
