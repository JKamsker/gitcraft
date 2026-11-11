use super::*;

#[test]
fn cleanup_reason_interpolates_opaque_branch_and_numeric_counts() {
    let reason = Reason::Merged(Merged::Pr(1234, "Commit/日本語".to_owned()));
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let shown = reason.describe();
        assert!(shown.contains("Commit/日本語"));
        assert!(shown.contains(if language == "de" { "1.234" } else { "1,234" }));
        assert!(reason.is_merged());
        assert_eq!(
            Reason::Stale(1).describe(),
            if language == "de" {
                "seit 1 Tag kein Commit"
            } else {
                "no commit for 1 day"
            }
        );
        let stale = Reason::Stale(1234);
        assert!(!stale.is_merged());
        assert!(
            stale
                .describe()
                .contains(if language == "de" { "1.234" } else { "1,234" })
        );
    }
    crate::i18n::set_language("en-US");
}
