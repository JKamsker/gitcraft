#[test]
fn guard_display_push_keeps_raw_guard_and_auth_receipt_classification() -> Result<(), String> {
    let reached = std::cell::Cell::new(false);
    let raw = crate::push_preflight::valid_ref(Path::new("inert"), "incomplete", &|| {
        reached.set(true);
        false
    })
    .err()
    .ok_or("expected an error")?;
    if reached.get() {
        return Err("the incomplete-ref guard must not reach Git".into());
    }
    let receipt =
        crate::push_preflight::remember_auth_endpoint("https://example.invalid/team/repo");
    let raw_receipt = receipt.clone();
    for (locale, expected) in [
        (
            "en-US",
            "Choose one complete destination ref, such as refs/heads/main.",
        ),
        (
            "de",
            "Wähle eine vollständige Zielreferenz, etwa refs/heads/main.",
        ),
    ] {
        set_language(locale);
        assert_eq!(diagnostic(&raw), expected);
        assert_eq!(
            raw,
            "Choose one complete destination ref, such as refs/heads/main."
        );
        assert!(crate::push_preflight::is_reviewed_auth_error(&receipt));
        assert_eq!(diagnostic(&receipt), raw_receipt);
        assert_eq!(crate::push_preflight::APPROVED, "\0push-preflight");
        assert_eq!(crate::push_preflight::FIX, "\0push-preflight-fix");
    }
    assert_eq!(
        crate::push_preflight::take_auth_endpoint(&receipt).as_deref(),
        Some("https://example.invalid/team/repo")
    );
    assert_eq!(
        reviewed_auth_failure(),
        "Die Authentifizierung für das geprüfte Push-Ziel ist fehlgeschlagen."
    );
    assert_eq!(receipt, raw_receipt);
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_push_supplied_findings_and_uncertain_recovery_keep_opaque_fields() {
    use crate::push_preflight::{Finding, Kind};
    let finding = Finding {
        kind: Kind::Secret,
        commit: "0123456789abcdef".into(),
        path: "src/12345 key.rs".into(),
        detail: "Likely secret at line 12345; value withheld.".into(),
    };
    let history = "Force push would drop 12345 remote commits, 2345 by other authors. Based on the fetched target; the lease must still match.";
    let directory = "/owned/12345/EXPECT { $detail }";
    let uncertain = format!(
        "The remote repair outcome is unknown. It may still be running; its owned guard and recovery remain in {directory}. Recheck after the host finishes; no replacement or cleanup was dispatched."
    );
    for (locale, number, other) in [("en-US", "12,345", "2,345"), ("de", "12.345", "2.345")] {
        set_language(locale);
        assert!(diagnostic(&finding.detail).contains(number));
        let shown = diagnostic(history);
        assert!(shown.contains(number) && shown.contains(other));
        assert!(diagnostic(&uncertain).contains(directory));
        assert!(diagnostic("5.0 MiB Git blob; consider Git LFS.").contains("5.0 MiB"));
        assert!(
            diagnostic("Author email differs from the profile policy for host/12345/repo.")
                .contains("host/12345/repo")
        );
        assert_eq!(
            finding.detail,
            "Likely secret at line 12345; value withheld."
        );
        assert_eq!(finding.commit, "0123456789abcdef");
        assert_eq!(finding.path, "src/12345 key.rs");
        assert_eq!(finding.kind, Kind::Secret);
        assert_eq!(
            diagnostic("Likely secret at line +12; value withheld."),
            "Likely secret at line +12; value withheld."
        );
        assert_eq!(
            diagnostic("Likely secret at line text; value withheld."),
            "Likely secret at line text; value withheld."
        );
        assert!(target("/owned/12345/repo").ends_with("/owned/12345/repo"));
        assert_eq!(path("src/a\nb.rs"), "src/a\\nb.rs");
        assert_eq!(path("[sensitive path]"), "[sensitive path]");
        assert_eq!(
            target_label("Local repository: /owned/12345/repo"),
            target("/owned/12345/repo")
        );
    }
    set_language("en-US");
}

#[test]
fn captured_push_leases_render_without_changing_raw_witnesses() {
    let pairs = [
        (
            "The push lease must name this exact ref.",
            "Die Push-Lease muss genau diese Referenz benennen.",
        ),
        (
            "The push lease must name this exact ref and object.",
            "Die Push-Lease muss genau diese Referenz und dieses Objekt benennen.",
        ),
        (
            "The push contains contradictory explicit leases; reopen its original action.",
            "Der Push enthält widersprüchliche explizite Leases; öffne die ursprüngliche Aktion erneut.",
        ),
        (
            "The fetched target differs from the captured push lease; reopen its original action.",
            "Das abgerufene Ziel weicht von der erfassten Push-Lease ab; öffne die ursprüngliche Aktion erneut.",
        ),
        (
            "The captured push lease changed; reopen its original action.",
            "Die erfasste Push-Lease wurde geändert; öffne die ursprüngliche Aktion erneut.",
        ),
        (
            "The reviewed fork source changed; reopen push preflight",
            "Die geprüfte Fork-Quelle wurde geändert; öffne die Push-Vorabprüfung erneut",
        ),
    ];
    for locale in ["en-US", "de", "en-US"] {
        set_language(locale);
        for (raw, german) in pairs {
            let before = raw.to_owned();
            let expected = if locale == "de" { german } else { raw };
            assert_eq!(diagnostic(raw), expected);
            assert_eq!(crate::git::display_error(raw), expected);
            assert_eq!(raw, before);
            let unknown = format!("{raw}\nopaque provider detail: refs/heads/a:b");
            assert_eq!(diagnostic(&unknown), unknown);
            assert_eq!(crate::git::display_error(&unknown), unknown);
        }
    }
}
