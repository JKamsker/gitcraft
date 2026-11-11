use super::*;

#[test]
fn claims_its_hosts() {
    assert!(FORGEJO.claims("codeberg.org"));
    assert!(FORGEJO.claims("forgejo.example.org"));
    assert!(!FORGEJO.claims("gitea.com"));
    assert!(GITEA.claims("gitea.com"));
    assert!(GITEA.claims("git.gitea.example"));
    assert!(!GITEA.claims("github.com"));
}

#[test]
fn reads_the_list_of_pull_requests() {
    let item = json!({
        "number": 4, "title": "Fix it", "html_url": "https://codeberg.org/o/r/pulls/4",
        "head": {"ref": "fix", "sha": "abc"}, "base": {"ref": "main"}, "user": {"login": "ana"}, "draft": false
    });
    let pr = parse_pull_request(&item);
    assert_eq!(
        (pr.number, pr.branch.as_str(), pr.author.as_str(), pr.draft),
        (4, "fix", "ana", false)
    );
    assert_eq!(pr.url, "https://codeberg.org/o/r/pulls/4");
    assert_eq!(pr.sha, "abc");
    assert_eq!(pr.base, "main");
}

/// Closed and merged are told apart: only the merged ones say a branch's work is in.
#[test]
fn only_merged_pull_requests_count_as_merged() {
    let items = [
        json!({"number": 1, "head": {"ref": "a", "sha": "1"}, "base": {"ref": "main"}, "merged": true}),
        json!({"number": 2, "head": {"ref": "b", "sha": "2"}, "merged": false}),
        json!({"number": 3, "head": {"ref": "c", "sha": "3"}}),
    ];
    let merged = merged_only(&items);
    assert_eq!(merged.len(), 1);
    assert_eq!(
        (
            merged[0].number,
            merged[0].branch.as_str(),
            merged[0].base.as_str()
        ),
        (1, "a", "main")
    );
    assert!(merged_only(&[]).is_empty());
}

#[test]
fn a_work_in_progress_title_is_a_draft() {
    for title in ["WIP: x", "[WIP] x", "wip: x", "  [wip] x"] {
        assert!(is_draft_title(title), "{title}");
    }
    assert!(!is_draft_title("Wipe the cache"));
    let item = json!({"number": 1, "title": "[WIP] x", "user": null});
    let pr = parse_pull_request(&item);
    assert!(pr.draft);
    assert_eq!(pr.author, "Ghost");
}

#[test]
fn reads_teas_logins() {
    let config = "logins:\n    - name: codeberg\n      url: https://codeberg.org\n      \
            token: \"abc123\"\n      default: true\n    - name: home\n      url: \
            'https://git.home.example:3000/'\n      token: def\n      user: me\npreferences:\n    \
            editor: false\n    url: https://not-a-login.example\n";
    let logins = parse_tea_config(config);
    assert_eq!(
        logins,
        [
            TeaLogin {
                name: "codeberg".into(),
                user: String::new(),
                url: "https://codeberg.org".into(),
                token: "abc123".into()
            },
            TeaLogin {
                name: "home".into(),
                user: "me".into(),
                url: "https://git.home.example:3000/".into(),
                token: "def".into()
            },
        ]
    );
}

#[test]
fn teas_list_can_sit_flush_with_its_key() {
    let config = "logins:\n- name: a\n  url: https://a.example\n  token: t\n";
    assert_eq!(parse_tea_config(config)[0].token, "t");
    assert!(parse_tea_config("preferences:\n  editor: false\n").is_empty());
}

#[test]
fn a_login_is_only_for_its_own_server() {
    assert!(same_server("https://Codeberg.org/", "https://codeberg.org"));
    assert!(same_server(
        "https://Codeberg.org:443/reviews/",
        "https://codeberg.org/reviews"
    ));
    assert!(!same_server(
        "https://codeberg.org/REVIEWS",
        "https://codeberg.org/reviews"
    ));
    assert!(!same_server(
        "https://codeberg.org",
        "https://codeberg.org:3000"
    ));
    assert!(!same_server("http://codeberg.org", "https://codeberg.org"));
    assert!(!same_server("https://evil.example", "https://codeberg.org"));
}
