#[test]
fn backend_fluent_symbol_rename_diagnostic_preserves_raw_paths() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        assert_eq!(super::rename_path(b"de/raw path.rs"), Ok("de/raw path.rs"));
        assert_eq!(
            super::rename_path(b"de/\xff.rs"),
            Err(if language == "de" {
                "Der Umbenennungspfad ist nicht UTF-8-kodiert."
            } else {
                "Non-UTF-8 rename"
            })
        );
    }
    crate::i18n::set_language("en-US");
}
