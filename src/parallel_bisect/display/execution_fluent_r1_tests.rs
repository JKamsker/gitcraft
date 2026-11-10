use super::*;

#[test]
fn unwrapped_unknown_cleanup_and_keep_lookalikes_remain_opaque_in_both_languages() {
    for locale in ["en-US", "de"] {
        i18n::set_language(locale);
        for raw in [
            "provider text; workspace cleanup: provider detail",
            "provider text; quarantine cleanup failed: provider detail",
            "provider text; transfer cleanup: provider detail",
            "provider text; lease release: provider detail",
            "provider text; owned pack-01234567.keep retained",
            "provider text; owned pack-01234567.keep retained\n",
            "provider text; workspace cleanup: first; workspace cleanup: second (exit 7); stderr: raw $HOME {detail}\n",
        ] {
            assert_eq!(diagnostic(raw), raw);
            let prefixed = format!("01234567: {raw}");
            assert_eq!(warning(&prefixed), prefixed);
        }
    }
    i18n::set_language("en-US");
}

#[test]
fn supplied_source_context_keeps_delimiters_framing_and_hashes_opaque() {
    let primary = "provider; transfer cleanup: inner; owned pack-opaque.keep retained\n(exit 7); stderr: $HOME {cleanup}";
    let cleanup = "provider; lease release: inner; workspace cleanup: nested\n";
    let hash = "0123456789012345678901234567890123456789";
    for (locale, transfer, lease, retained) in [
        ("en-US", "transfer cleanup", "lease release", "owned pack-"),
        (
            "de",
            "Bereinigung der Übertragung",
            "Freigabe der Besitzreservierung",
            "die eigene Schutzmarkierung pack-",
        ),
    ] {
        i18n::set_language(locale);
        let raw_transfer = format!("{primary}; transfer cleanup: {cleanup}");
        let transfer_shown = cleanup_diagnostic(
            &raw_transfer,
            CleanupContext::WorkspaceTransfer { primary, cleanup },
        );
        assert_eq!(transfer_shown, format!("{primary}; {transfer}: {cleanup}"));
        let raw_lease = format!("{primary}; lease release: {cleanup}");
        let lease_shown = cleanup_diagnostic(
            &raw_lease,
            CleanupContext::WorkspaceLeaseRelease {
                primary,
                release: cleanup,
            },
        );
        assert_eq!(lease_shown, format!("{primary}; {lease}: {cleanup}"));
        let raw_keep = format!("{primary}; owned pack-{hash}.keep retained");
        let keep_shown = cleanup_diagnostic(
            &raw_keep,
            CleanupContext::BundleKeepRetained { primary, hash },
        );
        let suffix = if locale == "de" {
            ".keep wurde aufbewahrt."
        } else {
            ".keep retained."
        };
        assert_eq!(keep_shown, format!("{primary}; {retained}{hash}{suffix}"));
        assert_eq!(diagnostic(&raw_transfer), raw_transfer);
        assert_eq!(diagnostic(&raw_lease), raw_lease);
        assert_eq!(diagnostic(&raw_keep), raw_keep);
    }
    i18n::set_language("en-US");
}

#[test]
fn stale_supplied_context_cannot_replace_the_original_raw_error() {
    for locale in ["en-US", "de"] {
        i18n::set_language(locale);
        let raw = "original provider error; workspace cleanup: provider detail\n";
        for context in [
            CleanupContext::BundleCompleted {
                primary: "other",
                cleanup: "other",
            },
            CleanupContext::ObservationFinish {
                primary: "other",
                cleanup: "other",
            },
            CleanupContext::WorkspaceTransfer {
                primary: "other",
                cleanup: "other",
            },
            CleanupContext::WorkspaceLeaseRelease {
                primary: "other",
                release: "other",
            },
            CleanupContext::BundleKeepRetained {
                primary: "other",
                hash: "other",
            },
        ] {
            assert_eq!(cleanup_diagnostic(raw, context), raw);
        }
    }
    i18n::set_language("en-US");
}
