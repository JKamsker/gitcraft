use super::*;
#[test]
fn fluent_partition_fence_supplied_target_preserves_punctuation_and_raw_gate_identity() {
    let target = "/supplied/x against the agent fence: y/ß";
    let raw = format!(
        "Could not check {target} against the agent fence: there is no main branch to compare with"
    );
    for lang in ["en-US", "de"] {
        crate::i18n::set_language(lang);
        let errors = vec![fence::display::check_diagnostic(&raw, target)];
        let body = confirm_body("refs/heads/ß", Some(target), &[], &errors);
        assert!(body.contains(target));
        assert_eq!(body.contains("keinen Hauptbranch"), lang == "de");
        assert_eq!(fence::display::diagnostic(&raw), raw);
        assert_eq!(fence::display::check_diagnostic(&raw, "different"), raw);
        assert!(format!("Merge refs/heads/ß{PAST}").ends_with(PAST));
        assert_eq!(PAST, " (past the agent fence)");
    }
    crate::i18n::set_language("en-US");
}
