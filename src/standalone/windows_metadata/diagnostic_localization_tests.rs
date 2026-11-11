use super::*;

#[test]
fn actual_failure_envelope_localizes_source_bound_named_stream_and_native_policy_errors()
-> Result<(), String> {
    // These exact source bindings stop the finite classifier drifting from the
    // unchanged helper. This test never constructs or executes a native command.
    assert!(SCRIPT.contains(&format!("'{HELPER_FAILURE}' + $_.Exception.Message")));
    for (raw, _) in HELPER_DIAGNOSTICS {
        assert!(SCRIPT.contains(&format!("'{raw}'")) || SCRIPT.contains(&format!("\"{raw}\"")));
    }
    assert!(SCRIPT.contains(&format!(
        "\"{NATIVE_POLICY_PREFIX}\" + error + \"{NATIVE_POLICY_SUFFIX}\""
    )));
    assert!(SCRIPT.contains("static void Check(uint error)"));
    assert!(SCRIPT.contains("if (error != 0)"));
    assert!(SCRIPT.contains("public static byte[] Read(string path)"));
    assert!(SCRIPT.contains("public static void Copy(string source,string target)"));
    let cases = [
        ("Missing metadata paths", "Metadatenpfade fehlen"),
        (
            "Named data streams cannot be preserved with the bounded save protocol",
            "Benannte Datenströme können mit dem begrenzten Speicherprotokoll nicht erhalten werden",
        ),
        (
            "Full native descriptor exceeds 1 MiB",
            "Der vollständige native Sicherheitsdeskriptor überschreitet 1 MiB",
        ),
        (
            "Full native policy differs after copy",
            "Die vollständige native Zugriffsrichtlinie weicht nach dem Kopieren ab",
        ),
        (
            "Source policy changed during copy",
            "Die Zugriffsrichtlinie der Quelle hat sich beim Kopieren geändert",
        ),
    ];
    for (locale, outer, prefix) in [
        (
            "en-US",
            "Windows access metadata could not be preserved; original unchanged: ",
            HELPER_FAILURE,
        ),
        (
            "de",
            "Die Windows-Zugriffsmetadaten konnten nicht erhalten werden; das Original ist unverändert: ",
            "Die begrenzten Windows-Zugriffsmetadaten konnten nicht erhalten werden: ",
        ),
    ] {
        crate::i18n::set_language(locale);
        for (en, de) in cases {
            for ending in ["", "\n", "\r\n"] {
                let input = format!("{HELPER_FAILURE}{en}{ending}");
                let expected = if locale == "de" { de } else { en };
                assert_eq!(
                    display_failure(&input),
                    format!("{outer}{prefix}{expected}{ending}")
                );
            }
        }
        // Error codes identify native failures; they must never acquire locale
        // grouping, a sign, or another spelling, even at uint's maximum value.
        for code in [5u32, 1234, u32::MAX] {
            let en = format!("{NATIVE_POLICY_PREFIX}{code}{NATIVE_POLICY_SUFFIX}");
            let translated = if locale == "de" {
                format!(
                    "Die vollständige Windows-Zugriffsrichtlinie ist nicht verfügbar (nativer Fehler {code}); der Zugriff auf SACL/Auditdaten erfordert möglicherweise SeSecurityPrivilege. Das Original muss unverändert bleiben."
                )
            } else {
                en.clone()
            };
            for frame in [
                "",
                "Exception calling \"Read\" with \"1\" argument(s): \"",
                "Exception calling \"Copy\" with \"2\" argument(s): \"",
            ] {
                let close = if frame.is_empty() { "" } else { "\"" };
                let input = format!("{HELPER_FAILURE}{frame}{en}{close}\r\n");
                assert_eq!(
                    display_failure(&input),
                    format!("{outer}{prefix}{frame}{translated}{close}\r\n")
                );
            }
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn actual_failure_envelope_preserves_unknown_details_and_rejects_near_matches() {
    let unknown = [
        "opaque OS error: ' $() & ü\n{}\r\n",
        "Named data streams cannot be preserved with the bounded save protocol (external detail)",
        "named data streams cannot be preserved with the bounded save protocol",
        "Missing metadata paths ",
        "before Missing metadata paths after",
        "Full native policy differs after copy\nextra detail",
        "Exception calling \"Other\" with \"1\" argument(s): \"Full native policy differs after copy\"",
        "Exception calling \"Read\" with \"2\" argument(s): \"Full native policy differs after copy\"",
        "Exception calling \"Read\" with \"1\" argument(s): \"Full native policy differs after copy\" extra",
        "Exception calling \"Copy\" with \"2\" argument(s): \"opaque detail: ' $() & ü\n{}\"",
        "gitcraft-acl-v1-ok",
    ];
    for (locale, outer, prefix) in [
        (
            "en-US",
            "Windows access metadata could not be preserved; original unchanged: ",
            HELPER_FAILURE,
        ),
        (
            "de",
            "Die Windows-Zugriffsmetadaten konnten nicht erhalten werden; das Original ist unverändert: ",
            "Die begrenzten Windows-Zugriffsmetadaten konnten nicht erhalten werden: ",
        ),
    ] {
        crate::i18n::set_language(locale);
        for raw in unknown {
            // Unframed stderr cannot be attributed to this helper's catch.
            assert_eq!(display_failure(raw), format!("{outer}{raw}"));
            // The known catch sentence translates, while all unknown cause bytes
            // (including whitespace, line endings, braces and shell text) survive.
            let input = format!("{HELPER_FAILURE}{raw}");
            assert_eq!(display_failure(&input), format!("{outer}{prefix}{raw}"));
        }
        for raw in [
            "0",
            "00",
            "05",
            "+5",
            "-5",
            " 5",
            "5 ",
            "５",
            "5.0",
            "5e0",
            "4294967296",
            "5) spoof",
        ] {
            let cause = format!("{NATIVE_POLICY_PREFIX}{raw}{NATIVE_POLICY_SUFFIX}");
            assert_eq!(
                display_failure(&format!("{HELPER_FAILURE}{cause}")),
                format!("{outer}{prefix}{cause}")
            );
        }
        for raw in [
            format!(
                "could not preserve bounded Windows access metadata: {}",
                unknown[1]
            ),
            format!(" {HELPER_FAILURE}Missing metadata paths"),
            format!("before {HELPER_FAILURE}Missing metadata paths"),
            format!("{HELPER_FAILURE}Missing metadata paths\r"),
        ] {
            let expected = if let Some(rest) = raw.strip_prefix(HELPER_FAILURE) {
                format!("{outer}{prefix}{rest}")
            } else {
                format!("{outer}{raw}")
            };
            assert_eq!(display_failure(&raw), expected);
        }
    }
    crate::i18n::set_language("en-US");
}
