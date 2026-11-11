//! A pull request selected in the list: its branch in the graph and its whole patch.

use std::path::Path;
use std::sync::Arc;

use super::editor_in_the_way;
use crate::forge::{Forge, Hosting, PullRequest};
use crate::git::{self, Credentials, Remote};
use crate::pull_request::{self, PrDetails};
use crate::repo::{Compare, CompareData, RepoTab};

struct Selection {
    base: String,
    head: git::Commit,
    data: CompareData,
}

impl RepoTab {
    pub(in crate::repo) fn select_pr(&mut self, number: u64, ctx: &egui::Context) {
        if let Some(why) = editor_in_the_way(self.conflict.is_some(), self.rebase.is_some()) {
            self.status = Some(crate::repo::Notice::plain((why.to_owned(), true)));
            return;
        }
        let Some(pr) = self.prs.iter().find(|pr| pr.number == number).cloned() else {
            return;
        };
        let Some(target) = self.pr_target(&pr) else {
            self.status = Some(crate::repo::Notice::plain((
                "The pull request's branch is not available yet".to_owned(),
                true,
            )));
            return;
        };
        self.leave_pages();
        self.doctor = None;
        self.bakeoff.view = None;
        self.file_view = None;
        self.diff = None;
        self.nav_jump = true;
        self.pending_goto = None;
        self.focus_pr_head(&target);

        // The remote's base is what the pull request goes into; a local main can have
        // unpublished commits. Pages below replace both endpoints with the forge's OIDs.
        let remote = self.primary_forge_remote().map(str::to_owned);
        let base = remote
            .as_ref()
            .and_then(|remote| {
                self.refs
                    .remotes
                    .iter()
                    .find(|(r, _)| r == remote)
                    .and_then(|(_, branches)| {
                        branches
                            .iter()
                            .find(|b| b.name == format!("{remote}/{}", pr.base))
                    })
            })
            .map(|b| format!("refs/remotes/{}", b.name))
            .unwrap_or_else(|| format!("refs/heads/{}", pr.base));
        let compare = Compare {
            base,
            base_label: pr.base.clone(),
            target: Some(target),
            target_label: pr.branch.clone(),
            captions: [None, None],
            since_fork: true,
            data: None,
            request: 0,
        };
        let Some(hosting) = self.hosting.clone() else {
            self.compare(compare, ctx);
            return;
        };
        self.compare_requests += 1;
        let request = self.compare_requests;
        self.compare = Some(Compare { request, ..compare });
        self.pr_selection_request = Some(request);
        // A persisted or superseded list can carry valid-looking but stale OIDs.
        let use_list_endpoints = !self.prs_from_cache && self.pending_prs.is_none();
        let (path, remotes, credentials) = (
            self.path.clone(),
            self.remotes.clone(),
            self.credentials.clone(),
        );
        self.forge_then(
            ctx,
            move || {
                let _scope = crate::timing::scope(format!("pr-diff#{request}"));
                let start = std::time::Instant::now();
                let result = match hosting.forge.provider().pages() {
                    Some(pages) => selection_for_pr(
                        &path,
                        &pr,
                        &hosting,
                        &remotes,
                        credentials.as_ref(),
                        use_list_endpoints,
                        || pages.details(&path, &hosting, number),
                    ),
                    None => {
                        remote_selection_data(&path, &pr, &hosting, &remotes, credentials.as_ref())
                    }
                };
                crate::timing::step("PR comparison ready", start);
                result
            },
            move |tab, result, _| tab.pr_selection_arrived(request, result),
        );
    }

    fn focus_pr_head(&mut self, hash: &str) {
        if let Some(row) = self.index.get(hash).copied() {
            self.selected = Some(row);
            self.scroll_to = Some(row);
        }
        self.stash_view = None;
    }

    fn pr_selection_arrived(&mut self, request: u64, result: Result<Selection, String>) {
        if self.pr_selection_request == Some(request) {
            self.pr_selection_request = None;
        }
        let Some(compare) = self.compare.as_mut().filter(|c| c.request == request) else {
            return;
        };
        match result {
            Ok(selection) => {
                let hash = selection.head.hash.clone();
                compare.base = selection.base;
                compare.target = Some(hash.clone());
                compare.data = Some(Ok(selection.data));
                self.pr_graph_head = Some(Arc::new(selection.head));
                self.rebuild_rows();
                self.focus_pr_head(&hash);
            }
            Err(why) => compare.data = Some(Err(why)),
        }
    }

    /// Adds a tip fetched only for a PR without changing the paged history or any Git refs.
    pub(in crate::repo) fn insert_pr_graph_head(&mut self) {
        let Some(head) = self.pr_graph_head.as_ref().filter(|head| {
            self.compare.as_ref().and_then(|c| c.target.as_deref()) == Some(head.hash.as_str())
                && !self.commits.iter().any(|c| c.hash == head.hash)
        }) else {
            return;
        };
        // Preserve the order of loaded rows. The added tip must follow any children already
        // shown and precede any parents; dates decide only between unrelated rows.
        let after_children = self
            .commits
            .iter()
            .enumerate()
            .filter(|(_, c)| c.parents.contains(&head.hash))
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0);
        let before_parents = self
            .commits
            .iter()
            .position(|c| head.parents.contains(&c.hash))
            .unwrap_or(self.commits.len());
        let start = after_children.max(usize::from(
            self.commits
                .first()
                .is_some_and(|c| c.hash == crate::repo::WIP_HASH),
        ));
        let end = before_parents.max(start);
        let index = self.commits[start..end]
            .iter()
            .position(|c| c.time <= head.time)
            .map_or(end, |i| start + i);
        self.commits.insert(index, Arc::clone(head));
    }

    /// An overlay belongs only to the comparison that selected its head.
    pub(in crate::repo) fn drop_unfocused_pr_head(&mut self) {
        let stale = self.pr_graph_head.as_ref().is_some_and(|head| {
            self.compare.as_ref().and_then(|c| c.target.as_deref()) != Some(head.hash.as_str())
        });
        if stale {
            let selected = self.selected_hash();
            self.pr_graph_head = None;
            self.rebuild_rows();
            self.selected = selected.and_then(|hash| self.index.get(&hash).copied());
        }
    }
}

/// The PR list is already a snapshot of the proposed head. Keep its base from the same
/// snapshot, rather than downloading comments, reviews and threads on every selection.
/// This only supplies diff inputs: review/post/merge metadata still comes from Pages.
fn listed_endpoints(pr: &PullRequest, hosting: &Hosting) -> Option<PrDetails> {
    if hosting.forge != Forge::GitHub
        || !hosting.matches_pull_request(&pr.url)
        || !pull_request::is_oid(&pr.base_oid)
        || !pull_request::is_oid(&pr.sha)
        || !pull_request::is_branch_name(&pr.base)
    {
        return None;
    }
    Some(PrDetails {
        forge: Some(hosting.forge),
        number: pr.number,
        url: pr.url.clone(),
        base_branch: pr.base.clone(),
        base_oid: pr.base_oid.clone(),
        head_branch: pr.branch.clone(),
        head_oid: pr.sha.clone(),
        head_ref: format!("refs/pull/{}/head", pr.number),
        ..PrDetails::default()
    })
}

/// Choose exact comparison endpoints. Trust listed OIDs only when the displayed list is fresh.
fn selection_for_pr(
    path: &Path,
    pr: &PullRequest,
    hosting: &Hosting,
    remotes: &[Remote],
    credentials: Option<&Credentials>,
    use_list_endpoints: bool,
    read_details: impl FnOnce() -> Result<PrDetails, String>,
) -> Result<Selection, String> {
    let start = std::time::Instant::now();
    let details = match (use_list_endpoints, listed_endpoints(pr, hosting)) {
        (true, Some(details)) => {
            crate::timing::step("PR endpoints from list", start);
            details
        }
        _ => {
            let details = read_details()?;
            crate::timing::step("PR endpoints from forge", start);
            details
        }
    };
    selection_data(path, hosting, &details, remotes, credentials)
}

/// Forges whose details open on the web still provide a head SHA and base branch in their
/// list. Read that base on the PR's own remote, never from a possibly divergent local branch.
fn remote_selection_data(
    path: &Path,
    pr: &PullRequest,
    hosting: &Hosting,
    remotes: &[Remote],
    credentials: Option<&Credentials>,
) -> Result<Selection, String> {
    let remote = pull_request::remote_for(remotes, &pr.url, hosting).ok_or(crate::i18n::text(
        "repo-workflows-pr_view-remote-selection-data-60a19c21",
    ))?;
    let base_ref = format!("refs/heads/{}", pr.base);
    git::run(Some(path), &["check-ref-format", &base_ref])?;
    let listed = git::run_with(
        Some(path),
        &["ls-remote", "--", remote, &base_ref],
        credentials,
    )?;
    let base_oid = listed
        .lines()
        .find_map(|line| {
            let (oid, name) = line.split_once('\t')?;
            (name == base_ref).then(|| oid.to_owned())
        })
        .ok_or(crate::i18n::text(
            "repo-workflows-pr_view-remote-selection-data-f41f1bfc",
        ))?;
    let details = PrDetails {
        base_branch: pr.base.clone(),
        base_oid,
        head_oid: pr.sha.clone(),
        url: pr.url.clone(),
        head_ref: match hosting.forge {
            Forge::GitLab => format!("refs/merge-requests/{}/head", pr.number),
            _ => format!("refs/pull/{}/head", pr.number),
        },
        ..PrDetails::default()
    };
    selection_data(path, hosting, &details, remotes, credentials)
}

/// Uses the forge's exact commits, fetching missing objects without changing any branches.
/// Reading the patch here does not mark the pull request as reviewed or open its details.
fn selection_data(
    path: &Path,
    hosting: &Hosting,
    details: &PrDetails,
    remotes: &[Remote],
    credentials: Option<&Credentials>,
) -> Result<Selection, String> {
    let start = std::time::Instant::now();
    let files = pull_request::files(path, hosting, details, remotes, credentials)?;
    crate::timing::step("PR merge base and file list", start);
    let start = std::time::Instant::now();
    let apart = git::count_apart(path, &details.base_oid, &details.head_oid)?;
    crate::timing::step("PR commit distance", start);
    let data = CompareData {
        apart,
        from: files.from,
        to: Some(files.to),
        files: files.files,
    };
    let start = std::time::Instant::now();
    let head = git::commits_of(path, std::slice::from_ref(&details.head_oid))?
        .into_iter()
        .next()
        .ok_or(crate::i18n::text(
            "repo-workflows-pr_view-selection-data-fb954333",
        ))?;
    crate::timing::step("PR head metadata", start);
    Ok(Selection {
        base: details.base_oid.clone(),
        head,
        data,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "select/tests.rs"]
mod tests;
