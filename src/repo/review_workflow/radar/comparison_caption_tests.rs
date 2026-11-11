use super::*;

#[test]
fn adaptation_action_retains_raw_payload_with_explicit_caption_provenance() -> Result<(), String> {
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        let action = adapted_fix_comparison("original-1234", "adapted-5678");
        match action {
            Action::Compare {
                base,
                base_label,
                target,
                target_label,
                captions,
                since_fork,
            } => {
                assert_eq!(base, "original-1234");
                assert_eq!(base_label, "Original fix");
                assert_eq!(target.as_deref(), Some("adapted-5678"));
                assert_eq!(target_label, "Adapted fix");
                assert_eq!(
                    captions,
                    [
                        Some(crate::repo::state::CompareCaption::OriginalFix),
                        Some(crate::repo::state::CompareCaption::AdaptedFix)
                    ]
                );
                assert!(!since_fork);
            }
            _ => return Err("fixed comparison action changed".into()),
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
