use super::*;
#[test]
fn saved_branch_enters_the_reviewed_plan_without_overriding_explicit_refs() {
    assert_eq!(
        saved_branch_spec("main", "dev", true),
        Some("refs/heads/main:refs/heads/dev".into())
    );
    for (branch, saved, implicit) in [
        ("main", "dev", false),
        ("", "dev", true),
        ("main", "", true),
    ] {
        assert_eq!(saved_branch_spec(branch, saved, implicit), None);
    }
}
