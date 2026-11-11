use super::*;
#[test]
fn fluent_partition_forks_actual_tidy_preview_keeps_remote_names_opaque() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for keep in [vec![], vec!["fork/1234-ß".into()]] {
            let body = tidy_body(&["unused/1234".into()], &keep);
            assert!(body.contains("unused/1234"));
            assert_eq!(body.contains("fork/1234-"), !keep.is_empty());
            if !keep.is_empty() {
                assert!(body.contains(if language == "de" {
                    "Behalten:"
                } else {
                    "Kept:"
                }));
            }
        }
    }
    crate::i18n::set_language("en-US");
}
