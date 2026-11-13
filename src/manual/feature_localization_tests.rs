use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use fluent_bundle::{FluentResource, concurrent::FluentBundle};
use std::collections::BTreeSet;

const CATALOGS: [(&str, &str); 2] = [
    (
        "en-US",
        include_str!("../../locales/en-US/interaction-manual-features.ftl"),
    ),
    (
        "de",
        include_str!("../../locales/de/interaction-manual-features.ftl"),
    ),
];

#[test]
fn every_feature_has_a_paired_whole_message_without_missing_variables() -> Result<(), String> {
    let expected: BTreeSet<_> = features()
        .iter()
        .flat_map(|(title, points)| std::iter::once(*title).chain(points.iter().copied()))
        .collect();
    assert_eq!(features().len(), 25);
    assert_eq!(features().iter().map(|(_, p)| p.len()).sum::<usize>(), 220);
    assert_eq!(expected.len(), 245);
    let mut values = Vec::new();
    for (locale, source) in CATALOGS {
        let resource = FluentResource::try_new(source.to_owned())
            .map_err(|(_, errors)| format!("{locale}: {errors:?}"))?;
        let ids: Vec<_> = source
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(id, _)| id))
            .collect();
        let actual: BTreeSet<_> = ids.iter().copied().collect();
        assert_eq!(ids.len(), actual.len(), "duplicate {locale} feature ID");
        assert_eq!(actual, expected, "{locale} feature coverage");
        let mut bundle = FluentBundle::new_concurrent(vec![
            locale.parse().map_err(|e| format!("{locale}: {e}"))?,
        ]);
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .map_err(|e| format!("{locale}: {e:?}"))?;
        let mut translated = Vec::new();
        for id in &expected {
            let message = bundle.get_message(id).ok_or_else(|| id.to_string())?;
            assert!(message.attributes().next().is_none());
            let pattern = message.value().ok_or_else(|| id.to_string())?;
            let mut errors = Vec::new();
            let value = bundle
                .format_pattern(pattern, None, &mut errors)
                .into_owned();
            // Every feature is a complete static message. Empty arguments must render
            // without errors in both catalogs, including literal shell $VARIABLES.
            assert!(errors.is_empty(), "{locale}/{id}: {errors:?}");
            assert!(!value.is_empty());
            crate::i18n::set_language(locale);
            assert_eq!(crate::i18n::text(id), value, "embedded {locale}/{id}");
            translated.push(value);
        }
        values.push(translated);
    }
    for ((id, en), de) in expected.iter().zip(&values[0]).zip(&values[1]) {
        if *id != "interaction-manual-feature-gerrit-title"
            && *id != "interaction-manual-feature-graph-title"
            && *id != "interaction-manual-feature-bisect-title"
            && *id != "interaction-manual-feature-updates-title"
        {
            assert_ne!(en, de, "untranslated feature {id}");
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}

#[test]
fn actual_manual_feature_labels_render_in_selected_german_and_english() {
    // Only the real manual's immediate-mode UI; no app, repo, reader or native setup.
    let fonts = std::cell::Cell::new(false);
    crate::i18n::set_language("de");
    let mut h = Harness::builder()
        .with_size(egui::vec2(680.0, 100_000.0))
        .build_ui(|ui| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            super::ui(ui);
        });
    h.run();
    h.get_by_label("REPOSITORYS UND TABS");
    h.get_by_label("PAKETE");
    for (_, points) in features() {
        for point in points {
            h.get_by_label(&crate::i18n::text(point).replace("Ctrl", command()));
        }
    }
    h.get_by_label(
        "Ein Tab pro Repository. Änderungen aus Editor oder Terminal erscheinen von selbst.",
    );
    h.get_by_label("Ein Terminalbereich mit Tabs im Ordner des Repositorys.");
    let german_command = crate::i18n::text("interaction-manual-feature-terminal-file-manager-01")
        .replace("Ctrl", command());
    h.get_by_label(&german_command);
    // Accessible label queries observe the selected prose, never the stable IDs.
    assert!(h.query_by_label("One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.").is_none());
    crate::i18n::set_language("en-US");
    h.run();
    h.get_by_label("REPOSITORIES AND TABS");
    h.get_by_label("PACKAGES");
    for (_, points) in features() {
        for point in points {
            h.get_by_label(&crate::i18n::text(point).replace("Ctrl", command()));
        }
    }
    h.get_by_label(
        "One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.",
    );
    h.get_by_label("A terminal panel with tabs, in the repository's folder.");
    let english_command = crate::i18n::text("interaction-manual-feature-terminal-file-manager-01")
        .replace("Ctrl", command());
    h.get_by_label(&english_command);
    assert!(
        h.query_by_label("Ein Terminalbereich mit Tabs im Ordner des Repositorys.")
            .is_none()
    );
}
