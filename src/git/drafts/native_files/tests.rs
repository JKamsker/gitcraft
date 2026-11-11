use super::*;
#[test]
fn pure_native_capture_decisions_preserve_foreign_and_unknown_entries() {
    assert_eq!(restoration(Captured::Owned, true), Restore::Original);
    assert_eq!(restoration(Captured::Owned, false), Restore::Retain);
    assert_eq!(restoration(Captured::Foreign, true), Restore::Captured);
    assert_eq!(restoration(Captured::Foreign, false), Restore::Captured);
    assert_eq!(restoration(Captured::Unreadable, true), Restore::Captured);
    // Pure decision matrix only. Native capture ordering and create-only
    // filesystem interleavings remain held for independent validation.
}
