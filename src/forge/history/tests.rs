use super::*;
use serde_json::json;
fn query() -> Query {
    Query {
        at: Seed {
            commit: "a".repeat(40),
            path: "src/café.rs".into(),
            line: 7,
        },
        file_oid: "b".repeat(40),
        comments: true,
    }
}
fn pull() -> Pull {
    Pull {
        number: 1,
        title: String::new(),
        url: String::new(),
        state: "merged".into(),
        association: String::new(),
        issues: vec![],
        comments: vec![],
        checked: 0,
        unmapped: 0,
        limited: false,
        notes: vec![],
    }
}
#[test]
fn github_matches_exact_or_proven_identical_revisions_and_never_unrelated_lines() {
    let q = query();
    let pr = json!({"head":{"sha":"c".repeat(40)},"base":{"sha":"d".repeat(40)}});
    let mut p = pull();
    let mut prove = |rev: &str, _: &str| rev == q.at.commit || rev == "c".repeat(40);
    github_comment(
        &json!({"path":q.at.path,"side":"RIGHT","commit_id":q.at.commit,"line":7,"body":"exact","user":{"login":"reviewer"},"html_url":"https://github.com/o/r/pull/1#discussion_r1"}),
        &pr,
        &q,
        &mut prove,
        &mut p,
    );
    github_comment(
        &json!({"path":q.at.path,"side":"RIGHT","original_commit_id":"c".repeat(40),"original_line":7,"body":"same blob"}),
        &pr,
        &q,
        &mut prove,
        &mut p,
    );
    github_comment(
        &json!({"path":q.at.path,"side":"RIGHT","commit_id":q.at.commit,"line":8,"body":"other line"}),
        &pr,
        &q,
        &mut prove,
        &mut p,
    );
    github_comment(
        &json!({"path":"unrelated.rs","side":"RIGHT","commit_id":q.at.commit,"line":7,"body":"other file"}),
        &pr,
        &q,
        &mut prove,
        &mut p,
    );
    github_comment(
        &json!({"path":q.at.path,"side":"RIGHT","commit_id":"e".repeat(40),"line":7,"body":"unavailable revision"}),
        &pr,
        &q,
        &mut prove,
        &mut p,
    );
    assert_eq!(p.comments.len(), 2);
    assert_eq!(p.unmapped, 3);
    assert!(p.comments[1].proof.contains("Identical pinned file"));
    assert!(p.comments.iter().all(|c| !c.body.starts_with("other")));
}
#[test]
fn gitlab_and_gitea_use_their_revision_path_and_side_coordinates() {
    let q = query();
    let mut p = pull();
    let mut prove = |rev: &str, _: &str| rev == q.at.commit;
    gitlab_comment(
        &json!({"position":{"head_sha":q.at.commit,"new_path":q.at.path,"new_line":7},"body":"new-side note","author":{"username":"a"}}),
        &q,
        &mut prove,
        &mut p,
    );
    gitlab_comment(
        &json!({"position":{"base_sha":q.at.commit,"old_path":q.at.path,"old_line":7},"body":"old-side note"}),
        &q,
        &mut prove,
        &mut p,
    );
    gitea_comment(
        &json!({"path":q.at.path,"position":7,"body":"review-head note"}),
        &q.at.commit,
        &q,
        &mut prove,
        &mut p,
    );
    gitea_comment(
        &json!({"path":q.at.path,"position":0,"original_position":7,"commit_id":q.at.commit,"body":"old-side note"}),
        "",
        &q,
        &mut prove,
        &mut p,
    );
    assert_eq!(p.comments.len(), 4);
    assert_eq!(p.checked, 4);
}
#[test]
fn association_pagination_and_issue_links_have_explicit_budgets() -> Result<(), String> {
    let h = Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: "o/r".into(),
        web: "https://github.com".into(),
    };
    let q = query();
    let mut paths = Vec::new();
    let mut get = |path: &str| {
        paths.push(path.to_owned());
        if path.contains("/commits/") {
            Ok(json!([{"number":1}]))
        } else if path.contains("/comments?") {
            Ok(
                json!([{ "id":1,"path":q.at.path,"side":"RIGHT","line":7,"commit_id":q.at.commit,"body":"bounded comment"}]),
            )
        } else {
            Ok(
                json!({"title":"Introduced","body":"Fixes #42. Closes #9","state":"closed","merge_commit_sha":q.at.commit,"html_url":"https://github.com/o/r/pull/1"}),
            )
        }
    };
    let result = lookup_with(&h, &q, &mut get, &mut |_, _| true)?;
    assert_eq!(result.pulls.len(), 1);
    assert_eq!(result.pulls[0].issues.len(), 2);
    assert!(
        result.limited,
        "An endpoint that repeats pages is visibly partial"
    );
    assert!(paths.len() <= 4);
    assert!(
        !paths
            .iter()
            .any(|p| p.contains("checks") || p.contains("actions"))
    );
    Ok(())
}
#[test]
fn invalid_ranges_urls_and_malformed_lists_are_not_evidence() {
    assert!(!safe_url("javascript:alert(1)"));
    assert!(!safe_url("https://user:secret@host/x"));
    assert!(!safe_url("https://"));
    assert!(range(&json!({"line":7,"start":8}), "/line", "/start").is_none());
    assert!(range(&json!({"line":u64::MAX}), "/line", "/start").is_none());
    assert!(list(json!({"unexpected":true})).is_err());
    assert_eq!(component("a/b ü"), "a%2Fb%20%C3%BC");
}
