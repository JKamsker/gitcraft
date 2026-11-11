#[test]
fn backend_fluent_hook_keeps_toolbar_header_and_opaque_output() -> Result<(), String> {
    let said = "provider HTTP 409: the other hook stopped it (exit code 7)\n/refs/heads/de/main";
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        for (name, args, prefix) in [
            ("pre-commit", vec!["commit"], "the "),
            ("pre-push", vec!["push"], "the "),
            ("post-checkout", vec!["checkout"], "it was done, but the "),
        ] {
            let hook = super::FailedHook {
                name: name.into(),
                code: 1234,
            };
            let raw = hook.explain(said, &args);
            let header = raw.lines().next().ok_or("missing hook header")?;
            assert!(header.starts_with(prefix) && header.contains(" hook "));
            assert!(raw.contains("exit code 1234)"));
            assert!(raw.contains(said));
            let display = crate::git::display_error(&raw);
            assert!(display.contains(said));
            if locale == "de" {
                assert!(display.contains("Hook") && display.contains("Exit-Code 1234"));
                assert_ne!(display, raw);
            } else {
                assert_eq!(display, raw);
            }
        }
        for opaque in [
            "fatal: provider hook output",
            "the unknown-provider hook stopped it (exit code 7)",
        ] {
            assert_eq!(crate::git::display_error(opaque), opaque);
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
