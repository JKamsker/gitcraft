use super::*;

#[test]
fn fluent_partition_ghost_actual_notices_keep_refs_opaque_and_counts_numeric() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let branch = "feature/ß-provider";
        let tracking = "upstream/ß (opaque)";
        let plain = diverged_pull(branch, tracking, false);
        let fetched = diverged_pull(branch, tracking, true);
        assert!(plain.contains(branch) && plain.contains(tracking));
        assert!(fetched.contains(branch) && fetched.contains(tracking));
        assert_ne!(plain, fetched);
        let one = dropped_note(1);
        let many = dropped_note(1234);
        if language == "de" {
            assert!(plain.contains("Fast-Forward"));
            assert_eq!(
                one,
                "Ein Commit ausgelassen: Seine Änderung ist bereits vorhanden."
            );
            assert!(many.starts_with("1.234 Commits ausgelassen:"));
        } else {
            assert!(plain.contains("fast-forward-only"));
            assert_eq!(one, "One commit dropped: its change is already there.");
            assert!(many.starts_with("1,234 commits dropped:"));
        }
    }
    crate::i18n::set_language("en-US");
}
