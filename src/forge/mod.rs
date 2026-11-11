//! The forges a repository can be hosted on — GitHub, GitLab, Forgejo, Gitea — behind one
//! interface. Each has a module of its own that implements [`Provider`]; the rest of the app only
//! talks to that, so adding a forge means adding a module and a [`Forge`] variant.
//!
//! - GitHub and GitLab go through their own command-line tools, `gh` and `glab`. They hold the
//!   user's login, so gitcraft never stores a token.
//! - Forgejo and Gitea share one API, which gitcraft calls itself with the login `tea` (or git's
//!   credential helper) already has for the host. See [`gitea`].

mod azure;
mod bitbucket;
mod cli;
pub(crate) mod cloud;
pub(crate) use cli::tool_location;
mod forks;
pub(crate) use cli::{ToolReport, tool_report};
pub use forks::ForkInfo;
mod gitea;
mod github;
mod gitlab;
pub(crate) mod http;
mod identity;
pub mod inbox;
pub mod links;
#[cfg(test)]
mod routing_tests;
pub(crate) mod ssh_keys;
pub mod stack;
#[cfg(test)]
mod stack_tests;

use std::collections::HashMap;
use std::path::Path;

use crate::avatars::Hint;
use crate::git::Remote;
use crate::pr_post::Target;
use crate::pull_request::PrDetails;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Forge {
    GitHub,
    GitLab,
    Forgejo,
    Gitea,
    Azure,
    Bitbucket,
}

impl Forge {
    pub const ALL: [Forge; 6] = [
        Forge::GitHub,
        Forge::GitLab,
        Forge::Forgejo,
        Forge::Gitea,
        Forge::Azure,
        Forge::Bitbucket,
    ];

    /// The module that does the work.
    pub fn provider(self) -> &'static dyn Provider {
        match self {
            Forge::GitHub => &github::GitHub,
            Forge::GitLab => &gitlab::GitLab,
            Forge::Forgejo => &gitea::FORGEJO,
            Forge::Gitea => &gitea::GITEA,
            Forge::Azure => &azure::Azure,
            Forge::Bitbucket => &bitbucket::Bitbucket,
        }
    }

    /// "GitHub", "Forgejo"…
    pub fn name(self) -> &'static str {
        self.provider().name()
    }

    /// "pull request" or "merge request".
    pub fn noun(self) -> &'static str {
        self.provider().noun()
    }

    /// Localized presentation; `noun()` retains its existing protocol identity.
    pub fn noun_label(self) -> &'static str {
        crate::i18n::text(match self {
            Forge::GitLab => "backend-noun-merge-request",
            _ => "backend-noun-pull-request",
        })
    }

    /// The heading of the sidebar's list.
    pub fn list_title(self) -> &'static str {
        match self.noun() {
            "merge request" => crate::i18n::text("backend-merge-requests-6e2149"),
            _ => crate::i18n::text("backend-pull-requests-334b28"),
        }
    }

    /// Whether gitcraft can show its pull requests in full (and review and merge them).
    pub fn has_pages(self) -> bool {
        self.provider().pages().is_some()
    }

    /// The name `gitcraft.forge` is set to in git's config to pick this forge.
    pub fn key(self) -> &'static str {
        match self {
            Forge::GitHub => "github",
            Forge::GitLab => "gitlab",
            Forge::Forgejo => "forgejo",
            Forge::Gitea => "gitea",
            Forge::Azure => "azure",
            Forge::Bitbucket => "bitbucket",
        }
    }

    pub fn from_key(key: &str) -> Option<Forge> {
        let key = key.trim().to_lowercase();
        Forge::ALL.into_iter().find(|forge| forge.key() == key)
    }
}

/// Where a repository lives: which forge, and where on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hosting {
    pub forge: Forge,
    /// The host's name, lowercase and without a port: what pull requests' URLs are matched by.
    pub host: String,
    /// `owner/repo` (on GitLab, `group/sub/repo`).
    pub path: String,
    /// Where its web pages and API are: `https://host`, with the port when the remote names one.
    pub web: String,
}

/// What a new pull request is made of.
pub struct NewPullRequest<'a> {
    pub branch: &'a str,
    pub base: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub draft: bool,
    /// Who to ask for a review: accounts (`ann`) and teams (`acme/payments`).
    pub reviewers: &'a [String],
}

/// What a new release on the forge is made of: its tag is pushed already.
pub struct NewRelease<'a> {
    pub tag: &'a str,
    pub title: &'a str,
    pub notes: &'a str,
    pub prerelease: bool,
}

/// Serializable for the list kept until the forge is asked again (`sidebar::pull_requests`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    /// The branch it proposes (head / source branch).
    pub branch: String,
    /// The branch it goes into (base / target branch).
    pub base: String,
    /// The base tip observed with `sha`, when the list supplies exact diff endpoints.
    /// Older persisted lists and providers without it retain the details lookup.
    #[serde(default)]
    pub base_oid: String,
    /// It proposes a branch of another repository (a fork), which may share a name with one
    /// here.
    pub from_fork: bool,
    /// Its latest commit.
    pub sha: String,
    pub author: String,
    pub url: String,
    pub draft: bool,
}

/// A merged pull request: what branch cleanup needs of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedPullRequest {
    pub number: u64,
    /// The branch it proposed.
    pub branch: String,
    /// Its last commit.
    pub sha: String,
    /// The branch it was merged into.
    pub base: String,
}

/// The merged pull requests in a forge's JSON list, `pointers` saying where its number, branch,
/// last commit and base are in each item.
fn parse_merged(
    json: &str,
    pointers: [&str; 4],
    tool: &str,
) -> Result<Vec<MergedPullRequest>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        crate::i18n::message(
            "backend-unexpected-tool-output-e-50db44",
            &[
                ("tool", (tool).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })?;
    Ok(merged_items(
        value.as_array().map_or(&[][..], Vec::as_slice),
        pointers,
    ))
}

/// [`parse_merged`] of items already read.
fn merged_items(items: &[serde_json::Value], pointers: [&str; 4]) -> Vec<MergedPullRequest> {
    let [number, branch, sha, base] = pointers;
    let text = |v: &serde_json::Value, p: &str| {
        v.pointer(p)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    items
        .iter()
        .map(|item| MergedPullRequest {
            number: item
                .pointer(number)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            branch: text(item, branch),
            sha: text(item, sha),
            base: text(item, base),
        })
        .filter(|pr| !pr.branch.is_empty() && !pr.sha.is_empty())
        .collect()
}

/// How many of the issues assigned to you are listed at most: the list says when it may have
/// more.
pub(crate) const ISSUES_MAX: usize = 100;

/// An open issue assigned to the person signed in, to start work from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub tracker: Option<crate::trackers::Identity>,
    /// An explicitly selected tracker field takes precedence over description heuristics.
    pub acceptance: Option<String>,
    /// Its number on the forge (GitLab's `iid`, the one `#N` means).
    pub number: u64,
    pub title: String,
    /// The description, as Markdown.
    pub body: String,
    pub url: String,
    pub labels: Vec<String>,
}

impl Issue {
    /// Source-qualified stable key for UI interactions; tracker display numbers are not IDs.
    pub(crate) fn identity(&self) -> String {
        self.tracker.as_ref().map_or_else(
            || serde_json::json!(["forge", self.url, self.number]).to_string(),
            |i| serde_json::json!([i.kind, i.origin, i.id]).to_string(),
        )
    }
    pub fn key(&self) -> String {
        self.tracker
            .as_ref()
            .map_or_else(|| format!("#{}", self.number), |i| i.key.clone())
    }
    pub fn branch_name(&self) -> String {
        self.tracker.as_ref().map_or_else(
            || crate::repo::issue_work::issue_branch(self.number, &self.title),
            |i| crate::repo::issue_work::tracker_branch(&i.key, &self.title),
        )
    }
}

/// Where a pull request stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    /// Closed without being merged.
    Closed,
}

/// A pull request found by its branch, and where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub pr: PullRequest,
    pub state: PrState,
}

/// The one to take of the pull requests proposing a branch, newest first: an open one, else
/// the newest; never a fork's.
pub(crate) fn pick_found(found: Vec<Found>) -> Option<Found> {
    let own: Vec<Found> = found.into_iter().filter(|f| !f.pr.from_fork).collect();
    own.iter()
        .find(|f| f.state == PrState::Open)
        .or_else(|| own.first())
        .cloned()
}

/// What a forge does for gitcraft.
pub trait Provider: Sync {
    /// "GitHub", "Forgejo"…
    fn name(&self) -> &'static str;

    /// What it calls a proposed change.
    fn noun(&self) -> &'static str {
        "pull request"
    }

    /// Whether a host is this forge's by its name alone (`github.com`, `gitlab.example.org`).
    fn claims(&self, host: &str) -> bool;

    /// The open pull requests.
    fn list(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<PullRequest>, String>;

    /// The latest pull requests merged (up to 100): a local branch still at or behind the last
    /// commit of one it proposed was merged, even squashed or rebased into other commits.
    fn merged(&self, _repo: &Path, _hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
        Ok(Vec::new())
    }

    /// Opens a pull request; returns its URL (or whatever the forge said).
    fn create(
        &self,
        repo: &Path,
        hosting: &Hosting,
        new: &NewPullRequest,
    ) -> Result<String, String>;

    /// Changes the branch the pull request numbered `number` goes into: the next layer down of
    /// a stack, or what that one went into once it merged.
    fn retarget(
        &self,
        repo: &Path,
        hosting: &Hosting,
        number: u64,
        base: &str,
    ) -> Result<(), String>;

    /// The latest pull request proposing `branch` of this repository (not a fork's branch of
    /// the same name), whatever its state: asked of the forge by branch, so neither a long list
    /// nor a branch copied or renamed here can mislead it.
    fn find(&self, repo: &Path, hosting: &Hosting, branch: &str) -> Result<Option<Found>, String>;

    /// Where the account pictures of the authors (by commit email) are.
    fn avatars(&self, repo: &Path, hosting: &Hosting, emails: &[String]) -> Vec<(String, Hint)>;

    /// The open issues of the repository assigned to the person signed in.
    fn assigned_issues(&self, _repo: &Path, _hosting: &Hosting) -> Result<Vec<Issue>, String> {
        Err(crate::i18n::message(
            "backend-gitcraft-can-t-list-s-issues-0a6c81",
            &[("name", (self.name()).to_string().into())],
        ))
    }

    /// Issue `number` of the repository as it is now, to read its description again.
    fn issue(&self, _repo: &Path, _hosting: &Hosting, number: u64) -> Result<Issue, String> {
        Err(crate::i18n::message(
            "backend-gitcraft-can-t-read-s-issue-number-6d6c88",
            &[
                ("name", (self.name()).to_string().into()),
                ("number", (number).to_string().into()),
            ],
        ))
    }

    /// The open pull requests that wait for the person logged in: those whose review is asked
    /// of them, and theirs (see [`inbox::needs_you`] for which of them need them now). It runs
    /// unasked, so it never asks for a password or a keyring: only logins already at hand.
    fn inbox(&self, repo: &Path, hosting: &Hosting) -> Result<inbox::Answer, String> {
        let _ = (repo, hosting);
        Err(crate::i18n::message(
            "backend-gitcraft-can-t-list-the-s-waiting-for-you-on-c02926",
            &[
                ("noun", (self.noun()).to_string().into()),
                ("name", (self.name()).to_string().into()),
            ],
        ))
    }

    /// Whether [`Provider::inbox`] answers for the whole host (every repository there), so
    /// that it is asked once per host rather than once per repository.
    fn inbox_per_host(&self) -> bool {
        false
    }

    /// The page of one pull request, if gitcraft has one for this forge.
    fn pages(&self) -> Option<&dyn Pages> {
        None
    }

    /// Where each open pull request's branch is (a fork or the repository itself), and what its
    /// author allows.
    fn forks(&self, _repo: &Path, _hosting: &Hosting) -> Result<Vec<ForkInfo>, String> {
        Err(crate::i18n::message(
            "backend-can-t-say-where-its-s-come-from-013949",
            &[
                ("name", (self.name()).to_string().into()),
                ("noun", (self.noun()).to_string().into()),
            ],
        ))
    }

    /// The workflow runs waiting for a maintainer's approval: `(commit, run id)`.
    fn waiting_runs(&self, _repo: &Path, _hosting: &Hosting) -> Result<Vec<(String, u64)>, String> {
        Ok(Vec::new())
    }

    /// Approves the workflow run `run`, which then runs.
    fn approve_run(&self, _repo: &Path, _hosting: &Hosting, _run: u64) -> Result<(), String> {
        Err(crate::i18n::message(
            "backend-has-no-runs-to-approve-from-gitcraft-635889",
            &[("name", (self.name()).to_string().into())],
        ))
    }

    /// Makes the release of a pushed tag; returns its URL (or whatever the forge said).
    fn create_release(
        &self,
        _repo: &Path,
        _hosting: &Hosting,
        _new: &NewRelease,
    ) -> Result<String, String> {
        Err(crate::i18n::message(
            "backend-gitcraft-can-t-make-releases-on-16af5d",
            &[("name", (self.name()).to_string().into())],
        ))
    }

    /// Attaches `files` (of this machine) to the release of `tag`.
    fn upload_assets(
        &self,
        _repo: &Path,
        _hosting: &Hosting,
        _tag: &str,
        _files: &[std::path::PathBuf],
    ) -> Result<(), String> {
        Err(crate::i18n::message(
            "backend-gitcraft-can-t-attach-files-to-releases-on-c02ad8",
            &[("name", (self.name()).to_string().into())],
        ))
    }
}

/// The paths of `files` as arguments.
pub(crate) fn path_args(files: &[std::path::PathBuf]) -> Vec<String> {
    files
        .iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect()
}

/// One pull request in full: reading it, and acting on it.
pub trait Pages: Sync {
    /// Reads the pull request numbered `number`: description, checks, commits, conversation,
    /// threads on lines and what the person logged in may do.
    fn details(&self, repo: &Path, hosting: &Hosting, number: u64) -> Result<PrDetails, String>;

    /// What says or does something on the pull request `details`.
    fn target(
        &self,
        repo: &Path,
        hosting: &Hosting,
        details: &PrDetails,
    ) -> Option<Box<dyn Target>>;
}

pub(crate) mod history;
#[cfg(test)]
mod merged_tests;
#[cfg(test)]
mod tests;
pub(crate) use gitea::{selected_tea_metadata, selected_tea_secret};

pub(crate) use http::environment_proxy;

mod routing;
use routing::*;
pub use routing::{detect, location, web_url};
pub(crate) use routing::{detect_remotes, gitcraft_config};

mod followup;
use followup::*;
pub(crate) use followup::{ask_reviewers, tool};
