use super::*;
use std::path::{Path, PathBuf};

const ID: &str = "I0123456789abcdef0123456789abcdef01234567";
const OTHER: &str = "I1123456789abcdef0123456789abcdef01234567";

#[test]
fn identity_is_a_single_nonzero_final_trailer() {
    assert_eq!(
        change_id(&format!(
            "Subject\n\nBody\n\nChange-Id: {ID}\nSigned-off-by: A <a@b>\n"
        )),
        Ok(ID)
    );
    assert!(change_id(&format!("Subject\n\nChange-Id: {ID}\n\nMore prose")).is_err());
    assert!(change_id(&format!("Change-Id: {ID}")).is_err());
    assert_eq!(
        change_id(&format!("Subject\r\n\r\nChange-Id: {ID}\r\n")),
        Ok(ID)
    );
    assert!(change_id(&format!("Subject\n\nChange-Id: {ID}\nChange-Id: {ID}")).is_err());
    assert!(change_id(&format!("Subject\n\nChange-Id: {ID}\nMore prose")).is_err());
    for invalid in [
        "I0000000000000000000000000000000000000000",
        "Ibad",
        "i0123456789abcdef0123456789abcdef01234567",
    ] {
        assert!(change_id(&format!("Subject\n\nChange-Id: {invalid}")).is_err());
    }
}

#[test]
fn amendments_keep_the_review_identity_when_replacing_the_description() {
    let original = format!("Original\n\nChange-Id: {ID}");
    assert_eq!(
        amended_message(&original, "Reworded"),
        Ok(format!("Reworded\n\nChange-Id: {ID}"))
    );
    assert_eq!(amended_message(&original, &original), Ok(original.clone()));
    assert!(amended_message(&original, &format!("New\n\nChange-Id: {OTHER}")).is_err());
    assert!(amended_message(&original, "New\n\nChange-Id: invalid").is_err());
}

#[test]
fn gerrit_projects_and_explicit_web_overrides_are_detected_without_a_network_probe() {
    let remote = |url: &str| {
        vec![Remote {
            name: "origin".into(),
            url: url.into(),
        }]
    };
    let config = HashMap::from([("gitcraft.forge".into(), "gerrit".into())]);
    let ssh = detect_with(
        &remote("ssh://me@review.example:29418/project.git"),
        &config,
    );
    assert_eq!(
        ssh,
        Some(Server {
            remote: "origin".into(),
            web: "https://review.example".into(),
            project: "project".into(),
            web_warning: None
        })
    );
    let google = detect_with(
        &remote("https://gerrit.googlesource.com/a/plugins/test"),
        &HashMap::new(),
    );
    assert_eq!(
        google.map(|s| (s.project, s.web)),
        Some((
            "plugins/test".into(),
            "https://gerrit-review.googlesource.com".into()
        ))
    );
    let off = HashMap::from([("gitcraft.forge".into(), "gitea".into())]);
    assert!(detect_with(&remote("ssh://gerrit.example/project"), &off).is_none());
    let mut config = config;
    config.insert(
        "gitcraft.gerriturl".into(),
        "https://review.example:8443/gerrit/".into(),
    );
    let server = detect_with(&remote("ssh://me@review.example:29418/project"), &config);
    assert_eq!(
        server.map(|s| s.change_url(ID)),
        Some(format!(
            "https://review.example:8443/gerrit/q/project%3Aproject%20change%3A{ID}"
        ))
    );
    let proxy = detect_with(
        &remote("https://review.example:8443/gerrit/a/project"),
        &config,
    );
    assert_eq!(proxy.map(|server| server.project), Some("project".into()));
    config.insert("gitcraft.gerriturl".into(), "javascript:alert(1)".into());
    let invalid = detect_with(&remote("ssh://review.example/project"), &config)
        .expect("invalid web setting must keep review routing");
    assert_eq!(invalid.web, "https://review.example");
    assert!(invalid.web_warning.is_some());
    assert!(detect_with(&remote("/tmp/project"), &config).is_none());
}

#[test]
fn push_refspec_and_topic_cannot_introduce_git_options_or_gerrit_magic_options() {
    let sha = "a".repeat(40);
    for bad in [
        "--force",
        "main%submit",
        "main%topic=x",
        "a:b",
        "a.lock/x",
        "a/.b",
        "a b",
        "a..b",
    ] {
        assert!(review_ops("origin", bad, "", &sha, &sha).is_err(), "{bad}");
    }
    assert!(review_ops("--mirror", "main", "", &sha, &sha).is_err());
    assert!(review_ops("origin", "main", "x\ny", &sha, &sha).is_err());
    let ops = review_ops("origin", "stable/1", "topic,submit%ready=yes", &sha, &sha);
    assert!(
        matches!(ops, Ok(ops) if ops[1] == vec!["push".to_owned(), "--push-option=topic=topic,submit%ready=yes".to_owned(), "--".to_owned(), "origin".to_owned(), format!("{sha}:refs/for/stable/1")])
    );
}

fn repo(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("gitcraft-gerrit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    assert!(std::fs::create_dir_all(&path).is_ok());
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.org"],
        vec!["commit", "--allow-empty", "-qm", "Base without Change-Id"],
    ] {
        assert!(git::run(Some(&path), &args).is_ok());
    }
    path
}
fn commit(repo: &Path, message: &str) {
    assert!(git::run(Some(repo), &["commit", "--allow-empty", "-qm", message]).is_ok());
}

#[test]
fn all_commits_in_the_series_are_checked_and_duplicates_are_refused() {
    let path = repo("stack");
    let base = git::head(&path);
    assert!(check_stack(&path, &base, &base).is_err());
    commit(&path, "Missing footer in ancestor");
    commit(&path, &format!("Valid tip\n\nChange-Id: {ID}"));
    assert!(check_stack(&path, &base, &git::head(&path)).is_err());
    assert!(git::run(Some(&path), &["reset", "--hard", &base]).is_ok());
    commit(&path, &format!("First\n\nChange-Id: {ID}"));
    assert_eq!(check_stack(&path, &base, &git::head(&path)), Ok(()));
    commit(&path, &format!("Duplicate\n\nChange-Id: {ID}"));
    assert!(check_stack(&path, &base, &git::head(&path)).is_err());
    assert!(
        git::run(
            Some(&path),
            &[
                "commit",
                "--allow-empty",
                "--amend",
                "-qm",
                &format!("Second\n\nChange-Id: {OTHER}")
            ]
        )
        .is_ok()
    );
    assert_eq!(check_stack(&path, &base, &git::head(&path)), Ok(()));
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn divergence_and_merge_commits_need_a_linear_rebase_first() {
    let path = repo("merge");
    let base = git::head(&path);
    assert!(git::run(Some(&path), &["branch", "side"]).is_ok());
    commit(&path, &format!("Main change\n\nChange-Id: {ID}"));
    let main = git::head(&path);
    assert!(git::run(Some(&path), &["checkout", "side"]).is_ok());
    commit(&path, &format!("Side change\n\nChange-Id: {OTHER}"));
    assert!(check_stack(&path, &main, &git::head(&path)).is_err());
    assert!(git::run(Some(&path), &["checkout", "main"]).is_ok());
    assert!(
        git::run(
            Some(&path),
            &["merge", "--no-ff", "side", "-m", "Merge side"]
        )
        .is_ok()
    );
    assert!(check_stack(&path, &base, &git::head(&path)).is_err());
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn restored_identity_joins_existing_trailers_and_preserves_continuations() {
    let original = format!("Original\n\nChange-Id: {ID}");
    for edited in [
        "New\n\nCo-authored-by: A <a@b>",
        "New\n\nReviewed-by: A\n continued",
    ] {
        let result = amended_message(&original, edited).unwrap();
        assert_eq!(result, format!("{edited}\nChange-Id: {ID}"));
        assert_eq!(change_id(&result), Ok(ID));
    }
    let prose = "New\n\nThis: looks like a trailer\nbut is prose";
    assert_eq!(
        amended_message(&original, prose),
        Ok(format!("{prose}\n\nChange-Id: {ID}"))
    );
}

#[test]
fn preflight_propagates_cancellation_before_validation_and_between_reads() {
    for stop in [0, 1] {
        let mut calls = 0;
        let result = check_stack_with("base", "head", |_| {
            let call = calls;
            calls += 1;
            if call == stop {
                Err(git::CANCELLED.to_owned())
            } else {
                Ok(String::new())
            }
        });
        assert_eq!(result, Err(git::CANCELLED.to_owned()));
        assert_eq!(calls, stop + 1);
    }
}
