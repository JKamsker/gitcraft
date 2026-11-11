use super::*;

#[test]
fn fluent_partition_outbox_actual_summary_keeps_supplied_state_opaque() {
    let paused = logic::Status::Paused("Provider detail / refs/heads/ß".into());
    for (language, ready, count, paused_label) in [
        ("en-US", "Ready", "1,234 pinned endpoints", "Paused:"),
        ("de", "Bereit", "1.234 festgelegte Endpunkte", "Pausiert:"),
    ] {
        crate::i18n::set_language(language);
        let summary = outbox_entry_summary(&logic::Status::Ready, 1234, true);
        assert!(summary.starts_with(ready));
        assert!(summary.contains(count));
        let summary = outbox_entry_summary(&paused, 1, false);
        assert!(summary.starts_with(paused_label));
        assert!(summary.contains("Provider detail / refs/heads/ß"));
        assert!(summary.contains(if language == "de" {
            "ein festgelegter Endpunkt"
        } else {
            "one pinned endpoint"
        }));
        assert_eq!(
            paused,
            logic::Status::Paused("Provider detail / refs/heads/ß".into())
        );
        assert!(
            outbox_entry_summary(&logic::Status::Sending, 0, false).starts_with(
                if language == "de" {
                    "Wird gesendet"
                } else {
                    "Sending"
                }
            )
        );
        assert!(
            outbox_entry_summary(&logic::Status::Sent, 0, false).starts_with(if language == "de" {
                "Gesendet"
            } else {
                "Sent"
            })
        );
    }
    crate::i18n::set_language("en-US");
}
