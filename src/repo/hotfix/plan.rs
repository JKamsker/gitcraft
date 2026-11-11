//! The git side of the hotfix flow, as git-flow does it: a `hotfix/2.3.2` branch from the tag
//! `v2.3.1`, then the tag `v2.3.2` on it, merged back into the main branch (and the development
//! one, if used), the branch deleted, everything pushed.

#[path = "plan/recovery.rs"]
mod recovery;
#[path = "plan/steps.rs"]
mod steps;
#[allow(unused_imports)] // Compatibility re-export.
pub(crate) use recovery::Now;
use recovery::upstream_back;
pub(crate) use recovery::{now, recover};
use std::path::Path;

use crate::git;
use crate::release::conventional::Change;
use crate::release::version::Version;
use crate::repo::operations::{expect_ref, expect_ref_at, expect_symref};

/// The branches the flow uses: git-flow's settings when it is set up, else `main` (or `master`)
/// and `develop` as they exist here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Names {
    /// The hotfix branches' prefix, `hotfix/`.
    pub prefix: String,
    pub main: Option<String>,
    pub develop: Option<String>,
    /// The version tags' prefix git-flow is set up with (`gitflow.prefix.versiontag`).
    pub version_tag: Option<String>,
}

/// Reads [`Names`] (`gitflow.prefix.hotfix`, `gitflow.branch.master`, `gitflow.branch.develop`).
pub(crate) fn names(repo: &Path) -> Names {
    let config = |key: &str| {
        git::run_ok_codes(Some(repo), &["config", "--get", key], &[0, 1])
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let exists = |b: &str| git::resolve(repo, &format!("refs/heads/{b}")).is_ok();
    let main = config("gitflow.branch.master")
        .or_else(|| config("gitflow.branch.main"))
        .filter(|b| exists(b))
        .or_else(|| {
            ["main", "master"]
                .into_iter()
                .find(|b| exists(b))
                .map(str::to_owned)
        });
    let develop = config("gitflow.branch.develop")
        .filter(|b| exists(b))
        .or_else(|| exists("develop").then(|| "develop".to_owned()));
    Names {
        prefix: config("gitflow.prefix.hotfix").unwrap_or_else(|| "hotfix/".to_owned()),
        // git-flow can be set up with both the same.
        develop: develop.filter(|d| main.as_deref() != Some(d.as_str())),
        main,
        version_tag: config("gitflow.prefix.versiontag"),
    }
}

/// The branch a hotfix from `tag` starts as: `v2.3.1` → `hotfix/2.3.2`.
pub(crate) fn start_name(tag: &str, prefix: &str) -> Option<String> {
    let v = Version::parse(tag)?;
    let next = v.next(crate::release::version::Bump::Patch);
    Some(format!("{prefix}{}", next.number()))
}

/// The tag a hotfix branch ends with: `hotfix/2.3.2` → `v2.3.2`, its prefix git-flow's
/// (`versiontag`), else as `like`'s (the tag it started from), else `v`.
pub(crate) fn finish_tag(
    branch: &str,
    prefix: &str,
    version_tag: Option<&str>,
    like: Option<&str>,
) -> String {
    let rest = branch.strip_prefix(prefix).unwrap_or(branch);
    let tag_prefix = version_tag.map(str::to_owned).unwrap_or_else(|| {
        like.and_then(Version::parse)
            .map_or_else(|| "v".to_owned(), |v| v.prefix)
    });
    match Version::parse(rest) {
        Some(v) => format!("{tag_prefix}{}", v.number()),
        None => rest.to_owned(),
    }
}

/// A branch and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct At {
    pub name: String,
    pub head: String,
    /// Where it goes on the remote (`refs/heads/main`): its upstream there, else its name.
    pub remote_ref: String,
    /// Its upstream as configured (`branch.<name>.remote`, `.merge`), put back with it.
    pub upstream: Option<(String, String)>,
}

/// What finishing the hotfix checked out starts from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Finish {
    pub hotfix: At,
    /// The version tag it started from, if any.
    pub base_tag: Option<String>,
    pub tag: String,
    pub main: At,
    pub develop: Option<At>,
    pub changes: Vec<Change>,
    /// Where merging it would stop on conflicts: (branch, files); `None` when this git can't
    /// tell beforehand (older than 2.38).
    pub conflicts: Option<Vec<(String, Vec<String>)>>,
    pub remote: Option<String>,
    pub tags: Vec<String>,
    /// git signs tags by default (`tag.gpgSign`).
    pub sign: bool,
}

/// What the dialog lets the user choose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choices {
    pub tag: String,
    pub notes: String,
    pub develop: bool,
    pub delete: bool,
    pub push: bool,
    pub sign: bool,
}

/// Reads what finishing the hotfix checked out starts from, or why it can't be finished.
pub(crate) fn plan(repo: &Path, remote: Option<String>) -> Result<Finish, String> {
    let names = names(repo);
    let full = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    let branch = full
        .trim()
        .strip_prefix("refs/heads/")
        .filter(|b| b.starts_with(&names.prefix))
        .ok_or_else(|| {
            crate::i18n::message(
                "repo-workflows-hotfix-plan-a798cdeb",
                &[("names-prefix-1", (names.prefix).clone().into())],
            )
        })?
        .to_owned();
    let main_name = names
        .main
        .clone()
        .ok_or(crate::i18n::text("repo-workflows-hotfix-plan-97906966"))?;
    let dirty = git::run(
        Some(repo),
        &["status", "--porcelain", "--untracked-files=no"],
    )?;
    if !dirty.trim().is_empty() {
        return Err(crate::i18n::text("repo-workflows-hotfix-plan-e373acd0").into());
    }
    let config = |key: String| {
        git::run_ok_codes(Some(repo), &["config", "--get", &key], &[0, 1])
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let at = |name: &str| -> Result<At, String> {
        Ok(At {
            name: name.to_owned(),
            head: git::resolve(repo, &format!("refs/heads/{name}"))?,
            remote_ref: upstream_ref(repo, name, remote.as_deref()),
            upstream: config(format!("branch.{name}.remote"))
                .zip(config(format!("branch.{name}.merge"))),
        })
    };
    let hotfix = at(&branch)?;
    let main = at(&main_name)?;
    let develop = match &names.develop {
        Some(d) => Some(at(d)?),
        None => None,
    };
    // Checked out in another worktree, a branch can't be checked out here to merge into.
    let worktrees = git::run(Some(repo), &["worktree", "list", "--porcelain"])?;
    for t in std::iter::once(&main).chain(develop.as_ref()) {
        let line = format!("branch refs/heads/{}", t.name);
        if worktrees.lines().any(|l| l == line) {
            return Err(crate::i18n::message(
                "repo-workflows-hotfix-plan-eeb1f496",
                &[("t-name-1", (t.name).clone().into())],
            ));
        }
    }
    let base_tag = git::run_ok_codes(
        Some(repo),
        &[
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            "v[0-9]*",
            "--match",
            "[0-9]*",
            &hotfix.head,
        ],
        &[0, 128],
    )
    .ok()
    .map(|s| s.trim().to_owned())
    .filter(|s| !s.is_empty());
    let range = match &base_tag {
        Some(tag) => format!("refs/tags/{tag}..{}", hotfix.head),
        None => format!("{}..{}", main.head, hotfix.head),
    };
    let log = git::run(
        Some(repo),
        &[
            "log",
            "--no-merges",
            "--format=%H%x1f%s%x1f%b%x1e",
            &range,
            "--",
        ],
    )?;
    let changes: Vec<Change> = log
        .split('\u{1e}')
        .filter_map(|r| {
            let mut f = r.trim_start_matches('\n').splitn(3, '\u{1f}');
            let hash = f.next()?.trim();
            (!hash.is_empty()).then(|| {
                Change::parse(
                    hash,
                    f.next().unwrap_or_default(),
                    f.next().unwrap_or_default(),
                )
            })
        })
        .collect();
    if changes.is_empty() {
        return Err(crate::i18n::message(
            "repo-workflows-hotfix-plan-8f734f01",
            &[("branch", (branch).clone().into())],
        ));
    }
    let conflicts = std::iter::once(&main)
        .chain(develop.as_ref())
        .map(|into| merge_conflicts(repo, &into.head, &hotfix.head).map(|f| (into.name.clone(), f)))
        .collect::<Option<Vec<_>>>()
        .map(|list| list.into_iter().filter(|(_, f)| !f.is_empty()).collect());
    let tags = git::run(Some(repo), &["tag", "--list"])?
        .lines()
        .map(str::to_owned)
        .collect();
    let sign = git::run_ok_codes(
        Some(repo),
        &["config", "--bool", "--get", "tag.gpgSign"],
        &[0, 1],
    )
    .is_ok_and(|v| v.trim() == "true");
    Ok(Finish {
        tag: finish_tag(
            &branch,
            &names.prefix,
            names.version_tag.as_deref(),
            base_tag.as_deref(),
        ),
        hotfix,
        base_tag,
        main,
        develop,
        changes,
        conflicts,
        remote,
        tags,
        sign,
    })
}

/// Where `branch` goes on `remote`: its upstream there, else the branch of its name.
fn upstream_ref(repo: &Path, branch: &str, remote: Option<&str>) -> String {
    let out = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(upstream:remotename)%00%(upstream:remoteref)",
            &format!("refs/heads/{branch}"),
        ],
    )
    .unwrap_or_default();
    let mut parts = out.trim().split('\0');
    match (parts.next(), parts.next()) {
        (Some(r), Some(dst)) if Some(r) == remote && !dst.is_empty() => dst.to_owned(),
        _ => format!("refs/heads/{branch}"),
    }
}

/// The files merging `theirs` into `ours` would stop on, without touching anything (`git
/// merge-tree --write-tree`, git 2.38 or later); `None` when this git can't tell.
fn merge_conflicts(repo: &Path, ours: &str, theirs: &str) -> Option<Vec<String>> {
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            ours,
            theirs,
        ],
        &[0, 1],
    )
    .ok()?;
    // The tree first, then the conflicted files.
    Some(
        out.lines()
            .skip(1)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

fn merge_message(hotfix: &str, into: &str, main: &str) -> String {
    if into == main {
        format!("Merge branch '{hotfix}'")
    } else {
        format!("Merge branch '{hotfix}' into {into}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotfix_names_and_tags() {
        assert_eq!(
            start_name("v2.3.1", "hotfix/").as_deref(),
            Some("hotfix/2.3.2")
        );
        assert_eq!(start_name("2.3.1", "hf-").as_deref(), Some("hf-2.3.2"));
        assert_eq!(start_name("nightly", "hotfix/"), None);
        assert_eq!(
            finish_tag("hotfix/2.3.2", "hotfix/", None, Some("v2.3.1")),
            "v2.3.2"
        );
        assert_eq!(
            finish_tag("hotfix/2.3.2", "hotfix/", None, Some("2.3.1")),
            "2.3.2"
        );
        assert_eq!(
            finish_tag("hotfix/2.3.2", "hotfix/", Some("rel-"), Some("v2.3.1")),
            "rel-2.3.2"
        );
        assert_eq!(finish_tag("hotfix/urgent", "hotfix/", None, None), "urgent");
    }
}
