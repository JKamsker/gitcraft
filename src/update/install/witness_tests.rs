use super::*;

#[test]
fn actual_download_guard_and_copy_status_combinator_capture_before_placement_wrapping() {
    let url = "https://example/1234.5/ref/main; ;";
    assert!(check_download_size(url, MAX_DOWNLOAD).is_ok());
    let large = check_download_size(url, MAX_DOWNLOAD + 1).expect_err("size guard");
    let lookalike = download_failure(url, DownloadFailure::Opaque("bigger than expected".into()));
    assert_eq!(large.raw(), lookalike.raw());
    assert!(matches!(
        large,
        Failure::Download {
            failure: DownloadFailure::TooBig { .. },
            ..
        }
    ));
    let mut calls = 0;
    assert!(check_copy_status(true, || calls += 1).is_ok());
    assert_eq!(calls, 0);
    let copy = check_copy_status(false, || calls += 1).expect_err("copy status");
    assert_eq!(calls, 1);
    let raw = copy.raw().to_owned();
    let placed = previous::placement_failure(copy);
    let opaque = previous::placement_failure(PlacementFailure::Opaque(raw));
    assert_eq!(placed.raw(), opaque.raw());
    let before = placed.clone();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        assert_eq!(large.display() == large.raw(), locale == "en-US");
        assert!(large.display().contains(url));
        assert!(lookalike.display().ends_with("bigger than expected"));
        assert!(opaque.display().ends_with("Could not copy the new version"));
        assert_eq!(
            placed.display(),
            if locale == "de" {
                "Die neue Version konnte nicht an ihren Platz verschoben werden: Die neue Version konnte nicht kopiert werden"
            } else {
                placed.raw()
            }
        );
        assert_eq!(placed, before);
        assert_eq!(placed.clone().into_raw(), before.raw());
    }
}

#[test]
fn actual_install_guard_and_unpack_failure_factories_retain_source_fields() {
    let failures = [
        not_updated(),
        no_build(),
        no_download_folder(),
        asset_missing("gitcraft-v1234.5; ;.zip".into()),
        unpack_failure("bigger than expected; ; /1234.5".into()),
    ];
    let snapshots = failures.clone();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        for (failure, before) in failures.iter().zip(&snapshots) {
            assert_eq!(failure.display() == failure.raw(), locale == "en-US");
            assert_eq!(failure, before);
            assert_eq!(failure.clone().into_raw(), before.raw());
        }
        assert!(failures[3].display().contains("gitcraft-v1234.5; ;.zip"));
        assert!(
            failures[4]
                .display()
                .ends_with("bigger than expected; ; /1234.5")
        );
    }
}
