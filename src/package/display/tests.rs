use super::*;
use crate::package::check;

fn report(
    files: Vec<(String, Option<u64>)>,
    tracked: &[String],
    manifest: &str,
    eco: Ecosystem,
) -> Report {
    Report {
        missing: check::missing(eco, &files, tracked, manifest),
        accidental: check::accidental(eco, &files, tracked),
        hint: check::hint(eco, manifest),
        files,
    }
}

#[test]
fn actual_note_and_hint_data_resolve_again_after_locale_change_without_raw_mutation() {
    crate::i18n::set_language("en-US");
    let paths = [
        ".env",
        ".npmrc",
        "id_rsa",
        "target/output",
        ".vscode/config",
        "app.log",
        "app.zip",
        "db.sqlite",
        "leftover",
        "big.bin",
    ];
    let files: Vec<_> = paths
        .iter()
        .map(|p| ((*p).to_owned(), (*p == "big.bin").then_some(1536 * 1024)))
        .collect();
    let tracked: Vec<_> = paths
        .iter()
        .filter(|p| **p != "leftover")
        .map(|p| (*p).to_owned())
        .collect();
    let cargo = report(files, &tracked, "", Ecosystem::Cargo);
    let raw = cargo.clone();
    for (locale, reasons) in [
        (
            "en-US",
            [
                "an environment file: often secrets",
                "a credentials file: may hold a token",
                "a key or certificate",
                "build output or dependencies",
                "an editor's or tool's leftover",
                "a log",
                "an archive, perhaps an earlier package",
                "a database",
                "git doesn't track it: a leftover of this folder",
                "big for a package (1.5 MB)",
            ],
        ),
        (
            "de",
            [
                "eine Umgebungsdatei: enthält oft Geheimnisse",
                "eine Zugangsdaten-Datei: enthält möglicherweise ein Token",
                "ein Schlüssel oder Zertifikat",
                "Build-Ausgaben oder Abhängigkeiten",
                "Überreste eines Editors oder Werkzeugs",
                "eine Protokolldatei",
                "ein Archiv, möglicherweise ein früheres Paket",
                "eine Datenbank",
                "Git verfolgt die Datei nicht: ein Überrest in diesem Ordner",
                "groß für ein Paket (1,5 MB)",
            ],
        ),
    ] {
        crate::i18n::set_language(locale);
        let shown: Vec<_> = cargo
            .accidental
            .iter()
            .map(|n| cargo.display_why(n))
            .collect();
        assert_eq!(shown, reasons);
        assert_eq!(cargo, raw);
    }
    // The preserved raw producer can also run in German; a later English paint still works.
    crate::i18n::set_language("de");
    let de_data = report(
        vec![("huge.bin".into(), Some(12345 * 1024_u64.pow(4)))],
        &["huge.bin".into()],
        "",
        Ecosystem::Cargo,
    );
    assert_eq!(de_data.accidental[0].why, "big for a package (12.345,0 TB)");
    crate::i18n::set_language("en-US");
    assert_eq!(
        de_data.display_why(&de_data.accidental[0]),
        "big for a package (12,345.0 TB)"
    );
    // All fixed and finite manifest-key branches come from the actual pure producer.
    for (eco, manifest, tracked) in [
        (
            Ecosystem::Cargo,
            "[package]\nreadme = 'docs/1234.5.md'\nlicense-file = 'COPYING.1234.5'",
            vec!["README.md", "LICENSE", "src/1234.5.rs", "build.rs"],
        ),
        (
            Ecosystem::Npm,
            r#"{"main":"1234.5/main", "module":"module", "types":"types", "typings":"typings", "bin":{"x":"bin"}}"#,
            vec![],
        ),
    ] {
        let data = report(
            vec![],
            &tracked.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>(),
            manifest,
            eco,
        );
        let before = data.clone();
        crate::i18n::set_language("en-US");
        for note in &data.missing {
            assert_eq!(data.display_why(note), note.why);
        }
        crate::i18n::set_language("de");
        for note in &data.missing {
            assert_ne!(data.display_why(note), note.why);
        }
        assert_eq!(data, before);
        assert!(data.missing.iter().any(|n| n.path.contains("1234.5")));
    }
    for script in ["prepack", "prepare", "prepublishOnly"] {
        let manifest = format!(r#"{{"scripts":{{"{script}":"echo do not execute"}}}}"#);
        let data = report(vec![], &[], &manifest, Ecosystem::Npm);
        let raw = data.hint.clone();
        crate::i18n::set_language("en-US");
        assert_eq!(data.display_hint(), raw);
        crate::i18n::set_language("de");
        assert!(
            data.display_hint()
                .is_some_and(|s| s.contains(&format!("ein {script}-Skript")))
        );
        assert_eq!(data.hint, raw);
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn unknown_note_hint_and_external_error_lookalikes_remain_opaque() {
    crate::i18n::set_language("de");
    let data = report(vec![], &[], "", Ecosystem::Npm);
    for why in [
        "a log\n",
        "a log: user detail",
        "Cargo.toml names it (arbitrary)",
        "package.json names it (1234.5)",
        "big for a package (1.5 MB)",
        "big for a package (1536 KB)",
    ] {
        let note = Note {
            path: "1234.5/ref/main".into(),
            why: why.into(),
        };
        assert_eq!(data.display_why(&note), why);
    }
    let note = Note {
        path: "x".into(),
        why: "big for a package (1.5 MB)".into(),
    };
    assert_eq!(
        note.display_why(Some(1536 * 1024)),
        "groß für ein Paket (1,5 MB)"
    );
    assert_eq!(note.display_why(Some(2048 * 1024)), note.why);
    for raw in [
        "npm said no package",
        "a damaged archive",
        "couldn't download https://1234.5: bigger than expected",
        "cargo failed; package.json: a log",
    ] {
        assert_eq!(Diagnostic::Opaque(raw).display(), raw);
        assert_eq!(ToolFailure::Opaque(raw).display(), raw);
    }
    let unknown = Report {
        hint: Some("package.json has a custom script, which npm publish runs first".into()),
        ..data
    };
    assert_eq!(unknown.display_hint(), unknown.hint);
    crate::i18n::set_language("en-US");
}

#[test]
fn actual_parser_errors_and_finite_diagnostic_composition_preserve_external_details()
-> Result<(), String> {
    // Invoke only pure JSON parsers. Retain their known branch, never parse their strings.
    for (locale, answer, no_package) in [
        ("en-US", "npm pack's answer: ", "npm said no package"),
        (
            "de",
            "Antwort von npm pack: ",
            "npm hat kein Paket gemeldet",
        ),
    ] {
        crate::i18n::set_language(locale);
        let invalid = "[broken";
        let raw = crate::package::tools::parse_npm_pack(invalid)
            .err()
            .ok_or("invalid JSON")?;
        let detail = serde_json::from_str::<serde_json::Value>(invalid)
            .err()
            .ok_or("invalid JSON")?
            .to_string();
        assert_eq!(raw, format!("npm pack's answer: {detail}"));
        assert_eq!(
            Diagnostic::NpmPackAnswer { detail: &detail }.display(),
            format!("{answer}{detail}")
        );
        assert_eq!(
            crate::package::tools::parse_npm_pack("no JSON")
                .err()
                .ok_or("no JSON")?,
            "npm said no package"
        );
        assert_eq!(Diagnostic::NpmNoPackage.display(), no_package);
        let detail = serde_json::from_str::<serde_json::Value>("{")
            .err()
            .ok_or("invalid JSON")?
            .to_string();
        let raw = crate::package::tools::parse_cargo_metadata("{", "")
            .err()
            .ok_or("invalid metadata")?;
        assert_eq!(Diagnostic::CargoMetadata { detail: &detail }.display(), raw);
        // App-owned child translates only when explicitly marked Tool, external lookalike stays raw.
        let known = Diagnostic::Packages {
            cargo: Some(CargoFailure::Tool(ToolFailure::Failed { program: "cargo" })),
            package_json: Some("npm said no package; /1234.5"),
        }
        .display();
        assert_eq!(
            known,
            if locale == "de" {
                "cargo: cargo ist fehlgeschlagen; package.json: npm said no package; /1234.5"
            } else {
                "cargo: cargo failed; package.json: npm said no package; /1234.5"
            }
        );
        let opaque = Diagnostic::Packages {
            cargo: Some(CargoFailure::Tool(ToolFailure::Opaque("cargo failed"))),
            package_json: None,
        }
        .display();
        assert_eq!(opaque, "cargo: cargo failed");
    }
    crate::i18n::set_language("en-US");
    assert_eq!(
        Diagnostic::ArchiveTooBig.display(),
        crate::package::archive::too_big()
    );
    crate::i18n::set_language("de");
    assert_eq!(
        Diagnostic::ArchiveTooBig.display(),
        "das Archiv ist größer als 200,0 MB: kein Vergleich"
    );
    let url = "https://example/1234.5/ref/main";
    let detail = "a damaged archive; npm said no package\nfilesystem 1234.5";
    assert_eq!(
        Diagnostic::Download { url, detail }.display(),
        format!("{url} konnte nicht heruntergeladen werden: {detail}")
    );
    assert_eq!(
        Diagnostic::OtherRegistry {
            registry: "https://1234.5/main",
            ecosystem: Ecosystem::Cargo
        }
        .display(),
        "Auf https://1234.5/main statt auf crates.io veröffentlicht: kein Vergleich"
    );
    assert_eq!(
        ToolFailure::Timeout {
            program: "tool-1234.5",
            minutes: 1234
        }
        .display(),
        "tool-1234.5 benötigte länger als 1.234 Minuten (wartet es auf eine Sperre oder das Netzwerk?): gestoppt"
    );
    crate::i18n::set_language("en-US");

    Ok(())
}
