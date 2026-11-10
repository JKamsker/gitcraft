#[test]
fn backend_fluent_deferred_notices_preserve_branch_and_tip() {
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        for (undo, english, german) in [
            (
                false,
                "Accepted de/feature as abc123",
                "de/feature als abc123 übernommen.",
            ),
            (
                true,
                "Restored de/feature as abc123",
                "de/feature als abc123 wiederhergestellt.",
            ),
        ] {
            assert_eq!(
                super::accepted_tip("de/feature", "abc123", undo),
                if locale == "de" { german } else { english }
            );
            let notice = super::guarded_in_flight("de/feature", undo);
            assert!(notice.contains("de/feature"));
            if locale == "de" {
                assert!(notice.contains(if undo {
                    "Rückgängigmachen"
                } else {
                    "Übernahme"
                }));
            } else {
                assert!(notice.contains(if undo { "undo" } else { "acceptance" }));
            }
        }
    }
    crate::i18n::set_language("en-US");
}
