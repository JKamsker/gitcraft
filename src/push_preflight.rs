//! Explicit push previews. Reads are local, bounded and never ask a credential helper.
//! Findings contain object IDs, paths and line numbers, never matched secret values.

pub(crate) mod display;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::git;
use crate::settings::profiles::UserProfile;

pub(crate) const APPROVED: &str = "\0push-preflight";
pub(crate) const FIX: &str = "\0push-preflight-fix";
mod fix;
pub(crate) use fix::fix;
#[cfg(test)]
pub(crate) mod tests;
const MAX_COMMITS: usize = 10_000;
const MAX_OBJECTS: usize = 50_000;
const MAX_FINDINGS: usize = 2_000;
const LARGE: u64 = 5 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Identity,
    Signature,
    Message,
    Secret,
    Large,
    RemoteHistory,
    Transcript,
}

#[derive(Clone, Debug)]
pub(crate) struct Finding {
    pub kind: Kind,
    pub commit: String,
    pub path: String,
    pub detail: String,
}

#[derive(Clone)]
pub(crate) struct Commit {
    pub oid: String,
    pub parents: Vec<String>,
    pub name: String,
    pub email: String,
    pub subject: String,
    pub signed: bool,
}

#[derive(Clone)]
pub(crate) struct Plan {
    pub branch: String,
    pub remote: String,
    pub destination: String,
    pub head: String,
    pub base: Option<String>,
    pub commits: Vec<Commit>,
    pub findings: Vec<Finding>,
    pub targets: Vec<String>,
    pub expected_name: String,
    pub expected_email: String,
    pub expected_accounts: BTreeMap<String, String>,
    pub profile: UserProfile,
    pub force: bool,
    pub deleting: bool,
    pub tracking: bool,
    pub lfs_objects: BTreeSet<String>,
    pub skip_hooks: bool,
    push_oid: String,
    original: Vec<Vec<String>>,
    urls: Vec<String>,
    checkout: String,
    checkout_oid: String,
    config: String,
    config_before_push: String,
    local_refs: BTreeMap<String, String>,
    extras: Vec<String>,
    tracking_ref: Option<String>,
    lfs_config: Vec<(String, String)>,
    receive_pack: String,
    selection: Option<git::repository_tools::routes::PreparedPush>,
    // Captured lifetime survives tab invalidation, APPROVED, reload and auth retry.
    // Never serialized or reconstructed from a label on the current tab.
    fork_scope: Option<crate::repo::forks::ExecutionScope>,
}

impl Plan {
    pub(crate) fn fork_scope(&self) -> Option<&crate::repo::forks::ExecutionScope> {
        self.fork_scope.as_ref()
    }

    pub(crate) fn retain_fork_scope(&mut self, scope: crate::repo::forks::ExecutionScope) {
        self.fork_scope = Some(scope);
    }

    fn check_fork_scope(&self, repo: &Path) -> Result<(), String> {
        self.fork_scope
            .as_ref()
            .map_or(Ok(()), |scope| scope.check_remote(repo))
    }

    pub(crate) fn selection(&self) -> Option<&git::repository_tools::routes::PreparedPush> {
        self.selection.as_ref()
    }

    /// A sidebar/dependency choice can constrain a root checklist, never approve
    /// it. Keep the original endpoint and lease across rechecks and echo/retry.
    pub(crate) fn retain_selection(
        &mut self,
        selected: git::repository_tools::routes::PreparedPush,
    ) -> Result<(), String> {
        self.matches_selection(&selected.route)?;
        self.selection = Some(selected);
        Ok(())
    }

    fn matches_selection(
        &self,
        selected: &git::repository_tools::routes::PushRoute,
    ) -> Result<(), String> {
        if self.deleting
            || self.remote != selected.remote
            || self.destination != selected.destination
            || self.push_oid != selected.commit
            || self.head != selected.commit
            || self.urls != selected.urls
            || selected
                .lease
                .as_deref()
                .is_some_and(|lease| self.base.as_deref() != Some(lease))
        {
            return Err("The selected push endpoint, commit or advertised lease changed. Fetch the exact target if needed, then reopen the original push action; nothing was sent.".into());
        }
        Ok(())
    }
    pub(crate) fn is_current_branch(&self) -> bool {
        !self.deleting && self.branch == self.checkout && self.branch.starts_with("refs/heads/")
    }
    pub(crate) fn lfs_targets(&self) -> Vec<String> {
        let values: BTreeSet<_> = self
            .lfs_config
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(_, v)| target_label(v))
            .collect();
        if values.is_empty() {
            self.targets.clone()
        } else {
            values.into_iter().collect()
        }
    }

    pub(crate) fn local_steps(&self) -> Vec<(bool, String)> {
        let mut before = true;
        let mut steps = Vec::new();
        for op in &self.original {
            if push_words(op).is_some() {
                before = false;
            } else if !op.first().is_some_and(|value| value.starts_with('\0')) {
                steps.push((before, display_path(&op.join(" "))));
            }
        }
        steps
    }

    pub(crate) fn again(&self) -> Vec<Vec<String>> {
        self.original.clone()
    }
}

#[cfg(all(test, unix))]
pub(super) fn recording_input(
    command: std::process::Command,
    input: Option<Vec<u8>>,
    _active: &dyn Fn() -> bool,
    timeout: std::time::Duration,
    stdout: usize,
    stderr: usize,
) -> Result<std::process::Output, String> {
    crate::processes::run(
        command,
        crate::processes::Options {
            input,
            timeout: Some(timeout),
            output_limit: stdout,
            stderr_limit: Some(stderr),
            ..Default::default()
        },
    )?
    .complete()
}

mod approval;
mod capture;
mod execution;
mod findings;
mod identity;
mod objects;
mod parsing;
mod routing;
mod scan;
mod secrets;

pub(crate) use approval::{
    is_reviewed_auth_error, register, registered, remember_auth_endpoint, take_auth_endpoint,
};
use capture::{clean_lfs_config, config, config_stamp, optional, read, valid_ref};
pub(crate) use execution::*;
pub(crate) use identity::{display_path, location, target_label};
use parsing::{
    CapturedLease, captured_lease, companion_branch, lease_requirement, parse, push_words,
    saved_branch_spec, valid_object_id,
};
pub(crate) use parsing::{has_push, validate_operations};
pub(crate) use scan::scan;
use secrets::{blob_secret_lines, secret_lines};
#[cfg(test)]
mod internals_route_recording;
#[cfg(test)]
pub(crate) mod selected_continuation_recording;
/// Credential-free routing/identity stamp for durable outbox endpoint identity.
pub(crate) fn outbox_config_stamp(repo: &Path) -> Result<String, String> {
    config_stamp(repo, &|| !crate::tasks::cancellation().is_cancelled())
}

pub(crate) use fix::accept_rewrite;

/// The direct Push flow retains route validation without scanning or repairing history.
/// Git itself decides whether the ordinary update is a fast-forward.
pub(crate) fn check_ordinary_push(repo: &Path, ops: &[Vec<String>]) -> Result<(), String> {
    for op in ops {
        // Reviewer echo can bind the command to a captured source. Its executor
        // validates that envelope independently; route resolution needs the Git argv.
        let mut plain = Vec::new();
        let mut words = op.as_slice();
        while let [flag, value, rest @ ..] = words {
            if flag != "-c" {
                break;
            }
            if !value.starts_with("gitcraft.commandSource=") {
                plain.extend([flag.clone(), value.clone()]);
            }
            words = rest;
        }
        plain.extend_from_slice(words);
        if !has_push(std::slice::from_ref(&plain)) {
            continue;
        }
        let route = routing::resolve(repo, &[plain], &|| true)?;
        if route.force || route.deleting {
            return Err("This push configuration rewrites or deletes remote history. Use an explicit force-with-lease or deletion action to review it; nothing was pushed.".into());
        }
    }
    Ok(())
}
