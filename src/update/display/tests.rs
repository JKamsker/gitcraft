use super::*;
use std::path::PathBuf;

#[test]
fn actual_previous_and_channel_values_resolve_at_paint_without_changing_raw_labels_or_state() {
    let unknown = Previous {
        version: None,
        path: PathBuf::from("/1234.5/gitcraft.previous"),
    };
    let known = Previous {
        version: Some("1234.5-dev.6789+build.1234".into()),
        path: unknown.path.clone(),
    };
    let unknown_before = unknown.clone();
    let known_before = known.clone();
    // Retained schema can be displayed again after switching, without caching a translation.
    let rollback = Diagnostic::RestorePrevious {
        previous: (&unknown).into(),
        detail: "No previous version of gitcraft is kept",
    };
    for (locale, kept, main, dev) in [
        (
            "en-US",
            "the version kept",
            "Main (releases)",
            "Dev (a build of every change)",
        ),
        (
            "de",
            "die aufbewahrte Version",
            "Main (Veröffentlichungen)",
            "Dev (ein Build für jede Änderung)",
        ),
    ] {
        crate::i18n::set_language(locale);
        assert_eq!(unknown.display_label(), kept);
        assert_eq!(PreviousLabel::from(&unknown).display(), kept);
        assert_eq!(known.display_label(), "1234.5-dev.6789+build.1234");
        assert_eq!(PreviousLabel::from(&known).display(), known.label());
        assert_eq!(Channel::Main.display_label(), main);
        assert_eq!(Channel::Dev.display_label(), dev);
        let shown = rollback.display();
        assert!(shown.contains(kept));
        assert!(shown.ends_with("No previous version of gitcraft is kept"));
        assert_eq!(unknown.label(), "the version kept");
        assert_eq!(Channel::Main.label(), "Main (releases)");
        assert_eq!(Channel::Dev.label(), "Dev (a build of every change)");
        assert_eq!(unknown, unknown_before);
        assert_eq!(known, known_before);
        assert_eq!(super::super::PIN_THIS, "this version");
    }
    // Even a label-like value in a version field is data, not a translated fallback.
    let lookalike = Previous {
        version: Some("the version kept".into()),
        path: PathBuf::new(),
    };
    assert_eq!(lookalike.display_label(), "the version kept");
    crate::i18n::set_language("en-US");
}

#[test]
fn finite_nested_update_messages_require_source_provenance_and_keep_lookalikes_opaque() {
    let url = "https://example/1234.5/ref/main";
    let old = "/1234.5/old; rename back";
    let target = "/1234.5/current";
    let detail = "Could not copy the new version";
    let known_download = Diagnostic::Download {
        url,
        failure: DownloadFailure::TooBig,
    };
    let unknown_download = Diagnostic::Download {
        url,
        failure: DownloadFailure::Opaque("bigger than expected"),
    };
    let known_placement = Diagnostic::PutNew {
        failure: PlacementFailure::CopyFailed,
    };
    let unknown_placement = Diagnostic::PutNew {
        failure: PlacementFailure::Opaque(detail),
    };
    for (locale, download, put, kept) in [
        (
            "en-US",
            "Could not download",
            "Could not put the new version in place",
            "the version kept",
        ),
        (
            "de",
            "konnte nicht heruntergeladen werden",
            "Die neue Version konnte nicht an ihren Platz verschoben werden",
            "die aufbewahrte Version",
        ),
    ] {
        crate::i18n::set_language(locale);
        let outer_download = if locale == "de" {
            format!("{url} {download}")
        } else {
            format!("{download} {url}")
        };
        assert_eq!(
            unknown_download.display(),
            format!("{outer_download}: bigger than expected")
        );
        let big = if locale == "de" {
            "größer als erwartet"
        } else {
            "bigger than expected"
        };
        assert_eq!(known_download.display(), format!("{outer_download}: {big}"));
        assert_eq!(unknown_placement.display(), format!("{put}: {detail}"));
        let copy = if locale == "de" {
            "Die neue Version konnte nicht kopiert werden"
        } else {
            detail
        };
        assert_eq!(known_placement.display(), format!("{put}: {copy}"));
        let restore = Diagnostic::RestoreBoth {
            previous: PreviousLabel::Kept,
            detail,
            old,
            target,
        }
        .display();
        for opaque in [detail, old, target] {
            assert!(restore.contains(opaque));
        }
        assert!(restore.contains(kept));
        let restore = Diagnostic::RestorePrevious {
            previous: PreviousLabel::Version("1234.5-dev.6789"),
            detail: "the version kept",
        }
        .display();
        assert!(restore.contains("1234.5-dev.6789"));
        assert!(restore.ends_with("the version kept"));
        for raw in [
            "This copy of gitcraft isn't updated",
            "bigger than expected",
            "Could not copy the new version",
            "No previous version of gitcraft is kept",
            "Could not put the new version in place: Could not copy the new version",
            "arbitrary\nfilesystem: 1234.5",
        ] {
            assert_eq!(Diagnostic::Opaque(raw).display(), raw);
        }
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn finite_update_diagnostic_catalog_contracts_match_raw_source_templates() {
    // Do not call installation/download/rollback functions to manufacture errors. These
    // branch contracts are bound to the actual source and must also be reviewed at integration.
    let install = include_str!("../install.rs");
    let previous = include_str!("../previous.rs");
    let cases = [
        (
            Diagnostic::NotUpdated,
            "This copy of gitcraft isn't updated",
            "Diese Kopie von gitcraft wird nicht aktualisiert",
        ),
        (
            Diagnostic::NoBuild,
            "The releases have no build for this system",
            "Die Veröffentlichungen enthalten keinen Build für dieses System",
        ),
        (
            Diagnostic::NoDownloadFolder,
            "No folder to download the update into",
            "Kein Ordner zum Herunterladen des Updates vorhanden",
        ),
        (
            Diagnostic::CopyNew,
            "Could not copy the new version",
            "Die neue Version konnte nicht kopiert werden",
        ),
        (
            Diagnostic::NoPrevious,
            "No previous version of gitcraft is kept",
            "Es wurde keine vorherige Version von gitcraft aufbewahrt",
        ),
    ];
    for (diagnostic, english, german) in cases {
        assert!(install.contains(english) || previous.contains(english));
        crate::i18n::set_language("en-US");
        assert_eq!(diagnostic.display(), english);
        crate::i18n::set_language("de");
        assert_eq!(diagnostic.display(), german);
    }
    for (diagnostic, source, template, english, german) in [
        (
            Diagnostic::AssetMissing {
                asset: "v1234.5.zip",
            },
            install,
            "{asset} holds no gitcraft",
            "v1234.5.zip holds no gitcraft",
            "v1234.5.zip enthält kein gitcraft",
        ),
        (
            Diagnostic::Unpack {
                detail: "bigger than expected",
            },
            install,
            "Could not unpack the update: {e}",
            "Could not unpack the update: bigger than expected",
            "Das Update konnte nicht entpackt werden: bigger than expected",
        ),
        (
            Diagnostic::MoveAside {
                target: "/1234.5/main",
                detail: "a damaged archive",
            },
            previous,
            "Could not move {} aside: {e}",
            "Could not move /1234.5/main aside: a damaged archive",
            "/1234.5/main konnte nicht beiseite verschoben werden: a damaged archive",
        ),
    ] {
        assert!(source.contains(template));
        crate::i18n::set_language("en-US");
        assert_eq!(diagnostic.display(), english);
        crate::i18n::set_language("de");
        assert_eq!(diagnostic.display(), german);
    }
    crate::i18n::set_language("en-US");
}
