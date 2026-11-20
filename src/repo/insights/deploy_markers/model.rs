//! What the deployment markers are made of, apart from their drawing: for each
//! environment, the commit the forge last recorded a successful deployment of, with how fresh
//! that record is and, when its run uploaded any, the artifacts' digests.
//!
//! Only the forge's deployment records count. A green build or a release tag is never taken
//! for a deployment. An environment none of whose read deployments succeeded is marked on its
//! newest record only when that record's status is unknown, and the marker says so.

use std::collections::HashMap;
use std::path::Path;

use crate::ci::Project;
use crate::ci::deployments::{self, DeployState, Deployment, Deployments};
use crate::insights::deploy::{Freshness, environments, freshness};

/// Whether two commit ids name the same commit (either may be abbreviated).
fn same(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

/// So many runs have their artifacts read for the markers' digests.
const MAX_DIGEST_READS: usize = 4;

/// How far a marker can be trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    /// The latest successful deployment, reported within the last two weeks by a read that
    /// is itself recent.
    Current,
    /// Reported longer ago, or shown from an old or failed read: it may no longer run there.
    Stale,
    /// No time was reported, or no successful deployment was among those read and this is
    /// the newest record, its status unknown.
    Unknown,
}

/// One environment's deployed commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Marker {
    pub(crate) environment: String,
    /// The environment's page on the forge.
    pub(crate) env_url: String,
    pub(crate) deployment: Deployment,
    /// The forge reported it successful (else it is the newest record, status unknown).
    pub(crate) succeeded: bool,
    /// A preview or review environment: collapsed with others of its kind on a row.
    pub(crate) preview: bool,
    /// The artifacts its run uploaded that have a digest: (name, digest).
    pub(crate) digests: Vec<(String, String)>,
    /// Whether the deployed commit is in this repository (`None`: not checked).
    pub(crate) present: Option<bool>,
}

impl Marker {
    pub(crate) fn sha(&self) -> &str {
        &self.deployment.sha
    }

    /// Whether it marks commit `hash` (either may be abbreviated).
    pub(crate) fn on(&self, hash: &str) -> bool {
        same(&self.deployment.sha, hash)
    }

    /// When the forge reported it: its latest status, else its creation.
    pub(crate) fn reported(&self) -> Option<i64> {
        self.deployment.reported.or(self.deployment.created)
    }

    /// How far it can be trusted at `now`; `read_stale`: the read it came from is old or the
    /// latest one failed.
    pub(crate) fn standing(&self, now: i64, read_stale: bool) -> Standing {
        if !self.succeeded {
            return Standing::Unknown;
        }
        match freshness(self.reported(), now) {
            Freshness::Unknown => Standing::Unknown,
            Freshness::Old => Standing::Stale,
            Freshness::Recent | Freshness::Days if read_stale => Standing::Stale,
            Freshness::Recent | Freshness::Days => Standing::Current,
        }
    }
}

/// Whether environment `name` is a preview (review app, pull-request environment).
fn is_preview(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("preview")
        || ["review/", "review-", "pr-", "pr/", "pull/", "pull-request"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

/// The markers of what was read: one per environment, in the forge's order.
fn markers(read: &Deployments) -> Vec<Marker> {
    environments(read, "", |_| None)
        .into_iter()
        .filter_map(|row| {
            let (deployment, succeeded) = match row.current {
                Some(d) => (d, true),
                None => {
                    let newest = read
                        .deployments
                        .iter()
                        .find(|d| d.environment == row.name)?;
                    let unknown =
                        matches!(newest.state(), DeployState::NotRead | DeployState::NoStatus);
                    if !unknown {
                        return None;
                    }
                    (newest.clone(), false)
                }
            };
            if deployment.sha.is_empty() {
                return None;
            }
            Some(Marker {
                preview: is_preview(&row.name),
                environment: row.name,
                env_url: row.url,
                deployment,
                succeeded,
                digests: Vec::new(),
                present: None,
            })
        })
        .collect()
}

/// What one read found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Read {
    pub(crate) markers: Vec<Marker>,
    /// Why it is incomplete, when it is.
    pub(crate) partial: Vec<String>,
    /// A deployment is queued or under way: worth reading again soon.
    pub(crate) in_flight: bool,
}

/// Whether commit `sha` is in the repository, without fetching anything.
fn present(repo: &Path, sha: &str) -> bool {
    let hex = (7..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit());
    hex && crate::git::run_env(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{sha}^{{commit}}"),
        ],
        &[("GIT_NO_LAZY_FETCH", "1")],
    )
    .is_ok()
}

/// Reads the deployments of `project` and makes the markers, with whether each deployed
/// commit is here and the digests of the first [`MAX_DIGEST_READS`] runs that did them.
pub(crate) fn read(repo: &Path, project: &Project) -> Result<Read, String> {
    let recorded = deployments::read(repo, project)?;
    let mut out = Read {
        markers: markers(&recorded),
        in_flight: recorded.deployments.iter().any(|d| d.state().in_flight()),
        partial: recorded.partial,
    };
    let mut digests: HashMap<u64, Vec<(String, String)>> = HashMap::new();
    let mut cut = false;
    for marker in &mut out.markers {
        marker.present = Some(present(repo, marker.sha()));
        let Some(run) = marker.deployment.run.filter(|_| marker.succeeded) else {
            continue;
        };
        if !digests.contains_key(&run) {
            if digests.len() == MAX_DIGEST_READS {
                cut = true;
                continue;
            }
            let found = deployments::artifacts(repo, project, run)
                .map(|list| {
                    list.into_iter()
                        .filter_map(|a| Some((a.name, a.digest?)))
                        .collect()
                })
                .unwrap_or_default();
            digests.insert(run, found);
        }
        marker.digests = digests.get(&run).cloned().unwrap_or_default();
    }
    if cut {
        out.partial.push(format!(
            "the artifacts of only {MAX_DIGEST_READS} deploying runs were read"
        ));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "model_tests.rs"]
pub(super) mod tests;
