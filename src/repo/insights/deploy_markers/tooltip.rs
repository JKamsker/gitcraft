//! What a deployment pill says on hover, and what "Has … reached …?" answers.

use super::model::{Marker, Standing};
use super::{Answer, State};
use crate::ci::deployments::DeployState;
use crate::forge::Forge;
use crate::i18n::{message, text};

/// So many digests are listed in a tooltip.
const MAX_DIGESTS: usize = 3;

pub(super) fn heading(m: &Marker, standing: Standing) -> String {
    let id = match standing {
        Standing::Current => "graph-deployments-standing-current",
        Standing::Stale => "graph-deployments-standing-stale",
        Standing::Unknown => "graph-deployments-standing-unknown",
    };
    format!(
        "{} — {}",
        message(
            "graph-deployments-heading",
            &[("environment", m.environment.clone().into())]
        ),
        text(id)
    )
}

fn state_label(state: DeployState) -> &'static str {
    text(match state {
        DeployState::Success => "graph-deployments-state-success",
        DeployState::InProgress => "graph-deployments-state-in-progress",
        DeployState::Queued => "graph-deployments-state-queued",
        DeployState::Failed => "graph-deployments-state-failed",
        DeployState::Inactive => "graph-deployments-state-inactive",
        DeployState::Cancelled => "graph-deployments-state-cancelled",
        DeployState::NoStatus => "graph-deployments-state-no-status",
        DeployState::NotRead => "graph-deployments-state-not-read",
    })
}

fn ago(at: i64, now: i64) -> crate::i18n::Value<'static> {
    crate::git::relative_time(at, now).into()
}

/// What a pill's tooltip says, line by line.
pub(super) fn tooltip(m: &Marker, standing: Standing, state: &State, now: i64) -> Vec<String> {
    let d = &m.deployment;
    let mut lines = vec![heading(m, standing)];
    lines.push(match d.created {
        Some(at) => message("graph-deployments-deployed", &[("age", ago(at, now))]),
        None => text("graph-deployments-deployed-unknown").to_owned(),
    });
    let reported = match m.reported() {
        Some(at) => message("graph-deployments-reported", &[("age", ago(at, now))]),
        None => text("graph-deployments-reported-unknown").to_owned(),
    };
    lines.push(message(
        "graph-deployments-status",
        &[
            ("status", state_label(d.state()).into()),
            ("reported", reported.into()),
        ],
    ));
    lines.push(message(
        "graph-deployments-commit",
        &[
            ("sha", crate::repo::short(m.sha()).to_owned().into()),
            ("reference", d.reference.clone().into()),
        ],
    ));
    if m.digests.is_empty() {
        lines.push(text("graph-deployments-no-digest").to_owned());
    }
    for (name, digest) in m.digests.iter().take(MAX_DIGESTS) {
        lines.push(message(
            "graph-deployments-digest",
            &[
                ("name", name.clone().into()),
                ("digest", digest.clone().into()),
            ],
        ));
    }
    lines.push(
        text(match state.forge {
            Some(Forge::GitLab) => "graph-deployments-source-gitlab",
            _ => "graph-deployments-source-github",
        })
        .to_owned(),
    );
    lines.extend(read_lines(m, state, now));
    lines.push(text("graph-deployments-evidence").to_owned());
    lines.push(text("graph-deployments-click").to_owned());
    lines
}

/// How old the read is, and what is known to be missing from it.
fn read_lines(m: &Marker, state: &State, now: i64) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(at) = state.observed {
        lines.push(message(
            "graph-deployments-observed",
            &[("age", ago(at, now))],
        ));
    }
    if let Some(error) = &state.error {
        lines.push(message(
            "graph-deployments-read-failed",
            &[("error", error.clone().into())],
        ));
    } else if state.read_stale(now) {
        lines.push(text("graph-deployments-read-old").to_owned());
    }
    if !state.partial.is_empty() {
        lines.push(message(
            "graph-deployments-partial",
            &[("why", state.partial.join("; ").into())],
        ));
    }
    if m.present == Some(false) {
        lines.push(text("graph-deployments-not-here").to_owned());
    }
    lines
}

/// What the answer to "Has … reached …?" says.
pub(super) fn reached_notice(answer: &Answer, environment: &str, deployed: &str) -> String {
    let id = match answer.contained {
        Some(true) => "graph-deployments-reached-yes",
        Some(false) => "graph-deployments-reached-no",
        None => "graph-deployments-reached-unknown",
    };
    message(
        id,
        &[
            (
                "commit",
                crate::repo::short(&answer.commit).to_owned().into(),
            ),
            ("environment", environment.to_owned().into()),
            ("deployed", crate::repo::short(deployed).to_owned().into()),
        ],
    )
}
