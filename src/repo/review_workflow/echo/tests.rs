use super::*;
#[test]
fn only_new_matching_lines_get_personal_cited_observations() -> Result<(), String> {
    let comment = Comment {
        url: "https://forge/review/1".into(),
        body: "No unwrap in handlers".into(),
        author: "me".into(),
    };
    let book = Book {
        enabled: true,
        comments: vec![comment],
        rules: vec![Rule {
            summary: "avoid unwrap".into(),
            pattern: r"\.unwrap\(\)".into(),
            source: "https://forge/review/1".into(),
            enabled: true,
        }],
    };
    let hits = check_patch(
        "+++ b/handler.rs\n@@ -1,2 +1,2 @@\n-old.unwrap()\n+new.unwrap()\n context.unwrap()",
        &book,
    )?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].path, "handler.rs");
    let disguised = check_patch(
        "diff --git a/real.rs b/real.rs\n--- a/real.rs\n+++ b/real.rs\n@@ -0,0 +1 @@\n+++ b/fake.unwrap()\ndiff --git a/next.rs b/next.rs\n--- a/next.rs\n+++ b/next.rs\n@@ -0,0 +1 @@\n+next.unwrap()",
        &book,
    )?;
    assert_eq!(disguised.len(), 2);
    assert_eq!(disguised[0].path, "real.rs");
    assert_eq!(disguised[1].path, "next.rs");
    let quoted = check_patch(
        "+++ \"b/quoted\\tname.rs\"\n@@ -0,0 +1 @@\n+x.unwrap()",
        &book,
    )?;
    assert_eq!(quoted[0].path, "quoted\tname.rs");
    let mut forged = book.clone();
    forged.rules[0].source = "https://invented".into();
    assert!(check_patch("", &forged).is_err());
    forged.enabled = false;
    assert!(check_patch("+new.unwrap()", &forged)?.is_empty());
    Ok(())
}
