use super::*;
use crate::package::{Ecosystem, Failure, Package};
use egui_kittest::{Harness, kittest::Queryable};

fn package() -> Package {
    Package {
        ecosystem: Ecosystem::Npm,
        name: "raw; {$name}".into(),
        version: "1234.5".into(),
        dir: "opaque/path\nmain".into(),
        registry: None,
    }
}

#[test]
fn fluent_partition_actual_package_three_cached_failure_slots_repaint_without_request_or_action_changes()
-> Result<(), String> {
    let detail =
        "provider: No Cargo.toml or package.json at the top of the repository; ;\n{$raw} 日本語";
    let failures = [
        Failure::NoManifest {
            raw: "No Cargo.toml or package.json at the top of the repository".into(),
        },
        Failure::NpmPackAnswer {
            raw: format!("npm pack answered: {detail}"),
            detail: detail.into(),
        },
        Failure::AskRegistry {
            raw: format!("registry lookup: {detail}"),
            url: "https://example/1234;raw".into(),
            detail: detail.into(),
        },
    ];
    let report = Report { files: vec![("raw.env".into(), Some(1536 * 1024))], accidental: vec![Note { path: "raw.env".into(), why: "big for a package (1.5 MB)".into() }], missing: vec![], hint: Some("package.json has a prepack script, which npm publish runs first and this listing doesn't: files it builds may show as left out or gone until it has run".into()) };
    let original = report.clone();
    for (slot, failure) in failures.into_iter().enumerate() {
        let mut installed = false;
        let view = PackageView {
            packages: Some(if slot == 0 {
                Err(failure.clone())
            } else {
                Ok(vec![package()])
            }),
            report: Some(if slot == 1 {
                Err(failure.clone())
            } else {
                Ok(report.clone())
            }),
            published: Some(Err(failure.clone())),
            selected: 0,
            request: 1234,
        };
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (PackageView, Vec<Action>)| {
                if !installed {
                    crate::theme::install_fonts(ui.ctx());
                    installed = true;
                    ui.ctx().request_discard("supplied fonts");
                    return;
                }
                contents(ui, &state.0, 1400.0, &mut state.1);
            },
            (view, Vec::new()),
        );
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            h.run();
            let id = [
                "repo-workflows-package-contents-e237505e",
                "repo-workflows-package-contents-89bee303",
                "repo-workflows-package-report-ui-74f3a93b",
            ][slot];
            let expected = crate::i18n::message(id, &[("e", failure.display().into())]);
            h.get_by_label(&expected);
            if locale == "de" {
                assert_ne!(failure.display(), failure.raw());
            }
            assert_eq!(h.state().0.request, 1234);
            assert_eq!(h.state().0.selected, 0);
            assert!(h.state().1.is_empty());
            let retained = match slot {
                0 => h
                    .state()
                    .0
                    .packages
                    .as_ref()
                    .ok_or("missing test value")?
                    .as_ref()
                    .err()
                    .ok_or("expected an error")?,
                1 => h
                    .state()
                    .0
                    .report
                    .as_ref()
                    .ok_or("missing test value")?
                    .as_ref()
                    .err()
                    .ok_or("expected an error")?,
                _ => h
                    .state()
                    .0
                    .published
                    .as_ref()
                    .ok_or("missing test value")?
                    .as_ref()
                    .err()
                    .ok_or("expected an error")?,
            };
            assert_eq!(retained, &failure);
            assert_eq!(retained.clone().into_raw(), failure.raw());
            if slot == 2 {
                let r = h
                    .state()
                    .0
                    .report
                    .as_ref()
                    .ok_or("missing test value")?
                    .as_ref()
                    .map_err(|error| format!("{error:?}"))?;
                h.get_by_label(&format!("raw.env — {}", r.display_why(&r.accidental[0])));
                h.get_by_label(&r.display_hint().ok_or("missing test value")?);
                assert_eq!(r, &original);
            }
        }
        h.get_by_label(crate::i18n::text("repo-workflows-package-buttons-0e916101"))
            .click();
        h.run();
        assert!(matches!(
            h.state().1.as_slice(),
            [Action::Package(PackageAct::Refresh)]
        ));
        assert_eq!(h.state().0.request, 1234);
    }
    let opaque =
        Failure::Opaque("No Cargo.toml or package.json at the top of the repository".into());
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        assert_eq!(opaque.display(), opaque.raw());
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
