//! The git side of pull requests from forks: checking a fork's branch out so that pushing goes
//! back to it, rebasing it onto its base and pushing it to the contributor, and the remotes made
//! for forks that nothing uses any more.

mod checkout;
pub(crate) use checkout::*;
mod rebase;
pub(crate) use rebase::*;

use std::path::Path;

use super::guard::{CheckoutDestination, Destination};
use crate::forge::{ForkInfo, Hosting};
use crate::git::{self, Remote};
use crate::repo::operations::{expect_ref, expect_ref_at, expect_symref};

/// The key that marks a remote gitcraft added for a fork: only those are offered for tidying.
pub(crate) const MARK: &str = "gitcraftFork";

/// How a local branch is set up: the remote it pulls from and the one it pushes to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BranchSetup {
    pub name: String,
    /// `branch.<name>.remote`.
    pub remote: String,
    /// `branch.<name>.merge`, `refs/heads/x`.
    pub merge: String,
    /// `branch.<name>.pushRemote`.
    pub push_remote: String,
}

/// What git's config says of the branches, and which remotes were added for forks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Setup {
    pub branches: Vec<BranchSetup>,
    pub fork_remotes: Vec<String>,
}

impl Setup {
    /// The local branch set up to push to `remote`'s `branch` (by its upstream there).
    pub(crate) fn pushing_to(&self, remote: &str, branch: &str) -> Option<&BranchSetup> {
        let merge = format!("refs/heads/{branch}");
        self.branches.iter().find(|b| {
            b.merge == merge
                && (b.push_remote == remote || (b.push_remote.is_empty() && b.remote == remote))
        })
    }

    /// The fork remotes no branch pulls from or pushes to.
    pub(crate) fn unused_forks(&self) -> Vec<String> {
        self.fork_remotes
            .iter()
            .filter(|r| {
                !self
                    .branches
                    .iter()
                    .any(|b| b.remote == **r || b.push_remote == **r)
            })
            .cloned()
            .collect()
    }
}

/// The push of the branch `local`, set up to push to a fork's branch of another name
/// (`ann/fix` → ann's `fix`): `git push` alone would look for one of its own name there.
pub(crate) fn renamed_push(setup: &Setup, local: &str) -> Option<Vec<String>> {
    let b = setup.branches.iter().find(|b| b.name == local)?;
    let theirs = b.merge.strip_prefix("refs/heads/")?;
    if b.push_remote.is_empty() || theirs == local {
        return None;
    }
    Some(strings(&[
        "push",
        "--",
        &b.push_remote,
        &format!("refs/heads/{local}:refs/heads/{theirs}"),
    ]))
}

/// Reads [`Setup`] from the repository's config.
pub(crate) fn read_setup(repo: &Path) -> Result<Setup, String> {
    // Exit 1: nothing matches.
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "config",
            "--null",
            "--get-regexp",
            r"^(branch\..*\.(remote|merge|pushremote)|remote\..*\.gitcraftfork)$",
        ],
        &[0, 1],
    )?;
    Ok(parse_setup(&out))
}

fn parse_setup(out: &str) -> Setup {
    let mut setup = Setup::default();
    // `--null`: `key\nvalue\0`.
    for entry in out.split('\0').filter(|e| !e.is_empty()) {
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        if let Some(rest) = key.strip_prefix("remote.") {
            if let Some(name) = rest.strip_suffix(".gitcraftfork")
                && value.trim() == "true"
            {
                setup.fork_remotes.push(name.to_owned());
            }
            continue;
        }
        let Some(rest) = key.strip_prefix("branch.") else {
            continue;
        };
        // The branch's name keeps its case and its dots; the variable's name is lowercase.
        let Some((name, var)) = rest.rsplit_once('.') else {
            continue;
        };
        let i = match setup.branches.iter().position(|b| b.name == name) {
            Some(i) => i,
            None => {
                setup.branches.push(BranchSetup {
                    name: name.to_owned(),
                    ..BranchSetup::default()
                });
                setup.branches.len() - 1
            }
        };
        let b = &mut setup.branches[i];
        match var {
            "remote" => b.remote = value.to_owned(),
            "merge" => b.merge = value.to_owned(),
            "pushremote" => b.push_remote = value.to_owned(),
            _ => {}
        }
    }
    setup
}

/// Removing the fork remotes nothing uses, and adding them back.
pub(crate) fn tidy(remotes: &[Remote], unused: &[String]) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let mut steps = Vec::new();
    let mut undo = Vec::new();
    for name in unused {
        let Some(r) = remotes.iter().find(|r| r.name == *name) else {
            continue;
        };
        steps.push(strings(&["remote", "remove", "--", name]));
        undo.push(strings(&["remote", "add", "--", name, &r.url]));
        undo.push(strings(&[
            "config",
            &format!("remote.{name}.{MARK}"),
            "true",
        ]));
    }
    (steps, undo)
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fluent_r1_identifier_tests;
