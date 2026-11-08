//! Teleport display adapters. Machine/step identities and protocol labels stay unchanged.
use super::{Machine, Step};
use crate::i18n::{message, text};

impl Machine {
    pub(crate) fn display_label(&self) -> String {
        match self {
            Self::Here => text("interaction-teleport-machine").to_owned(),
            Self::There(host) => host.label(),
        }
    }
}

impl Step {
    pub(crate) fn display_label(self) -> String {
        match self {
            Self::Checking => text("interaction-teleport-checking").to_owned(),
            Self::Cloning => text("interaction-teleport-cloning").to_owned(),
            Self::Snapshot => text("interaction-teleport-snapshot").to_owned(),
            Self::Bundling => text("interaction-teleport-bundling").to_owned(),
            Self::Landing => text("interaction-teleport-landing").to_owned(),
            Self::CleaningUp => text("interaction-teleport-cleaning").to_owned(),
            Self::Confirm(bytes) => message(
                "interaction-teleport-bundle-size",
                &[("size", size(bytes).into())],
            ),
            Self::Sending { sent, total } => message(
                "interaction-teleport-sending",
                &[("sent", size(sent).into()), ("total", size(total).into())],
            ),
        }
    }
}

pub(crate) fn size(bytes: u64) -> String {
    if bytes < 1024 {
        return message("interaction-teleport-bytes", &[("value", bytes.into())]);
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    message(
        "interaction-teleport-scaled",
        &[("value", value.into()), ("unit", units[unit].into())],
    )
}

#[cfg(test)]
mod tests {
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
}
