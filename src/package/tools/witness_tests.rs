use super::*;

#[test]
fn actual_parser_capture_and_ordered_aggregation_repaint_without_raw_or_success_changes() {
    let metadata = parse_cargo_metadata_witnessed("{", "").expect_err("JSON error");
    let raw_metadata = parse_cargo_metadata("{", "").expect_err("raw JSON error");
    assert_eq!(metadata.raw(), raw_metadata);
    let retained = finish_packages(
        vec![],
        Some(metadata),
        Some(PackageJsonFailure::new(
            "cargo failed; package.json: detail; detail".into(),
        )),
    )
    .expect_err("aggregate");
    let npm = parse_npm_pack_witnessed("notice\n[broken").expect_err("JSON error");
    let no_package = parse_npm_pack_witnessed("npm said no package; ;").expect_err("no array");
    let raw_npm = parse_npm_pack("notice\n[broken").expect_err("raw JSON error");
    let raw_no_package = parse_npm_pack("npm said no package; ;").expect_err("raw no array");
    assert_eq!(npm.raw(), raw_npm);
    assert_eq!(no_package.raw(), raw_no_package);
    assert!(matches!(npm, Failure::NpmPackAnswer { .. }));
    assert!(matches!(no_package, Failure::NpmNoPackage { .. }));
    let json = r#"[{"files":[{"path":"/1234.5/ref/main","size":1234}]}]"#;
    assert_eq!(
        parse_npm_pack_witnessed(json).expect("pure parse"),
        parse_npm_pack(json).expect("raw parse")
    );
    let snapshot = retained.clone();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        assert_eq!(retained.display(), retained.raw()); // command prefix + opaque parser/I/O text
        assert_eq!(npm.display() == npm.raw(), locale == "en-US");
        assert_eq!(no_package.display() == no_package.raw(), locale == "en-US");
        assert_eq!(retained, snapshot);
        assert_eq!(retained.clone().into_raw(), snapshot.raw());
    }
    // Real source aggregation suppresses errors when it found a package, as before.
    let found = Package {
        ecosystem: Ecosystem::Npm,
        name: "1234.5".into(),
        version: "1234.5".into(),
        dir: "ref/main".into(),
        registry: None,
    };
    assert_eq!(
        finish_packages(
            vec![found.clone()],
            None,
            Some(PackageJsonFailure::new("opaque".into()))
        )
        .expect("partial success"),
        vec![found]
    );
}

#[test]
fn actual_registry_parser_and_network_error_combinators_keep_entire_details_opaque() {
    let url = "https://example/1234.5; ;/main";
    let json = parse_registry_json(url, "[").expect_err("pure JSON parser");
    let detail = "npm said no package; couldn't ask https://x; ;\n/1234.5";
    let ask = ask_registry_failure(url, detail.into());
    let download = download_failure(url, detail.into());
    assert_eq!(ask.raw(), format!("couldn't ask {url}: {detail}"));
    assert_eq!(download.raw(), format!("couldn't download {url}: {detail}"));
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        for failure in [&json, &ask, &download] {
            assert!(failure.display().contains(url));
            assert_eq!(failure.display() == failure.raw(), locale == "en-US");
            assert_eq!(failure.clone().into_raw(), failure.raw());
        }
        assert!(ask.display().ends_with(detail));
        assert!(download.display().ends_with(detail));
    }
}
