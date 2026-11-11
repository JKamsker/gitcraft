use super::*;

#[test]
fn headline_prefers_conflicts_and_errors() {
    let merge = "Merge x failed: Auto-merging a.rs
CONFLICT (content): Merge conflict in a.rs
Automatic merge failed";
    assert_eq!(
        headline(merge),
        "Merge x failed: conflicts in 1 file, resolve them in Changes"
    );
    let push = "Push failed: To origin
 ! [rejected] main
error: failed to push some refs";
    assert_eq!(
        headline(push),
        "Push failed: error: failed to push some refs"
    );
    assert_eq!(headline("Fetch: done"), "Fetch: done");
    // A hook that stopped it is the headline, not git's own error line after its output.
    let hook = "Push failed: the pre-push hook stopped it (exit code 1)
error: tests failed
error: failed to push some refs to 'origin'";
    assert_eq!(
        headline(hook),
        "Push failed: the pre-push hook stopped it (exit code 1)"
    );
    let reused = "Merge x failed: CONFLICT (content): Merge conflict in a.rs
CONFLICT (content): Merge conflict in b.rs
Resolved 'a.rs' using previous resolution.
Automatic merge failed";
    assert_eq!(
        headline(reused),
        "Merge x failed: conflicts in 2 files, 1 resolved as before: check it, resolve the rest \
         in Changes"
    );
    let all = "Merge x failed: CONFLICT (content): Merge conflict in a.rs
Resolved 'a.rs' using previous resolution.";
    assert_eq!(
        headline(all),
        "Merge x failed: 1 file resolved as before, check it in Changes"
    );
}
