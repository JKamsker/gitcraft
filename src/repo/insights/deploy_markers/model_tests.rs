//! Marker placement from fixture deployments, and how far each can be trusted.

use super::*;
use crate::ci::deployments::Environment;

const NOW: i64 = 100 * 24 * 3600;
const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;

fn deployment(id: u64, env: &str, sha: &str, state: DeployState) -> Deployment {
    Deployment {
        id,
        environment: env.to_owned(),
        sha: sha.to_owned(),
        state: Some(state),
        reported: Some(NOW - HOUR),
        ..Deployment::default()
    }
}

fn fixture() -> Deployments {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let c = "c".repeat(40);
    Deployments {
        environments: ["production", "staging", "qa"]
            .map(|n| Environment {
                name: n.to_owned(),
                url: format!("https://github.com/o/r/deployments/{n}"),
            })
            .to_vec(),
        deployments: vec![
            // Production: a newer deployment failed; the one before it succeeded.
            deployment(9, "production", &c, DeployState::Failed),
            deployment(8, "production", &a, DeployState::Success),
            deployment(7, "staging", &b, DeployState::Success),
            deployment(6, "pr-12-preview", &b, DeployState::Success),
            deployment(5, "review/feature", &b, DeployState::Success),
            // qa: never read past the limit: its status is unknown.
            deployment(4, "qa", &c, DeployState::NotRead),
            // dev: only failures: nothing is known to run there.
            deployment(3, "dev", &a, DeployState::Failed),
            deployment(2, "production", &b, DeployState::Inactive),
        ],
        partial: Vec::new(),
    }
}

/// The markers of the fixture, for the graph's tests.
pub(crate) fn fixture_markers() -> Vec<Marker> {
    markers(&fixture())
}

#[test]
fn each_environment_marks_its_latest_successful_deployment_only() {
    let markers = markers(&fixture());
    let named: Vec<(&str, char, bool)> = markers
        .iter()
        .map(|m| {
            let first = m.sha().chars().next().unwrap_or('?');
            (m.environment.as_str(), first, m.succeeded)
        })
        .collect();
    assert_eq!(
        named,
        [
            ("production", 'a', true),
            ("staging", 'b', true),
            ("qa", 'c', false),
            ("pr-12-preview", 'b', true),
            ("review/feature", 'b', true),
        ],
        "a failed newer deployment, an inactive older one and an environment with only \
         failures never move or add a marker"
    );
}

#[test]
fn previews_are_told_by_name() {
    for name in ["Preview", "pr-3", "review/x", "app-preview-7", "pull/9"] {
        assert!(is_preview(name), "{name}");
    }
    for name in ["production", "staging", "github-pages", "prod"] {
        assert!(!is_preview(name), "{name}");
    }
}

#[test]
fn standing_marks_old_unknown_and_stale_reads() {
    let marker = |reported: Option<i64>, succeeded: bool| Marker {
        deployment: Deployment {
            reported,
            created: None,
            ..Deployment::default()
        },
        succeeded,
        ..Marker::default()
    };
    assert_eq!(
        marker(Some(NOW - HOUR), true).standing(NOW, false),
        Standing::Current
    );
    assert_eq!(
        marker(Some(NOW - 3 * DAY), true).standing(NOW, false),
        Standing::Current
    );
    assert_eq!(
        marker(Some(NOW - 30 * DAY), true).standing(NOW, false),
        Standing::Stale
    );
    assert_eq!(
        marker(Some(NOW - HOUR), true).standing(NOW, true),
        Standing::Stale,
        "a fresh record from a failed or old read is stale"
    );
    assert_eq!(marker(None, true).standing(NOW, false), Standing::Unknown);
    assert_eq!(
        marker(Some(NOW - HOUR), false).standing(NOW, false),
        Standing::Unknown,
        "a record whose status wasn't read is never current"
    );
}

#[test]
fn reported_falls_back_to_creation() {
    let m = Marker {
        deployment: Deployment {
            reported: None,
            created: Some(42),
            ..Deployment::default()
        },
        ..Marker::default()
    };
    assert_eq!(m.reported(), Some(42));
}
