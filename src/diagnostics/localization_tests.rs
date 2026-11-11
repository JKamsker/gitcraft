use super::*;

#[test]
fn actual_supplied_report_body_localizes_statuses_and_preserves_raw_rows_logs_and_redaction() {
    let facts = Facts {
        channel: "dev".into(),
        adapter: Some("opaque GPU 日本語".into()),
        repo: None,
    };
    let origins = config::parse_origins(
        "global\0file:/home/fixture/.gitconfig\0core.editor\nenv TOKEN=FIXTURE_SECRET editor\0local\0file:.git/config\0pull.rebase\nfalse\0",
    );
    let mut rows = settings_rows(&origins);
    rows.insert(0, "# repository on this machine".into());
    let original_rows = rows.clone();
    let settings = Ok(rows);
    let tools = [
        crate::forge::ToolReport {
            tool: "gh",
            version: Some("gh version opaque".into()),
            signed_in: Some(true),
        },
        crate::forge::ToolReport {
            tool: "glab",
            version: Some("glab opaque".into()),
            signed_in: Some(false),
        },
        crate::forge::ToolReport {
            tool: "tea",
            version: None,
            signed_in: None,
        },
        crate::forge::ToolReport {
            tool: "extra",
            version: Some("opaque version".into()),
            signed_in: None,
        },
    ];
    let panic = "raw log marker /home/fixture/private token=FIXTURE_TOKEN";
    let recent = vec!["git push [arguments and paths redacted] · exit 0 · 12 ms".to_owned()];
    crate::i18n::set_language("de");
    let de = report_body(
        &facts,
        "git version opaque",
        &tools,
        Some(&settings),
        "/home/fixture",
        ("recorded system", Some(panic), &recent),
    );
    for text in [
        "### gitcraft-Diagnose",
        "Kanal dev",
        "Grafik",
        "opaque GPU 日本語",
        "gh version opaque, angemeldet",
        "glab opaque, Prüfung des Anmeldestatus fehlgeschlagen",
        "**tea:** nicht gefunden (oder keine Antwort)",
        "opaque version, Anmeldung unbekannt",
        "# Repository auf diesem Rechner",
        "core.editor = (konfiguriert; Wert zurückgehalten) · Global · .gitconfig",
        "pull.rebase = false · Repository · config",
        "Letzte Panic",
        "Letzte Vorgänge",
        "git push [arguments and paths redacted] · exit 0 · 12 ms",
    ] {
        assert!(de.contains(text), "missing {text}: {de}");
    }
    for secret in [
        "FIXTURE_SECRET",
        "FIXTURE_TOKEN",
        "/home/fixture",
        "env TOKEN=",
    ] {
        assert!(!de.contains(secret), "{de}");
    }
    assert_eq!(settings.as_ref().ok(), Some(&original_rows));
    assert_eq!(tools[0].version.as_deref(), Some("gh version opaque"));
    let empty = report_body(
        &facts,
        "not found (opaque external detail)",
        &[],
        None,
        "",
        ("recorded system", None, &[]),
    );
    assert!(empty.contains("nicht gefunden (opaque external detail)"));
    assert!(empty.contains("Kein Repository geöffnet: keine Git-Einstellungen."));
    let failed = Err("ssh://user@host/private/FIXTURE_UNKNOWN".to_owned());
    let failure = report_body(
        &facts,
        "git",
        &[],
        Some(&failed),
        "",
        ("recorded system", None, &[]),
    );
    assert!(failure.contains("Git-Einstellungen konnten nicht gelesen werden."));
    assert!(!failure.contains("FIXTURE_UNKNOWN"));
    let no_rows = Ok(vec![]);
    assert!(
        report_body(
            &facts,
            "git",
            &[],
            Some(&no_rows),
            "",
            ("recorded system", None, &[])
        )
        .contains("Keine der aufgeführten Einstellungen ist gesetzt.")
    );
    crate::i18n::set_language("en-US");
    let en = report_body(
        &facts,
        "git version opaque",
        &tools,
        Some(&settings),
        "/home/fixture",
        ("recorded system", None, &recent),
    );
    assert!(en.starts_with("### gitcraft diagnostics\n\n"));
    assert!(en.contains("dev channel"));
    assert!(en.contains("gh version opaque, signed in"));
    for row in &original_rows {
        assert_eq!(display_setting_row(row), *row);
    }
    for unknown in [
        "opaque external row 日本語",
        "core.editor = unknown · Global · .gitconfig",
        "core.editor = (configured; value withheld) · Unknown · .gitconfig",
    ] {
        assert_eq!(display_setting_row(unknown), unknown);
    }
}

#[test]
fn actual_issue_url_keeps_protocol_and_encoded_data_while_localizing_human_title_prompt_and_limit_notice()
 {
    crate::i18n::set_language("de");
    let report = "opaque 日本語 & ?\n".repeat(5000);
    let original = report.clone();
    let url = issue_url(&report);
    assert!(
        url.starts_with(
            "https://github.com/JKamsker/gitcraft/issues/new?title=Fehlerbericht&body="
        )
    );
    assert!(url.contains(&encode(
        "Bitte beschreibe, was passiert ist und wie es sich reproduzieren lässt."
    )));
    assert!(url.contains(&encode(
        "[Die übrigen Diagnosedaten befinden sich in der Zwischenablage.]"
    )));
    assert!(url.contains("%26") && url.len() < 7200 && !url.contains('\n'));
    assert_eq!(report, original);
    crate::i18n::set_language("en-US");
    let en = issue_url("opaque & ?");
    assert!(en.contains("title=Bug%20report&body=Please%20describe"));
    assert!(en.contains("opaque%20%26%20%3F"));
}
