#[test]
fn backend_fluent_signin_keeps_retry_identity() {
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        let raw = super::not_signed_in("glab", "git.example:8443");
        assert_eq!(
            raw,
            "glab isn't signed in to git.example:8443: `glab auth login --hostname git.example:8443` to use it"
        );
        assert!(crate::forge::inbox::gives_up(&raw));
        let display = crate::forge::inbox::display_error(&raw);
        if locale == "de" {
            assert_eq!(
                display,
                "glab ist nicht bei git.example:8443 angemeldet: zur Verwendung `glab auth login --hostname git.example:8443` ausführen."
            );
        } else {
            assert_eq!(display, raw);
        }
        for opaque in [
            "provider says it isn't signed in to elsewhere",
            "provider isn't signed in to host: `provider auth login --hostname host` to use it",
        ] {
            assert_eq!(crate::forge::inbox::display_error(opaque), opaque);
        }
    }
    crate::i18n::set_language("en-US");
}
