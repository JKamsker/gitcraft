use super::*;

#[test]
fn finds_the_repository_of_a_pull_request() {
    assert_eq!(
        repo_of_url("https://github.com/o/r/pull/12"),
        Some(("github.com".to_owned(), "o/r".to_owned()))
    );
    assert_eq!(
        repo_of_url("https://codeberg.org/o/r/pulls/12"),
        Some(("codeberg.org".to_owned(), "o/r".to_owned()))
    );
    assert_eq!(
        repo_of_url("https://gitlab.com/g/sub/r/-/merge_requests/3"),
        Some(("gitlab.com".to_owned(), "g/sub/r".to_owned()))
    );
    assert_eq!(repo_of_url("https://github.com/o/r"), None);
    // A repository may be called what the markers say.
    assert_eq!(
        repo_of_url("https://codeberg.org/o/pull/pulls/3"),
        Some(("codeberg.org".to_owned(), "o/pull".to_owned()))
    );
    assert_eq!(
        repo_of_url("https://github.com/acme/pulls/pull/3"),
        Some(("github.com".to_owned(), "acme/pulls".to_owned()))
    );
    assert_eq!(
        repo_of_url("https://codeberg.org/o/pulls/pulls/3#issuecomment-1"),
        Some(("codeberg.org".to_owned(), "o/pulls".to_owned()))
    );
    assert_eq!(
        repo_of_url("https://github.com/o/pull/pull/12"),
        Some(("github.com".to_owned(), "o/pull".to_owned()))
    );
}

fn remote(name: &str, url: &str) -> Remote {
    Remote {
        name: name.to_owned(),
        url: url.to_owned(),
    }
}

#[test]
fn finds_the_remote_of_a_pull_request_whatever_its_url_looks_like() {
    let h = forge::Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: "o/r".into(),
        web: "https://github.com".into(),
    };
    let remotes = [
        remote("fork", "https://github.com/me/r.git"),
        remote("up", "git@github.com:o/r.git"),
    ];
    assert_eq!(
        remote_for(&remotes, "https://github.com/o/r/pull/1", &h),
        Some("up")
    );
    assert_eq!(
        remote_for(&remotes, "https://github.com/x/y/pull/1", &h),
        None
    );
    assert_eq!(
        remote_for(&remotes, "https://github.com/O/R/pull/1", &h),
        Some("up")
    );
}

#[test]
fn review13_remote_lookup_keeps_authority_and_gitlab_path_case() {
    let h = forge::Hosting {
        forge: Forge::Forgejo,
        host: "forge.example".into(),
        path: "Team/Repo".into(),
        web: "https://forge.example:4000".into(),
    };
    let remotes = [
        remote("wrong", "https://forge.example:3000/Team/Repo.git"),
        remote("selected", "https://forge.example:4000/Team/Repo.git"),
    ];
    assert_eq!(
        remote_for(&remotes, "https://forge.example:4000/Team/Repo/pulls/7", &h),
        Some("selected")
    );
    let remotes = [
        remote("wrong", "https://gitlab.example/Group/project.git"),
        remote("selected", "git@gitlab.example:Group/Project.git"),
    ];
    assert_eq!(
        remote_for(
            &remotes,
            "https://gitlab.example/Group/Project/-/merge_requests/7",
            &forge::Hosting {
                forge: Forge::GitLab,
                host: "gitlab.example".into(),
                path: "Group/Project".into(),
                web: "https://gitlab.example".into()
            }
        ),
        Some("selected")
    );
}

#[test]
fn only_full_commit_ids_go_to_git() {
    assert!(is_oid(&"a1".repeat(20)));
    assert!(is_oid(&"F".repeat(64)));
    assert!(!is_oid(""));
    assert!(!is_oid("h0"));
    assert!(!is_oid(&format!("--upload-pack={}", "a".repeat(26))));
    assert!(!is_oid(&"g".repeat(40)));
}

fn entry(kind: EntryKind) -> Entry {
    Entry {
        kind,
        author: "ana".to_owned(),
        at: 0,
        body: String::new(),
        url: String::new(),
        review_id: None,
    }
}

#[test]
fn a_review_without_a_verdict_still_counts_as_a_review() {
    let reviewed = [entry(EntryKind::Review("reviewed"))];
    assert_eq!(reviews_label("", true, &reviewed), "Reviewed");
    let on_a_line = [entry(EntryKind::Inline {
        path: "a.rs".to_owned(),
        line: Some(1),
        side: DiffSide::Right,
        outdated: false,
        resolved: false,
    })];
    assert_eq!(reviews_label("", true, &on_a_line), "Reviewed");
    let talk = [entry(EntryKind::Comment)];
    assert_eq!(reviews_label("", true, &talk), "No reviews yet");
}

#[test]
fn a_verdict_says_more_than_that_there_was_a_review() {
    let reviewed = [entry(EntryKind::Review("approved"))];
    assert_eq!(reviews_label("APPROVED", true, &reviewed), "Approved");
}

#[test]
fn no_review_is_coming_on_a_finished_pull_request() {
    assert_eq!(reviews_label("", false, &[]), "No reviews");
}

#[test]
fn only_commit_ids_and_branch_names_reach_the_fetch() {
    assert!(is_oid(&"a".repeat(40)) && is_oid(&"0".repeat(64)));
    for bad in [
        "",
        "abc",
        "+refs/pull/1/head:refs/heads/main",
        "--update-head-ok",
    ] {
        assert!(!is_oid(bad), "{bad}");
    }
    for good in ["main", "feat/x", "release-1.2", "a@b"] {
        assert!(is_branch_name(good), "{good}");
    }
    for bad in [
        "",
        "-x",
        "x:refs/heads/main",
        "a..b",
        "a b",
        "x.lock",
        "x.lock/y",
        "/x",
        "x/",
        "a@{1}",
        "a/.b",
    ] {
        assert!(!is_branch_name(bad), "{bad}");
    }
    assert!(is_branch_ref("refs/pull/12/head"));
    assert!(!is_branch_ref("+refs/pull/1/head:refs/heads/main"));
    assert!(!is_branch_ref(""));
}

#[test]
fn a_forge_naming_anything_but_commits_gets_nothing_fetched() {
    let h = forge::Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: "o/r".into(),
        web: "https://github.com".into(),
    };
    let details = PrDetails {
        head_oid: "+refs/pull/1/head:refs/heads/main".to_owned(),
        base_oid: "--update-head-ok".to_owned(),
        base_branch: "main".to_owned(),
        ..PrDetails::default()
    };
    let refused = files(Path::new("."), &h, &details, &[], None);
    assert!(refused.is_err_and(|e| e.contains("aren't commit ids")));
    let details = PrDetails {
        head_oid: "a".repeat(40),
        base_oid: "b".repeat(40),
        base_branch: "x:refs/heads/main".to_owned(),
        ..PrDetails::default()
    };
    assert!(
        files(Path::new("."), &h, &details, &[], None).is_err_and(|e| e.contains("can't take"))
    );
}

#[test]
fn only_web_links_may_be_opened() {
    assert!(is_web_url("https://github.com/o/r"));
    assert!(is_web_url("HTTP://example.com"));
    assert!(!is_web_url("file:///C:/Windows/System32/calc.exe"));
    assert!(!is_web_url("javascript:alert(1)"));
    assert!(!is_web_url("ms-settings:"));
    assert!(!is_web_url(""));
}
