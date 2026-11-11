//! Only the actual completed outcome supplies these fields. Test output is not retained.
use crate::parallel_bisect::{Machine, Outcome, presentation};

#[derive(Clone, Debug)]
pub(super) enum Summary {
    FirstBad(String),
    OneOf(usize),
    GoodEndFails {
        hash: String,
        machine: Machine,
    },
    BadEndPasses {
        hash: String,
        machine: Machine,
    },
    Cancelled,
    Failed {
        raw: String,
        receipt: Option<presentation::Presentation>,
    },
}

impl Summary {
    pub(super) fn capture(
        outcome: &Outcome,
        machine: &Machine,
        receipt: Option<presentation::Presentation>,
    ) -> Self {
        match outcome {
            Outcome::FirstBad(hash) => Self::FirstBad(hash.clone()),
            Outcome::OneOf(hashes) => Self::OneOf(hashes.len()),
            Outcome::GoodEndFails { hash, .. } => Self::GoodEndFails {
                hash: hash.clone(),
                machine: machine.clone(),
            },
            Outcome::BadEndPasses { hash, .. } => Self::BadEndPasses {
                hash: hash.clone(),
                machine: machine.clone(),
            },
            Outcome::Cancelled => Self::Cancelled,
            Outcome::Failed(raw) => Self::Failed {
                raw: raw.clone(),
                receipt: receipt.and_then(|p| p.matching(raw)),
            },
        }
    }

    pub(super) fn display(&self) -> String {
        use crate::i18n::{message, text};
        match self {
            Self::FirstBad(hash) => message(
                "repo-workflows-parallel_bisect-outcome-message-323d18af",
                &[("short-hash-1", crate::repo::short(hash).into())],
            ),
            Self::OneOf(count) => message(
                "repo-workflows-parallel_bisect-outcome-message-5f365ac7",
                &[("hashes-len-1", (*count).into())],
            ),
            Self::GoodEndFails { hash, machine } => message(
                "repo-workflows-parallel_bisect-outcome-message-a5f8e4f1",
                &[
                    ("short-hash-1", crate::repo::short(hash).into()),
                    (
                        "machine",
                        crate::parallel_bisect::display::machine_label(machine).into(),
                    ),
                ],
            ),
            Self::BadEndPasses { hash, machine } => message(
                "repo-workflows-parallel_bisect-outcome-message-12d782ed",
                &[
                    ("short-hash-1", crate::repo::short(hash).into()),
                    (
                        "machine",
                        crate::parallel_bisect::display::machine_label(machine).into(),
                    ),
                ],
            ),
            Self::Cancelled => {
                text("repo-workflows-parallel_bisect-outcome-message-9edd35e1").to_owned()
            }
            Self::Failed { raw, receipt } => {
                let rendered = presentation::error(raw, receipt.as_ref());
                message(
                    "repo-workflows-parallel_bisect-outcome-message-db128bb5",
                    &[(
                        "e-lines-next-unwrap-or-default-1",
                        rendered.lines().next().unwrap_or_default().into(),
                    )],
                )
            }
        }
    }
}
