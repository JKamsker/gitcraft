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

fn selection_for_pr(
    path: &Path,
    pr: &PullRequest,
    hosting: &Hosting,
    remotes: &[Remote],
    credentials: Option<&Credentials>,
    read_details: impl FnOnce() -> Result<PrDetails, String>,
) -> Result<Selection, String> {
    let start = std::time::Instant::now();
    let details = match listed_endpoints(pr, hosting) {
        Some(details) => {
            crate::timing::step("PR endpoints from list", start);
            details
        }
        None => {
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
mod tests {
    use super::*;

    fn github() -> Hosting {
        Hosting {
            forge: Forge::GitHub,
            host: "github.com".into(),
            path: "o/r".into(),
            web: "https://github.com".into(),
        }
    }

    fn listed_pr(base: String, head: String) -> PullRequest {
        PullRequest {
            number: 7,
            title: String::new(),
            branch: "feature".into(),
            base: "main".into(),
            base_oid: base,
            sha: head,
            from_fork: true,
            author: String::new(),
            url: "https://github.com/o/r/pull/7".into(),
            draft: false,
        }
    }

    #[test]
    fn unknown_or_mismatched_list_endpoints_retain_forge_lookup() {
        let original = listed_pr("a".repeat(40), "b".repeat(40));
        assert!(listed_endpoints(&original, &github()).is_some());
        for (base, head, branch, url) in [
            ("", original.sha.as_str(), "main", original.url.as_str()),
            (
                original.base_oid.as_str(),
                "short",
                "main",
                original.url.as_str(),
            ),
            (
                original.base_oid.as_str(),
                original.sha.as_str(),
                "--invalid",
                original.url.as_str(),
            ),
            (
                original.base_oid.as_str(),
                original.sha.as_str(),
                "main",
                "https://github.com/other/repo/pull/7",
            ),
        ] {
            let pr = PullRequest {
                base_oid: base.into(),
                sha: head.into(),
                base: branch.into(),
                url: url.into(),
                ..original.clone()
            };
            assert!(listed_endpoints(&pr, &github()).is_none());
        }
        let mut other = github();
        other.forge = Forge::GitLab;
        assert!(listed_endpoints(&original, &other).is_none());
    }

    /// Read-only benchmark of an explicitly selected immutable PR snapshot already local.
    #[test]
    #[ignore = "set GITCRAFT_PR_DIFF_REPO, GITCRAFT_PR_DIFF_BASE and GITCRAFT_PR_DIFF_HEAD"]
    fn local_pr_selection_repository_benchmark() -> Result<(), String> {
        let path = std::env::var_os("GITCRAFT_PR_DIFF_REPO").ok_or("missing repository")?;
        let repo = Path::new(&path);
        let pr = listed_pr(
            std::env::var("GITCRAFT_PR_DIFF_BASE").map_err(|e| e.to_string())?,
            std::env::var("GITCRAFT_PR_DIFF_HEAD").map_err(|e| e.to_string())?,
        );
        // Refuse fetching in this benchmark: both endpoints must already exist locally.
        git::resolve(repo, &pr.base_oid)?;
        git::resolve(repo, &pr.sha)?;
        let hosting = github();
        let start = std::time::Instant::now();
        let expected = selection_data(
            repo,
            &hosting,
            &listed_endpoints(&pr, &hosting).ok_or("invalid snapshot")?,
            &[],
            None,
        )?;
        println!(
            "first local PR comparison: {:.1} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        for i in 0..3 {
            git::backend::reset_metrics();
            let start = std::time::Instant::now();
            let actual = selection_for_pr(repo, &pr, &hosting, &[], None, || {
                Err("PR diff must not request metadata".into())
            })?;
            let elapsed = start.elapsed();
            assert_eq!(actual.base, expected.base);
            assert_eq!(actual.head.hash, expected.head.hash);
            assert_eq!(actual.data.from, expected.data.from);
            assert_eq!(actual.data.to, expected.data.to);
            assert_eq!(actual.data.apart, expected.data.apart);
            let file_keys = |selection: &Selection| {
                selection
                    .data
                    .files
                    .iter()
                    .map(|f| {
                        (
                            f.status,
                            f.path.clone(),
                            f.old_path.clone(),
                            f.added,
                            f.removed,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(file_keys(&actual), file_keys(&expected));
            let processes: u64 = git::backend::metrics().iter().map(|m| m.subprocesses).sum();
            println!(
                "local PR selection {i}: {:.1} ms, {} files, {processes} Git processes, zero forge requests",
                elapsed.as_secs_f64() * 1000.0,
                actual.data.files.len()
            );
        }
        Ok(())
    }

    fn comparison(request: u64) -> Compare {
        Compare {
            base: "main".into(),
            base_label: "main".into(),
            target: Some("head".into()),
            target_label: "feature".into(),
            captions: [None, None],
            since_fork: true,
            data: None,
            request,
        }
    }

    #[test]
    fn a_pending_unknown_pr_head_survives_refresh_and_is_focused_without_changing_history() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pr-pending".into(), &ctx);
        tab.life.shown(std::time::Instant::now());
        let base = Arc::new(git::Commit {
            hash: "base".into(),
            time: 10,
            ..git::Commit::default()
        });
        tab.history = vec![Arc::clone(&base)];
        tab.refs.head = "base".into();
        tab.compare = Some(comparison(7));
        tab.compare_requests = 7;
        tab.pr_selection_request = Some(7);
        tab.versions.seen_heads.insert(7, "previously-seen".into());
        tab.rebuild(&ctx);
        tab.reload_compare(&ctx);
        assert_eq!(tab.compare.as_ref().unwrap().request, 7);
        assert_eq!(tab.compare_requests, 7);
        tab.pr_selection_arrived(
            7,
            Ok(Selection {
                base: "base".into(),
                head: git::Commit {
                    hash: "head".into(),
                    parents: vec!["base".into()],
                    time: 1,
                    ..git::Commit::default()
                },
                data: CompareData {
                    from: "base".into(),
                    to: Some("head".into()),
                    files: Vec::new(),
                    apart: (0, 1),
                },
            }),
        );
        assert_eq!(tab.selected_hash().as_deref(), Some("head"));
        assert_eq!(tab.scroll_to, tab.selected);
        assert!(
            tab.index["head"] < tab.index["base"],
            "parent follows the tip even when its date is newer"
        );
        assert_eq!(tab.history.len(), 1, "paged history is unchanged");
        assert_eq!(
            tab.versions.seen_heads.get(&7).map(String::as_str),
            Some("previously-seen")
        );
        tab.apply(crate::repo::Action::CloseCompare, &ctx);
        assert!(!tab.index.contains_key("head"));
        assert!(tab.pr_graph_head.is_none());
    }

    #[test]
    fn a_previous_pr_selection_cannot_replace_a_new_comparison_or_details() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pr-selection".into(), &ctx);
        tab.compare = Some(comparison(2));
        tab.pr_selection_arrived(1, Err("old answer".into()));
        assert!(tab.compare.as_ref().unwrap().data.is_none());
        tab.compare = None; // Opening details (or selecting another view) closes it.
        tab.pr_selection_arrived(2, Err("late answer".into()));
        assert!(tab.compare.is_none());
    }

    #[test]
    fn a_pr_patch_contains_every_commit_and_excludes_changes_only_on_the_base() {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-pr-selection-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["checkout", "-q", "-b", "feature"],
        ] {
            git::run(Some(&dir), args).unwrap();
        }
        let common = git::head(&dir);
        for file in ["first.txt", "second.txt"] {
            std::fs::write(dir.join(file), file).unwrap();
            git::run(Some(&dir), &["add", file]).unwrap();
            git::run(Some(&dir), &["commit", "-q", "-m", file]).unwrap();
        }
        let head = git::head(&dir);
        git::run(Some(&dir), &["checkout", "-q", "main"]).unwrap();
        std::fs::write(dir.join("base-only.txt"), "main").unwrap();
        git::run(Some(&dir), &["add", "."]).unwrap();
        git::run(Some(&dir), &["commit", "-q", "-m", "main moved"]).unwrap();
        let details = PrDetails {
            base_branch: "main".into(),
            base_oid: git::head(&dir),
            head_oid: head.clone(),
            ..PrDetails::default()
        };
        let hosting = Hosting {
            forge: Forge::GitHub,
            host: "github.com".into(),
            path: "o/r".into(),
            web: "https://github.com".into(),
        };
        let mut pr = PullRequest {
            number: 7,
            branch: "feature".into(),
            base: "main".into(),
            base_oid: details.base_oid.clone(),
            sha: head.clone(),
            title: String::new(),
            author: String::new(),
            from_fork: true,
            draft: false,
            url: "https://github.com/o/r/pull/7".into(),
        };
        // A local base may have unpublished changes. Use the list's exact snapshot.
        git::run(
            Some(&dir),
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "unpublished local base",
            ],
        )
        .unwrap();
        let selection = selection_for_pr(&dir, &pr, &hosting, &[], None, || {
            Err("known local PR endpoints must not request forge details".into())
        })
        .unwrap();
        assert_eq!(selection.base, details.base_oid);
        let data = selection.data;
        assert_eq!(selection.head.hash, head);
        assert_eq!(data.from, common);
        let paths: Vec<_> = data.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["first.txt", "second.txt"]);
        assert_eq!(data.apart, (1, 2));
        pr.base_oid = git::head(&dir);
        let changed_base = selection_for_pr(&dir, &pr, &hosting, &[], None, || {
            Err("unexpected forge request".into())
        })
        .unwrap();
        assert_eq!(changed_base.data.apart, (2, 2));
        assert_eq!(changed_base.base, pr.base_oid);
        pr.base_oid.clear();
        let calls = std::cell::Cell::new(0);
        let fallback = selection_for_pr(&dir, &pr, &hosting, &[], None, || {
            calls.set(calls.get() + 1);
            Ok(details.clone())
        })
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(fallback.base, details.base_oid);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_gitlab_pr_fetches_an_unknown_head_and_uses_the_remote_base() {
        unknown_head_selection(Forge::GitLab);
    }

    #[test]
    fn listed_github_endpoints_fetch_only_missing_objects_without_a_details_request() {
        unknown_head_selection(Forge::GitHub);
    }

    fn unknown_head_selection(forge: Forge) {
        let root =
            std::env::temp_dir().join(format!("gitcraft-pr-{forge:?}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("remote");
        let clone = root.join("clone");
        std::fs::create_dir_all(&source).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["checkout", "-q", "-b", "feature"],
        ] {
            git::run(Some(&source), args).unwrap();
        }
        std::fs::write(source.join("feature.txt"), "proposed\n").unwrap();
        git::run(Some(&source), &["add", "."]).unwrap();
        git::run(Some(&source), &["commit", "-q", "-m", "feature"]).unwrap();
        let head = git::head(&source);
        let head_ref = if forge == Forge::GitHub {
            "refs/pull/7/head"
        } else {
            "refs/merge-requests/7/head"
        };
        git::run(Some(&source), &["update-ref", head_ref, &head]).unwrap();
        git::run(Some(&source), &["checkout", "-q", "main"]).unwrap();
        std::fs::write(source.join("base-only.txt"), "remote base\n").unwrap();
        git::run(Some(&source), &["add", "."]).unwrap();
        git::run(Some(&source), &["commit", "-q", "-m", "base moved"]).unwrap();
        let remote_base = git::head(&source);
        git::run(
            Some(&root),
            &[
                "clone",
                "-q",
                "--no-local",
                "--single-branch",
                "--branch",
                "main",
                "remote",
                "clone",
            ],
        )
        .unwrap();
        for args in [
            &["config", "user.name", "t"][..],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "local main diverged"],
        ] {
            git::run(Some(&clone), args).unwrap();
        }
        let local_head = git::head(&clone);
        assert!(git::run(Some(&clone), &["cat-file", "-e", &head]).is_err());
        let host = if forge == Forge::GitHub {
            "github.example"
        } else {
            "gitlab.example"
        };
        let url = if forge == Forge::GitHub {
            format!("https://{host}/team/repo/pull/7")
        } else {
            format!("https://{host}/team/repo/-/merge_requests/7")
        };
        let pr = PullRequest {
            number: 7,
            branch: "feature".into(),
            base: "main".into(),
            base_oid: remote_base.clone(),
            sha: head.clone(),
            title: String::new(),
            author: String::new(),
            from_fork: false,
            draft: false,
            url,
        };
        // The actual origin is local; the metadata names the URL associated with the PR.
        let remotes = [Remote {
            name: "origin".into(),
            url: format!("https://{host}/team/repo.git"),
        }];
        let hosting = Hosting {
            forge,
            host: host.into(),
            path: "team/repo".into(),
            web: format!("https://{host}"),
        };
        let selection = if forge == Forge::GitHub {
            selection_for_pr(&clone, &pr, &hosting, &remotes, None, || {
                Err("known PR endpoints must fetch objects without requesting metadata".into())
            })
        } else {
            remote_selection_data(&clone, &pr, &hosting, &remotes, None)
        }
        .unwrap();
        assert_eq!(selection.base, remote_base);
        assert_eq!(selection.head.hash, head);
        assert_eq!(
            selection
                .data
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["feature.txt"]
        );
        assert_eq!(
            git::head(&clone),
            local_head,
            "checkout and local branch are unchanged"
        );
        assert!(
            git::run(Some(&clone), &["for-each-ref", "refs/gitcraft/reviewed"])
                .unwrap()
                .is_empty()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
