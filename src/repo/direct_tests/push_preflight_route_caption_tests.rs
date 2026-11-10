use super::core_guard_paint_tests::paint;
use super::*;

#[test]
fn actual_checklist_route_is_a_whole_sentence_with_opaque_route_fields() {
    let route = crate::git::repository_tools::routes::PushRoute {
        branch: "topic-12345".into(),
        commit: "a".repeat(40),
        remote: "remote-12345".into(),
        destination: "refs/heads/release-12345".into(),
        urls: vec!["[sensitive path]".into()],
        lease: Some("b".repeat(40)),
    };
    let mut plan = check::selected_continuation_recording::plan(&route);
    plan.targets = vec![
        "Local repository: /opaque/on, auf/12345".into(),
        "ssh://user@example.invalid/path-on-12345".into(),
    ];
    let original_ops = plan.again();
    let original_targets = plan.targets.clone();
    let original_branch = plan.branch.clone();
    let original_destination = plan.destination.clone();
    let ctx = egui::Context::default();
    for (locale, connecting, local) in [
        ("en-US", "on", "Local repository"),
        ("de", "auf", "Lokales Repository"),
        ("en-US", "on", "Local repository"),
    ] {
        crate::i18n::set_language(locale);
        let expected = format!(
            "{} → {} {connecting} {local}: /opaque/on, auf/12345, ssh://user@example.invalid/path-on-12345",
            original_branch, original_destination,
        );
        assert_eq!(route_caption(&plan), expected);
        let mut acknowledge = false;
        let shown = paint(&ctx, |ui| {
            assert!(matches!(
                dialog(ui, &mut plan, &mut acknowledge),
                Outcome::Open
            ));
        });
        assert!(shown.contains(&expected), "{shown}");
        assert!(!acknowledge);
        assert_eq!(plan.again(), original_ops);
        assert_eq!(plan.targets, original_targets);
        assert_eq!(plan.branch, original_branch);
        assert_eq!(plan.destination, original_destination);
    }
    // The supplied delete state chooses the existing localized deletion caption;
    // no ref, URL, or path is inferred from that caption.
    plan.deleting = true;
    crate::i18n::set_language("de");
    assert!(route_caption(&plan).starts_with(crate::i18n::text("repo-core-3134")));
    assert_eq!(plan.targets, original_targets);
    crate::i18n::set_language("en-US");
}
