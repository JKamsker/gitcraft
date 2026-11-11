use super::*;

#[test]
fn actual_operation_names_and_cancel_identity_remain_raw_while_known_errors_translate() {
    crate::i18n::set_language("de");
    for op in [
        crate::git::InProgress::Merge,
        crate::git::InProgress::Rebase,
        crate::git::InProgress::CherryPick,
        crate::git::InProgress::Revert,
        crate::git::InProgress::Am,
    ] {
        let raw = format!(
            "A {} is in progress here: finish or abort it first.",
            op.raw_label().to_lowercase()
        );
        assert_ne!(diagnostic(&raw), raw);
        assert_eq!(
            op.command(),
            match op {
                crate::git::InProgress::Merge => "merge",
                crate::git::InProgress::Rebase => "rebase",
                crate::git::InProgress::CherryPick => "cherry-pick",
                crate::git::InProgress::Revert => "revert",
                crate::git::InProgress::Am => "am",
            }
        );
    }
    let failed = super::super::Failed {
        message: super::super::CANCELLED.into(),
        cancelled: true,
    };
    assert_eq!(
        diagnostic(&failed.message),
        "Teleportieren abgebrochen: Es wurde nichts geändert."
    );
    assert!(failed.cancelled);
    assert_eq!(failed.message, super::super::CANCELLED);
    assert_eq!(
        diagnostic("Teleport failed: Some files have conflicts: resolve them first."),
        "Teleportieren fehlgeschlagen: Einige Dateien enthalten Konflikte: Löse diese zuerst."
    );
    for raw in [
        "Unknown Git diagnostic 日本語",
        "A future operation is in progress here: finish or abort it first.",
        "The bundle that arrived on external fake machine didn't hold the work.",
    ] {
        assert_eq!(diagnostic(raw), raw);
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn known_destination_templates_preserve_supplied_path_ref_machine_and_external_details() {
    crate::i18n::set_language("de");
    let cases = [
        (
            "Give the clone as a full path on SSH · me@example.invalid: /home/me/x or ~/x.",
            "Gib für den Klon einen absoluten Pfad an. Ziel: SSH · me@example.invalid: /home/me/x oder ~/x.",
        ),
        (
            "/opaque/path on literal 日本語 is already there on SSH · build: pick another folder.",
            "/opaque/path on literal 日本語 ist am Ziel bereits vorhanden. Ziel: SSH · build. Wähle einen anderen Ordner.",
        ),
        (
            "topic-ü is checked out on this machine already, in /opaque/path on literal 日本語: pick another branch name.",
            "topic-ü ist am Ziel bereits in /opaque/path on literal 日本語 ausgecheckt. Ziel: dieser Rechner. Wähle einen anderen Branch-Namen.",
        ),
        (
            "The repository on WSL · Ubuntu-24.04 uses sha256 hashes and this one sha1: their commits can't be mixed.",
            "Das Repository am Ziel verwendet sha256-Hashes, dieses hier sha1-Hashes; ihre Commits können nicht kombiniert werden. Ziel: WSL · Ubuntu-24.04.",
        ),
        (
            "Only 1.5 KB of the bundle's 4.0 KB arrived on this machine.",
            "Am Ziel sind nur 1,5 KB von 4,0 KB des Bundles angekommen. Ziel: dieser Rechner.",
        ),
        (
            "could not remove /opaque/path: exit status 7",
            "Entfernen fehlgeschlagen: /opaque/path: Exit-Status 7",
        ),
        (
            "could not copy /opaque/path: the copy is on another machine",
            "Kopieren fehlgeschlagen: /opaque/path: die Kopie befindet sich auf einem anderen Rechner",
        ),
        (
            "could not remove /opaque/path on literal: External Git detail { $count }\nsecond line",
            "Entfernen fehlgeschlagen: /opaque/path on literal: External Git detail { $count }\nsecond line",
        ),
    ];
    for (raw, expected) in cases {
        assert_eq!(diagnostic(raw), expected);
    }
    for raw in [
        "Unknown native diagnostic 日本語",
        "Only External of the bundle's 4.0 KB arrived on this machine.",
        "Give the clone as a full path on unrecognized host: /home/me/x or ~/x.",
    ] {
        assert_eq!(diagnostic(raw), raw);
    }
    crate::i18n::set_language("en-US");
    for (raw, _) in cases {
        assert_eq!(diagnostic(raw), raw);
    }
}
