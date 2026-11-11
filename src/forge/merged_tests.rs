use super::*;

#[test]
fn merged_lists_are_read_with_their_base() {
    let json = r#"[{"number": 5, "headRefName": "fix/ü", "headRefOid": "abc", "baseRefName": "main"},
                       {"number": 6, "headRefName": "", "headRefOid": "def", "baseRefName": "main"}]"#;
    let read = parse_merged(
        json,
        ["/number", "/headRefName", "/headRefOid", "/baseRefName"],
        "gh",
    );
    assert_eq!(
        read,
        Ok(vec![MergedPullRequest {
            number: 5,
            branch: "fix/ü".to_owned(),
            sha: "abc".to_owned(),
            base: "main".to_owned(),
        }])
    );
    assert_eq!(parse_merged("[]", ["", "", "", ""], "gh"), Ok(Vec::new()));
    assert!(parse_merged("not json", ["", "", "", ""], "gh").is_err());
}
