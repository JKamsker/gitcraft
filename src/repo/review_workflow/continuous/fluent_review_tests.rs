use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_continuous_actual_metadata_button_preserves_diff_and_pick_identity()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let key = DiffKey::Work(git::StatusFile {
        path: "new {path}.rs".into(),
        old_path: Some("old {path}.rs".into()),
        code: 'R',
        area: Area::Staged,
    });
    let mut data = diff::DiffData::default();
    data.header = "rename from old {path}.rs\nrename to new {path}.rs".into();
    let mut acts = vec![];
    for (locale, status, label) in [
        ("en-US", "Status R · Staged", "Open file diff"),
        ("de", "Status R · Im Index", "Datei-Diff öffnen"),
    ] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| {
            file_intro(ui, &key, &data, &mut acts);
        });
        assert!(text.contains(status) && text.contains(label), "{text}");
        assert!(
            text.contains(&data.header) && text.contains("old {path}.rs → new {path}.rs"),
            "{text}"
        );
    }
    click(&ctx, "Datei-Diff öffnen", |ui| {
        file_intro(ui, &key, &data, &mut acts);
    })?;
    assert!(matches!(acts.as_slice(), [Action::OpenDiff(actual)] if actual == &key));
    assert_eq!(diff::Pick::Stage.verb(), "Stage");
    assert_eq!(
        crate::repo::messages::display_operation(diff::Pick::Stage.verb()),
        "Vormerken"
    );
    assert_eq!(picks(&key, true, false), &[diff::Pick::Unstage]);
    Ok(())
}
