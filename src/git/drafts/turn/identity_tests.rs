use super::*;
fn patch(mode: &str, hunks: &str) -> String {
    format!("diff --git a/f b/f\nindex aaaa..bbbb {mode}\n--- a/f\n+++ b/f\n{hunks}")
}
#[test]
fn pure_turn_review_identity_distinguishes_duplicate_locations_and_file_modes() {
    let text = " same\n-old\n+new\n same\n";
    let raw = patch(
        "100644",
        &format!("@@ -1,3 +1,3 @@\n{text}@@ -21,3 +21,3 @@\n{text}"),
    );
    let hunks = split(&raw, &BTreeSet::new());
    assert_eq!(hunks.len(), 2);
    assert_ne!(hunks[0].digest, hunks[1].digest);
    let known = [hunks[0].digest.clone()].into();
    let marked = split(&raw, &known);
    assert!(marked[0].reviewed);
    assert!(!marked[1].reviewed);
    let unique = patch("100644", &format!("@@ -1,3 +1,3 @@\n{text}"));
    assert!(!split(&unique, &known)[0].reviewed);
    let unique_mark = [split(&unique, &BTreeSet::new())[0].digest.clone()].into();
    let shifted = patch("100644", &format!("@@ -11,3 +11,3 @@\n{text}"));
    assert!(split(&shifted, &unique_mark)[0].reviewed);
    assert!(
        !split(
            &patch("100755", &format!("@@ -1,3 +1,3 @@\n{text}")),
            &unique_mark
        )[0]
        .reviewed
    );
    assert!(split(&raw, &unique_mark).iter().all(|h| !h.reviewed));
    let mode_change = format!(
        "diff --git a/f b/f\nold mode 100644\nnew mode 100755\nindex aaaa..bbbb\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n{text}"
    );
    assert!(!split(&mode_change, &unique_mark)[0].reviewed);
    let old_key = digest(format!("diff --git a/f b/f\n{}", text.trim_end()).as_bytes());
    assert!(!split(&unique, &[old_key].into())[0].reviewed);
}
