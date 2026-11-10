use super::*;

#[test]
fn parses_issues() {
    let gl = r###"[
            {"id": 9001, "iid": 12, "title": "Crash on login",
             "description": "## Acceptance criteria\n- [ ] works",
             "web_url": "https://gitlab.com/g/p/-/issues/12", "labels": ["bug", "ui"]},
            {"id": 9002, "iid": 13, "title": "Bare", "description": null, "web_url": "u",
             "labels": []},
            {"id": 9003, "title": "no iid"}
        ]"###;
    let issues = parse_issues(gl).unwrap_or_default();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].number, 12);
    assert_eq!(issues[0].body, "## Acceptance criteria\n- [ ] works");
    assert_eq!(issues[0].url, "https://gitlab.com/g/p/-/issues/12");
    assert_eq!(issues[0].labels, ["bug", "ui"]);
    assert_eq!((issues[1].number, issues[1].body.as_str()), (13, ""));
    assert!(parse_issues("oops").is_err());
    assert!(parse_issues(r#"{"message": "401 Unauthorized"}"#).is_err());
}

#[test]
fn parses_merge_requests() {
    let gl = r#"[{"iid":3,"title":"Add","source_branch":"feat","target_branch":"main","sha":"beef","author":{"username":"bo"},"web_url":"u","draft":false}]"#;
    let mrs = parse_list(gl).unwrap_or_default();
    assert_eq!((mrs[0].number, mrs[0].author.as_str()), (3, "bo"));
    assert_eq!(
        (mrs[0].branch.as_str(), mrs[0].sha.as_str()),
        ("feat", "beef")
    );
    assert_eq!(mrs[0].base, "main");
    assert!(parse_list("oops").is_err());
}

#[test]
fn a_project_is_named_by_its_encoded_path() {
    assert_eq!(project_id("g/sub/p.x").as_deref(), Some("g%2Fsub%2Fp.x"));
    for bad in ["p", "g//p", "g/../p", "g/p?x=1", "-g/p"] {
        assert_eq!(project_id(bad), None, "{bad}");
    }
}

#[test]
fn avatars_are_asked_on_the_remotes_port() {
    let at = |url: &str| {
        let origin = crate::git::Remote {
            name: "origin".into(),
            url: url.into(),
        };
        super::super::detect_with(&[origin], &Default::default(), &[])
            .map(|hosting| api_authority(&hosting))
            .unwrap_or_default()
    };
    assert_eq!(
        at("https://gitlab.example.com:8443/g/p.git"),
        "gitlab.example.com:8443"
    );
    assert_eq!(at("git@gitlab.com:g/p.git"), "gitlab.com");
    // An SSH port says nothing about the web pages'.
    assert_eq!(
        at("ssh://git@gitlab.example.com:2222/g/p.git"),
        "gitlab.example.com"
    );
}
