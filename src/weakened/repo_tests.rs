//! The scans against real repositories, through the git command layer the UI uses.

use std::path::{Path, PathBuf};

use super::{Reason, Scope, scan};

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

/// A new repository in the temp folder, with a config that would change git's output if
/// gitcraft didn't set it back (colours, no prefixes, an external diff).
pub(crate) fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-weakened-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
        &["config", "color.ui", "always"],
        &["config", "diff.noprefix", "true"],
        &["config", "diff.external", "false"],
        &["config", "core.quotePath", "true"],
    ] {
        git(&dir, args)?;
    }
    Ok(dir)
}

pub(crate) fn write(dir: &Path, path: &str, text: &str) -> Result<(), String> {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, text).map_err(|e| e.to_string())
}

pub(crate) fn commit(dir: &Path, message: &str) -> Result<String, String> {
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", message])?;
    Ok(git(dir, &["rev-parse", "HEAD"])?.trim().to_owned())
}

#[test]
fn a_commit_a_range_and_the_working_tree() -> Result<(), String> {
    let dir = repo("scan")?;
    let test = "tests/äpp tests/login_test.rs";
    write(
        &dir,
        test,
        "#[test]\nfn logs_in() {\n    assert!(login());\n}\n",
    )?;
    write(
        &dir,
        "tests/old_test.py",
        "def test_old():\n    assert old()\n",
    )?;
    write(
        &dir,
        ".github/workflows/ci.yml",
        "jobs:\n  lint:\n    runs-on: x\n  test:\n    runs-on: x\n",
    )?;
    let first = commit(&dir, "tests")?;

    write(&dir, test, "#[test]\n#[ignore]\nfn logs_in() {\n}\n")?;
    std::fs::remove_file(dir.join("tests/old_test.py")).map_err(|e| e.to_string())?;
    write(
        &dir,
        ".github/workflows/ci.yml",
        "jobs:\n  test:\n    runs-on: x\n    continue-on-error: true\n",
    )?;
    let second = commit(&dir, "weaker")?;

    let files = scan(&dir, &Scope::Commit(second.clone()))?;
    let by_path = |p: &str| files.iter().find(|f| f.path == p);
    let login = by_path(test).ok_or("the test file with a unicode name")?;
    let reasons: Vec<Reason> = login.flags.iter().map(|f| f.reason).collect();
    assert!(reasons.contains(&Reason::SkipsTests), "{reasons:?}");
    assert!(reasons.contains(&Reason::RemovedAssertions), "{reasons:?}");
    assert!(
        by_path("tests/old_test.py").is_some_and(|f| f.flags[0].reason == Reason::DeletedTestFile)
    );
    let ci = by_path(".github/workflows/ci.yml").ok_or("the workflow")?;
    let ci_reasons: Vec<Reason> = ci.flags.iter().map(|f| f.reason).collect();
    assert!(ci_reasons.contains(&Reason::CiRemovedJob), "{ci_reasons:?}");
    assert!(
        ci_reasons.contains(&Reason::CiAllowsFailure),
        "{ci_reasons:?}"
    );

    let range = scan(
        &dir,
        &Scope::Range {
            from: first.clone(),
            to: Some(second.clone()),
        },
    )?;
    assert_eq!(range.len(), files.len());
    // The first commit, against nothing: only additions, nothing weakened.
    assert!(scan(&dir, &Scope::Commit(first))?.is_empty());

    write(
        &dir,
        test,
        "#[test]\n#[ignore]\nfn logs_in() {\n    assert!(true);\n}\n",
    )?;
    let work = scan(&dir, &Scope::Work(1))?;
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].flags[0].reason, Reason::TrivialAssertion);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn errors_say_what_could_not_be_read() -> Result<(), String> {
    let dir = repo("errors")?;
    let err = scan(&dir, &Scope::Commit("0123456789abcdef".into()));
    let Err(message) = err else {
        return Err("a commit that isn't there is an error".into());
    };
    assert!(
        message.starts_with("Could not read the changes"),
        "{message}"
    );
    // A repository with no commit yet has no HEAD to compare the working tree with.
    assert!(scan(&dir, &Scope::Work(0)).is_err());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
