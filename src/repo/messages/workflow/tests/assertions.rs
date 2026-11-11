use super::*;

pub(super) fn unknown_workflow_diagnostics() {
    for raw in [
        "Abort custom-deploy failed: Analysis cancelled",
        "Continue custom-script: done",
        "Undo custom-deploy: done",
        "Push: wait for Abort custom-deploy",
        "Preparing custom-script (here only)",
        "Publish custom requests",
        "Turn on: arbitrary provider suggestion",
        "Set key to literal for custom scope",
        "custom path from staging lens",
        "Distribute staged fixes: a custom state is in progress",
        "Restack stopped at refs/00001234 on conflicts: resolve them at provider location, then Continue restack (or Abort restack)",
        "Push: done. provider prose (receipt in the timeline, Ctrl+Shift+Z)",
        "external-custom, and 12 more",
        "external provider: Analysis cancelled",
        "external provider failed: Analysis cancelled",
    ] {
        assert_eq!(display_operation(raw), raw, "operation unknown: {raw}");
        assert_eq!(display_status(raw), raw, "status unknown: {raw}");
    }
    for raw in [
        "Stage1",
        "Before (external)",
        "After (external)",
        "provider-a and provider-b",
        "Original fix",
        "Adapted fix",
    ] {
        assert_eq!(display_operation(raw), raw);
    }
    assert_eq!(headline("origin has 1 commits you don't have"), None);
    assert_eq!(headline("origin has +1 commit you don't have"), None);
    assert_eq!(headline("origin has 1,234 commits you don't have"), None);
    assert!(headline("origin has 1 commit you don't have").is_some());
    assert!(headline("origin has 0 commits you don't have").is_some());
    assert!(headline("origin has nothing you don't have now: push again").is_some());
}
