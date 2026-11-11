use super::*;

#[test]
fn real_step_and_machine_display_preserves_raw_labels_steps_paths_and_stoppability() {
    let cases = [
        (Step::Checking, "Beide Rechner werden geprüft…"),
        (Step::Cloning, "Das Projekt wird dort geklont…"),
        (
            Step::Snapshot,
            "Eine Momentaufnahme der Arbeit wird erstellt…",
        ),
        (Step::Bundling, "Die Commits werden gebündelt…"),
        (Step::Confirm(1536), "Das Bundle ist 1,5 KB groß."),
        (
            Step::Sending {
                sent: 1536,
                total: 4096,
            },
            "1,5 KB von 4,0 KB werden gesendet…",
        ),
        (Step::Landing, "Wird entpackt und ausgecheckt…"),
        (Step::CleaningUp, "Wird aufgeräumt…"),
    ];
    for (step, expected) in cases {
        let raw = step.label();
        let stoppable = step.stoppable();
        crate::i18n::set_language("de");
        assert_eq!(step.display_label(), expected);
        assert_eq!(step.label(), raw);
        assert_eq!(step.stoppable(), stoppable);
        crate::i18n::set_language("en-US");
        assert_eq!(step.display_label(), raw);
    }
    let here = Machine::Here;
    crate::i18n::set_language("de");
    assert_eq!(here.display_label(), "dieser Rechner");
    assert_eq!(here.label(), "this machine");
    assert_eq!(
        here.path("/opaque/日本語"),
        std::path::PathBuf::from("/opaque/日本語")
    );
    let there = Machine::There(crate::remote::Host::Ssh("me@example.invalid".into()));
    let path = there.path("~/literal 日本語");
    assert_eq!(there.display_label(), there.label());
    assert_eq!(there.path("~/literal 日本語"), path);
    assert_eq!(size(512), "512 B");
    crate::i18n::set_language("en-US");
}
