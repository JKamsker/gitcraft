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

mod diagnostic;
pub(crate) use diagnostic::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod diagnostic_tests;
