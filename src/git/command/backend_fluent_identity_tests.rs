#[test]
fn backend_fluent_missing_folder_keeps_recent_status_identity() {
    let target = crate::remote::Target {
        host: crate::remote::Host::Ssh("fixture-host".into()),
        path: "/fixture/de/repo".into(),
    };
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        let raw = super::missing_folder(&target);
        assert_eq!(raw, "/fixture/de/repo isn't a folder on SSH · fixture-host");
        assert!(raw.to_lowercase().contains("isn't a folder on"));
        assert_eq!(crate::across::short_reason(&raw), "folder not found");
        assert_eq!(
            crate::git::display_error(&raw),
            if locale == "de" {
                "/fixture/de/repo ist kein Ordner auf SSH · fixture-host."
            } else {
                &raw
            }
        );
    }
    crate::i18n::set_language("en-US");
}
