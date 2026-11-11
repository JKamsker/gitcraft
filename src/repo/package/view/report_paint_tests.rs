use super::*;

#[test]
fn retained_package_report_repaints_known_reasons_and_hint_without_changing_raw_data() {
    let report = Report {
        files: vec![("opaque/on, auf/12345.env".into(), Some(1536 * 1024))],
        accidental: vec![Note {
            path: "opaque/on, auf/12345.env".into(),
            why: "big for a package (1.5 MB)".into(),
        }, Note {
            path: "unknown/12345".into(),
            why: "provider text; package.json has a prepack script".into(),
        }],
        missing: vec![],
        hint: Some("package.json has a prepack script, which npm publish runs first and this listing doesn't: files it builds may show as left out or gone until it has run".into()),
    };
    let original = report.clone();
    let ctx = egui::Context::default();
    for (locale, reason) in [
        ("en-US", "big for a package (1.5 MB)"),
        ("de", "groß für ein Paket (1,5 MB)"),
        ("en-US", "big for a package (1.5 MB)"),
    ] {
        crate::i18n::set_language(locale);
        let shown = crate::repo::push_preflight::core_guard_paint_tests::paint(&ctx, |ui| {
            report_ui(ui, &report, Some(&Ok(None)), "registry-12345");
        });
        assert!(shown.contains(reason), "{shown}");
        let hint = if locale == "de" {
            "package.json enthält ein prepack-Skript"
        } else {
            "package.json has a prepack script"
        };
        assert!(shown.contains(hint), "{shown}");
        assert!(shown.contains("opaque/on, auf/12345.env"), "{shown}");
        assert!(
            shown.contains("provider text; package.json has a prepack script"),
            "{shown}"
        );
        assert_eq!(report, original);
    }
    crate::i18n::set_language("en-US");
}
