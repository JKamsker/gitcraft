//! Releases against real repositories: the draft read from the history, then every local step
//! and the push to a bare remote, undo and redo.

use std::path::{Path, PathBuf};

use super::*;

fn folder(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gitcraft release-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?} failed: {out:?}");
    out.unwrap_or_default()
}

fn commit(dir: &Path, file: &str, text: &str, message: &str) {
    let _ = std::fs::write(dir.join(file), text);
    run(dir, &["add", "--", file]);
    run(dir, &["commit", "-q", "-m", message]);
}

/// A Rust project released as v1.2.3, then a feature and a fix; pushed to a bare `origin`.
fn project(name: &str) -> (PathBuf, PathBuf) {
    let dir = folder(name);
    let bare = folder(&format!("{name}-origin.git"));
    run(&bare, &["init", "-q", "--bare"]);
    run(&dir, &["init", "-q", "-b", "main"]);
    run(&dir, &["config", "user.name", "Zoë"]);
    run(&dir, &["config", "user.email", "zoe@example.org"]);
    run(&dir, &["config", "tag.gpgSign", "false"]);
    run(&dir, &["config", "commit.gpgSign", "false"]);
    let cargo = "[package]\nname = \"app\"\nversion = \"1.2.3\"\nedition = \"2024\"\n";
    let lock = "version = 4\n\n[[package]]\nname = \"app\"\nversion = \"1.2.3\"\n";
    let _ = std::fs::write(dir.join("Cargo.lock"), lock);
    let _ = std::fs::write(
        dir.join(CHANGELOG),
        "# Changelog\n\nAll changes.\n\n## v1.2.3 - 2026-01-01\n\n- first\n",
    );
    run(&dir, &["add", "Cargo.lock", CHANGELOG]);
    commit(&dir, "Cargo.toml", cargo, "chore: start");
    run(&dir, &["tag", "-a", "v1.2.3", "-m", "v1.2.3"]);
    commit(&dir, "ü.txt", "1\n", "feat(ui): dark mode (#12)");
    commit(&dir, "b.txt", "1\n", "fix: crash");
    run(&dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
    run(&dir, &["push", "-q", "-u", "origin", "main", "v1.2.3"]);
    (dir, bare)
}

#[test]
fn the_draft_suggests_the_next_version_from_the_commits_since_the_last_tag() {
    let (dir, bare) = project("draft");
    let head = git::head(&dir);
    let draft = draft(&dir, "HEAD", Some("origin".into()));
    // An older commit: no version commit there.
    let older = draft_of_parent(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(draft.is_ok(), "{draft:?}");
    let Ok(draft) = draft else { return };
    assert_eq!(draft.commit, head);
    assert_eq!(draft.last_tag.as_deref(), Some("v1.2.3"));
    assert_eq!(draft.suggested.to_string(), "v1.3.0");
    assert_eq!(draft.changes.len(), 2);
    assert_eq!(draft.branch.as_deref(), Some("main"));
    assert_eq!(draft.manifests, [(Kind::Cargo, "1.2.3".to_owned())]);
    assert!(draft.changelog);
    assert!(draft.tags.contains(&"v1.2.3".to_owned()));
    assert!(older.is_some_and(|d| d.branch.is_none() && d.manifests.is_empty()));
}

fn draft_of_parent(dir: &Path) -> Option<Draft> {
    draft(dir, "HEAD~1", None).ok()
}

#[test]
fn a_repository_without_tags_starts_from_its_manifest() {
    let dir = folder("first");
    run(&dir, &["init", "-q", "-b", "main"]);
    run(&dir, &["config", "user.name", "Zoë"]);
    run(&dir, &["config", "user.email", "zoe@example.org"]);
    commit(
        &dir,
        "package.json",
        "{\n  \"name\": \"x\",\n  \"version\": \"0.4.0\"\n}\n",
        "feat: first",
    );
    let draft = draft(&dir, "HEAD", None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(draft.is_ok_and(|d| d.suggested.to_string() == "v0.4.0" && d.last_tag.is_none()));
}

fn plan_of(draft: &Draft, notes: &str) -> Plan {
    Plan {
        commit: draft.commit.clone(),
        tag: draft.suggested.to_string(),
        notes: notes.to_owned(),
        bump: Some(Bump {
            branch: "main".into(),
            remote_ref: "refs/heads/main".into(),
            version: draft.suggested.number(),
            manifests: draft.manifests.clone(),
            changelog: true,
        }),
        sign: false,
        push: Some("origin".into()),
        publish: false,
        prerelease: false,
        assets: Vec::new(),
    }
}

#[test]
fn every_local_step_and_the_push() {
    let (dir, bare) = project("cut");
    let draft = draft(&dir, "HEAD", Some("origin".into()));
    assert!(draft.is_ok());
    let Ok(draft) = draft else { return };
    let notes = notes::notes(&draft.changes, &|n| Some(format!("https://x/pull/{n}")));
    let plan = plan_of(&draft, &notes);
    let mut done = Done::default();
    let results: Vec<Result<(), String>> = plan
        .steps()
        .into_iter()
        .map(|step| run_step(&dir, &plan, step, &mut done, None, None))
        .collect();
    let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
    let lock = std::fs::read_to_string(dir.join("Cargo.lock")).unwrap_or_default();
    let changelog = std::fs::read_to_string(dir.join(CHANGELOG)).unwrap_or_default();
    let message = run(&dir, &["tag", "-l", "--format=%(contents)", "v1.3.0"]);
    let tagged = run(&dir, &["rev-parse", "v1.3.0^{commit}"]);
    let subject = run(&dir, &["log", "-1", "--format=%s"]);
    let remote_tag = git::resolve(&bare, "refs/tags/v1.3.0").unwrap_or_default();
    let remote_main = git::resolve(&bare, "refs/heads/main").unwrap_or_default();
    let status = run(&dir, &["status", "--porcelain"]);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert!(cargo.contains("version = \"1.3.0\""), "{cargo}");
    assert!(lock.contains("version = \"1.3.0\""), "{lock}");
    assert!(
        changelog.starts_with("# Changelog\n\nAll changes.\n\n## v1.3.0 - "),
        "{changelog}"
    );
    assert!(changelog.contains("dark mode ([#12](https://x/pull/12))"));
    assert!(changelog.contains("## v1.2.3 - 2026-01-01"));
    // Markdown headings stay in the tag's message.
    assert!(message.contains("### Features"), "{message}");
    assert_eq!(subject.trim(), "chore(release): v1.3.0");
    assert_eq!(tagged.trim(), done.bump_commit.clone().unwrap_or_default());
    assert_eq!(remote_tag, done.tag_commit.clone().unwrap_or_default());
    assert_eq!(remote_main, tagged.trim());
    assert!(status.trim().is_empty(), "{status}");
}

#[test]
fn a_failed_push_can_be_resumed_from_where_it_stopped() {
    let (dir, bare) = project("resume");
    let draft = draft(&dir, "HEAD", Some("origin".into()));
    let Ok(draft) = draft else { return };
    let mut plan = plan_of(&draft, "notes");
    plan.bump = None;
    // The remote is gone: the push fails, the tag stays.
    let _ = std::fs::rename(&bare, bare.with_extension("moved"));
    let mut done = Done::default();
    let tag = run_step(&dir, &plan, Step::Tag, &mut done, None, None);
    let push = run_step(&dir, &plan, Step::Push, &mut done, None, None);
    // Back: resumed at the push.
    let _ = std::fs::rename(bare.with_extension("moved"), &bare);
    let resumed = run_step(&dir, &plan, Step::Push, &mut done, None, None);
    let remote = git::resolve(&bare, "refs/tags/v1.3.0").ok();
    // A tag that exists can't be made again.
    let again = run_step(&dir, &plan, Step::Tag, &mut Done::default(), None, None);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(
        tag.is_ok() && push.is_err() && resumed.is_ok(),
        "{tag:?} {push:?} {resumed:?}"
    );
    assert_eq!(remote, done.tag_commit);
    assert!(again.is_err());
}

#[test]
fn the_changelog_entry_goes_under_the_heading() {
    assert_eq!(
        with_entry("# Changelog\n\n## v1 - x\n", "v2", "- a", "2026-11-06"),
        "# Changelog\n\n## v2 - 2026-11-06\n\n- a\n\n## v1 - x\n"
    );
    assert_eq!(
        with_entry("## v1 - x\n", "v2", "- a", "d"),
        "## v2 - d\n\n- a\n\n## v1 - x\n"
    );
    assert_eq!(with_entry("", "v2", "", "d"), "## v2 - d\n\n\n\n");
}

#[test]
fn dates_and_tag_names() {
    assert_eq!(civil(0), (1970, 1, 1));
    assert_eq!(civil(20_398), (2025, 11, 6));
    assert_eq!(civil(11_016), (2000, 2, 29));
    let taken = ["v1.0.0".to_owned()];
    assert!(tag_problem("v1.0.0", &taken).is_some());
    assert!(tag_problem("v1.1.0", &taken).is_none());
    assert!(tag_problem("v1 .0", &taken).is_some());
    assert!(tag_problem("-x", &taken).is_some());
    assert!(tag_problem("  ", &taken).is_some());
}

#[test]
fn the_preview_says_every_step() {
    let plan = Plan {
        commit: "a".repeat(40),
        tag: "v2.0.0".into(),
        notes: String::new(),
        bump: None,
        sign: true,
        push: Some("origin".into()),
        publish: true,
        prerelease: true,
        assets: vec![PathBuf::from("dist/app.tar.gz")],
    };
    let preview = plan.preview(Some(Forge::GitHub));
    let steps: Vec<Step> = preview.iter().map(|(s, _)| *s).collect();
    assert_eq!(steps, [Step::Tag, Step::Push, Step::Publish, Step::Upload]);
    assert!(preview[0].1.contains("signed tag v2.0.0"));
    assert!(preview[2].1.contains("pre-release v2.0.0 on GitHub"));
    assert!(preview[3].1.contains("app.tar.gz"));
}

/// Changes made to a manifest after the release was planned are never written over, and the
/// version commit goes where it was planned only.
#[test]
fn the_version_commit_refuses_changed_files_and_another_branch() {
    let (dir, bare) = project("guards");
    let draft = draft(&dir, "HEAD", Some("origin".into()));
    let Ok(draft) = draft else { return };
    let plan = plan_of(&draft, "notes");
    let _ = std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"1.2.3\"\n# mine\n",
    );
    let changed = run_step(&dir, &plan, Step::Bump, &mut Done::default(), None, None);
    let kept = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
    run(&dir, &["checkout", "-q", "--", "Cargo.toml"]);
    run(&dir, &["checkout", "-q", "-b", "other"]);
    let elsewhere = run_step(&dir, &plan, Step::Bump, &mut Done::default(), None, None);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(changed.is_err_and(|e| e.contains("changed since")));
    assert!(kept.contains("# mine"));
    assert!(elsewhere.is_err_and(|e| e.contains("isn't checked out")));
}

/// The push sends the version commit, not commits made on the branch after it.
#[test]
fn the_push_sends_the_version_commit_only() {
    let (dir, bare) = project("push-commit");
    let draft = draft(&dir, "HEAD", Some("origin".into()));
    let Ok(draft) = draft else { return };
    let plan = plan_of(&draft, "notes");
    let mut done = Done::default();
    let bumped = run_step(&dir, &plan, Step::Bump, &mut done, None, None);
    let tagged = run_step(&dir, &plan, Step::Tag, &mut done, None, None);
    commit(&dir, "later.txt", "later\n", "feat: later");
    let pushed = run_step(&dir, &plan, Step::Push, &mut done, None, None);
    let remote = git::resolve(&bare, "refs/heads/main").ok();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(
        bumped.is_ok() && tagged.is_ok() && pushed.is_ok(),
        "{pushed:?}"
    );
    assert_eq!(remote, done.bump_commit);
}

#[test]
fn a_tag_that_isnt_a_version_isnt_taken_for_the_last_release() {
    let (dir, bare) = project("not-a-version");
    run(&dir, &["tag", "nightly"]);
    let draft = draft(&dir, "HEAD", None);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(draft.is_ok_and(
        |d| d.last_tag.as_deref() == Some("v1.2.3") && d.suggested.to_string() == "v1.3.0"
    ));
}
