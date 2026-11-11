use super::*;
#[test]
fn fluent_partition_test_lane_actual_result_display_keeps_raw_status_identity() {
    let summary = RunSummary {
        good: 1234,
        bad: 1,
        known: 2,
        failed: vec![("opaque tree".into(), "provider failure".into())],
        stopped: true,
        cleanup: Some("Opaque cleanup / ß".into()),
    };
    let result = Ok(summary);
    crate::i18n::set_language("en-US");
    let raw = ended_message("refs/heads/ß..HEAD", &result);
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        assert_eq!(ended_message("refs/heads/ß..HEAD", &result), raw);
        let (display, bad) = display_ended_message("refs/heads/ß..HEAD", &result);
        assert!(bad);
        assert!(display.contains("refs/heads/ß..HEAD"));
        assert!(display.contains("Opaque cleanup / ß"));
        assert!(display.contains(if language == "de" {
            "1.234 bestanden"
        } else {
            "1,234 passed"
        }));
        assert!(display.contains(if language == "de" {
            "1.233 Commits"
        } else {
            "1,233 commits"
        }));
        let one = Ok(RunSummary {
            good: 1,
            ..Default::default()
        });
        let shown = display_ended_message("range/ß", &one);
        assert!(!shown.1);
        assert!(shown.0.contains(if language == "de" {
            "Ein Commit"
        } else {
            "one commit"
        }));
        assert_eq!(
            ended_message("range/ß", &one).0,
            "Tested 1 commit of range/ß: 1 passed"
        );
        let failure = Err("Opaque error refs/heads/ß".into());
        assert_eq!(
            ended_message("range", &failure),
            (
                "Couldn't test range: Opaque error refs/heads/ß".into(),
                true
            )
        );
        assert!(
            display_ended_message("range", &failure)
                .0
                .contains("Opaque error refs/heads/ß")
        );
    }
    crate::i18n::set_language("en-US");
}
