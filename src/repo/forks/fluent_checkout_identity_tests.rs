use super::*;
#[test]
fn fluent_partition_actual_fork_checkout_dispatch_keeps_opaque_pr_number_and_owner() {
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        let raw = checkout_operation_label(1234, "raw-owner/ß");
        assert_eq!(raw, "Check out #1234 to push to raw-owner/ß");
        let painted = crate::i18n::message(
            "repo-workflows-forks-checkout-fork-abaaf4c3",
            &[
                ("number", 1234u64.to_string().into()),
                ("info-owner-1", "raw-owner/ß".into()),
            ],
        );
        assert!(painted.contains("#1234"));
        assert!(painted.contains("raw-owner/ß"));
        if language == "en-US" {
            assert_eq!(painted, raw)
        } else {
            assert_ne!(painted, raw)
        }
    }
}
