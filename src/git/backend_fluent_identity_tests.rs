#[test]
fn backend_fluent_outside_repository_keeps_drop_classification() {
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        let raw = super::outside_repository(std::path::Path::new("/fixture/de/repo"));
        assert_eq!(raw, "/fixture/de/repo is not inside a git repository");
        assert!(raw.to_lowercase().contains("not inside a git repository"));
        assert_eq!(
            super::display_error(&raw),
            if locale == "de" {
                "/fixture/de/repo liegt nicht in einem Git-Repository."
            } else {
                &raw
            }
        );
    }
    crate::i18n::set_language("en-US");
}
