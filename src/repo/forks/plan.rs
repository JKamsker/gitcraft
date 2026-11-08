//! The git side of pull requests from forks: checking a fork's branch out so that pushing goes
//! back to it, rebasing it onto its base and pushing it to the contributor, and the remotes made
//! for forks that nothing uses any more.

use std::path::Path;

use crate::forge::ForkInfo;
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

/// The steps that check the fork's branch of `info` out so that a push goes back to it, how to
/// undo them, and the local branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Checkout {
    pub remote: String,
    /// Whether the remote is added (none had the fork's URL).
    pub new_remote: bool,
    pub url: String,
    pub local: String,
    pub steps: Vec<Vec<String>>,
    pub undo: Vec<Vec<String>>,
}

/// Whether two remote URLs are the same repository, however they're written.
fn same_repository(a: &str, b: &str) -> bool {
    match (crate::forge::location(a), crate::forge::location(b)) {
        (Some((ha, pa)), Some((hb, pb))) => {
            ha.eq_ignore_ascii_case(&hb) && pa.eq_ignore_ascii_case(&pb)
        }
        _ => a.trim_end_matches('/') == b.trim_end_matches('/'),
    }
}

/// Whether a remote URL is reached over SSH: `ssh://…`, or `user@host:path`.
fn is_ssh(url: &str) -> bool {
    url.starts_with("ssh://")
        || (!url.contains("://")
            && url
                .split_once(':')
                .is_some_and(|(left, _)| left.contains('@') && !left.contains('/')))
}

/// The remote that is `info`'s fork (by its URL, however written), if there is one.
pub(crate) fn fork_remote(info: &ForkInfo, remotes: &[Remote]) -> Option<String> {
    remotes
        .iter()
        .find(|r| {
            (!info.clone_https.is_empty() && same_repository(&r.url, &info.clone_https))
                || (!info.clone_ssh.is_empty() && same_repository(&r.url, &info.clone_ssh))
        })
        .map(|r| r.name.clone())
}

/// A remote name made of `owner`: what git takes in a name, nothing else.
fn remote_name(owner: &str) -> String {
    let name: String = owner
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name
        .replace("..", "-")
        .trim_matches(['-', '.'])
        .trim_end_matches(".lock")
        .to_owned();
    if name.is_empty() {
        "fork".to_owned()
    } else {
        name
    }
}

/// Plans checking out `info`'s branch to push to it: through the remote that already is the
/// fork, else one added for it (named after its owner), with the fork's URL of the same kind as
/// `origin`'s (SSH or HTTPS). The local branch is named as the fork's, unless that name is taken
/// by another branch: then `owner/branch`, which pushes to the fork's name all the same.
/// `previous`: what is checked out now, for undoing.
pub(crate) fn checkout(
    info: &ForkInfo,
    remotes: &[Remote],
    setup: &Setup,
    locals: &[String],
    previous: &str,
) -> Result<Checkout, String> {
    if !info.cross {
        return Err(crate::i18n::message(
            "repo-workflows-forks-checkout-13fc08d7",
            &[("info-number-1", (info.number).to_string().into())],
        ));
    }
    if !info.maintainers_can_edit {
        return Err(crate::i18n::message(
            "repo-workflows-forks-checkout-625f9e12",
            &[
                ("info-owner-1", (info.owner).clone().into()),
                ("info-number-2", (info.number).to_string().into()),
            ],
        ));
    }
    if !plain_branch(&info.branch) {
        return Err(crate::i18n::message(
            "repo-workflows-forks-checkout-00c3592c",
            &[
                ("info-number-1", (info.number).to_string().into()),
                ("info-branch-2", (info.branch).clone().into()),
            ],
        ));
    }
    let origin = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or_else(|| remotes.first());
    let ssh = origin.is_some_and(|r| is_ssh(&r.url));
    let url = if ssh && !info.clone_ssh.is_empty() {
        info.clone_ssh.clone()
    } else {
        info.clone_https.clone()
    };
    if url.is_empty() {
        return Err(crate::i18n::message(
            "repo-workflows-forks-checkout-43a1fda6",
            &[("info-owner-1", (info.owner).clone().into())],
        ));
    }
    let (remote, new_remote) = match fork_remote(info, remotes) {
        Some(r) => (r, false),
        None => {
            let base = remote_name(&info.owner);
            let taken = |n: &str| remotes.iter().any(|r| r.name == n);
            let name = std::iter::once(base.clone())
                .chain(std::iter::once(format!("{base}-fork")))
                .chain((2..100).map(|i| format!("{base}-fork-{i}")))
                .find(|n| !taken(n))
                .unwrap_or_else(|| format!("{base}-fork-{}", info.number));
            (name, true)
        }
    };
    let tracking = format!("refs/remotes/{remote}/{}", info.branch);
    let fetch = strings(&[
        "fetch",
        "--",
        &remote,
        &format!("+refs/heads/{}:{tracking}", info.branch),
    ]);
    let mut steps = Vec::new();
    let mut undo = Vec::new();
    if new_remote {
        steps.push(strings(&["remote", "add", "--", &remote, &url]));
        steps.push(strings(&[
            "config",
            &format!("remote.{remote}.{MARK}"),
            "true",
        ]));
    }
    steps.push(fetch);
    // Already set up for it: just check it out.
    if let Some(b) = setup.pushing_to(&remote, &info.branch) {
        let local = b.name.clone();
        steps.push(strings(&["checkout", &local, "--"]));
        undo.push(strings(&["checkout", previous, "--"]));
        return Ok(Checkout {
            remote,
            new_remote,
            url,
            local,
            steps,
            undo,
        });
    }
    // Free, and no branch is a folder of it nor it a folder of one (`fix` and `fix/a`).
    let free = |n: &str| {
        !locals
            .iter()
            .any(|l| l == n || l.starts_with(&format!("{n}/")) || n.starts_with(&format!("{l}/")))
    };
    let local = [
        info.branch.clone(),
        format!("{}/{}", remote_name(&info.owner), info.branch),
        format!("pr-{}", info.number),
    ]
    .into_iter()
    .find(|n| free(n))
    .ok_or_else(|| {
        crate::i18n::message(
            "repo-workflows-forks-checkout-d005bf20",
            &[
                ("info-branch-1", (info.branch).clone().into()),
                ("info-owner-2", (info.owner).clone().into()),
                ("info-branch-3", (info.branch).clone().into()),
                ("info-number-4", info.number.to_string().into()),
            ],
        )
    })?;
    steps.push(strings(&["checkout", "-b", &local, "--track", &tracking]));
    steps.push(strings(&[
        "config",
        &format!("branch.{local}.pushRemote"),
        &remote,
    ]));
    // Back to what was checked out, the branch deleted only while it has nothing new.
    undo.push(strings(&["checkout", previous, "--"]));
    undo.push(expect_ref(&format!("refs/heads/{local}")));
    undo.push(strings(&["branch", "-D", "--", &local]));
    if new_remote {
        undo.push(strings(&["remote", "remove", "--", &remote]));
    }
    Ok(Checkout {
        remote,
        new_remote,
        url,
        local,
        steps,
        undo,
    })
}

/// Whether `name` (a branch name a forge gave) is one git takes as it is, never an option.
fn plain_branch(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
}

/// Rebasing the contributor's branch onto its base and pushing it back to them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rebase {
    pub local: String,
    pub fork: String,
    /// The branch in the fork.
    pub branch: String,
    /// `origin/main`: the base as fetched.
    pub onto: String,
    pub head: String,
    /// Where the fork's branch was last fetched, which the push expects to replace.
    pub seen: String,
    pub tracking: String,
    /// The commits the base has that the branch hasn't.
    pub behind: usize,
}

/// Plans rebasing the checked-out branch, set up for `info`'s fork, onto `base` of `upstream`
/// (fetched first, as is the fork's branch). Refused when the fork's branch has commits the
/// branch hasn't (the push would throw them away), or with changes not committed.
pub(crate) fn plan_rebase(
    repo: &Path,
    info: &ForkInfo,
    upstream: &str,
    setup: &Setup,
    creds: Option<&git::Credentials>,
) -> Result<Rebase, String> {
    let remotes = git::remotes(repo)?;
    let fork = fork_remote(info, &remotes).ok_or_else(|| {
        crate::i18n::message(
            "repo-workflows-forks-plan-rebase-b597bd2f",
            &[
                ("info-number-1", (info.number).to_string().into()),
                ("info-owner-2", (info.owner).clone().into()),
            ],
        )
    })?;
    let full = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    let local = full
        .trim()
        .strip_prefix("refs/heads/")
        .unwrap_or_default()
        .to_owned();
    // The branch checked out must be the one pushing to this pull request's fork: another
    // fork's branch of the same name is someone else's.
    setup
        .pushing_to(&fork, &info.branch)
        .filter(|b| b.name == local)
        .ok_or_else(|| {
            crate::i18n::message(
                "repo-workflows-forks-plan-rebase-a0b078e0",
                &[
                    ("info-number-1", (info.number).to_string().into()),
                    ("info-owner-2", (info.owner).clone().into()),
                ],
            )
        })?;
    let dirty = git::run(
        Some(repo),
        &["status", "--porcelain", "--untracked-files=no"],
    )?;
    if !dirty.trim().is_empty() {
        return Err(crate::i18n::text("repo-workflows-forks-plan-rebase-e28bbaf9").to_owned());
    }
    let tracking = format!("refs/remotes/{fork}/{}", info.branch);
    let onto = format!("refs/remotes/{upstream}/{}", info.base);
    git::run_with(
        Some(repo),
        &[
            "fetch",
            "--",
            fork.as_str(),
            &format!("+refs/heads/{}:{tracking}", info.branch),
        ],
        creds,
    )?;
    git::run_with(
        Some(repo),
        &[
            "fetch",
            "--",
            upstream,
            &format!("+refs/heads/{}:{onto}", info.base),
        ],
        creds,
    )?;
    let head = git::head(repo);
    let seen = git::resolve(repo, &tracking)?;
    let missing = count(repo, &format!("{head}..{seen}"))?;
    if missing > 0 {
        return Err(crate::i18n::message(
            "repo-workflows-fork-unpulled-commits",
            &[
                ("fork", (fork.as_str()).into()),
                ("branch", (info.branch.as_str()).into()),
                ("count", (missing).into()),
                ("local", (local.as_str()).into()),
            ],
        ));
    }
    let behind = count(repo, &format!("{head}..{onto}"))?;
    if behind == 0 {
        return Err(crate::i18n::message(
            "repo-workflows-forks-plan-rebase-6ccf1bc5",
            &[
                ("local", (local).clone().into()),
                ("upstream", (upstream).clone().into()),
                ("info-base-1", (info.base).clone().into()),
            ],
        ));
    }
    Ok(Rebase {
        local,
        fork,
        branch: info.branch.clone(),
        onto,
        head,
        seen,
        tracking,
        behind,
    })
}

fn count(repo: &Path, range: &str) -> Result<usize, String> {
    git::run(Some(repo), &["rev-list", "--count", range, "--"])?
        .trim()
        .parse::<usize>()
        .map_err(|e| {
            crate::i18n::message(
                "repo-workflows-forks-count-082131a5",
                &[("e", e.to_string().into())],
            )
        })
}

impl Rebase {
    fn local_ref(&self) -> String {
        format!("refs/heads/{}", self.local)
    }

    fn rebase(&self) -> Vec<String> {
        strings(&["-c", "rebase.updateRefs=false", "rebase", &self.onto])
    }

    /// The rebase and the push, only while the branch is where the plan saw it.
    pub(crate) fn steps(&self) -> Vec<Vec<String>> {
        let local = self.local_ref();
        vec![
            expect_symref("HEAD", &local),
            expect_ref_at(&local, &self.head),
            self.rebase(),
            self.push(&local, Some(&self.seen)),
        ]
    }

    /// The push of `what` to the fork's branch, replacing it only if it is `seen` (or, without
    /// one, what was last fetched of it).
    fn push(&self, what: &str, seen: Option<&str>) -> Vec<String> {
        let target = format!("refs/heads/{}", self.branch);
        let lease = match seen {
            Some(seen) => format!("--force-with-lease={target}:{seen}"),
            None => format!("--force-with-lease={target}"),
        };
        strings(&[
            "push",
            &lease,
            "--",
            &self.fork,
            &format!("{what}:{target}"),
        ])
    }

    /// The push alone, of the branch as it is now (after a rebase continued past conflicts, or
    /// to retry after signing in): replacing the fork's branch only if it is still `seen`.
    pub(crate) fn push_now(&self) -> Vec<Vec<String>> {
        vec![self.push(&self.local_ref(), Some(&self.seen))]
    }

    /// The fork's branch back as it was, only while it is what was pushed.
    pub(crate) fn fork_undo_steps(&self) -> Vec<Vec<String>> {
        vec![expect_ref(&self.tracking), self.push(&self.head, None)]
    }

    /// Back as it was: on the fork first, then here.
    pub(crate) fn undo_steps(&self) -> Vec<Vec<String>> {
        let local = self.local_ref();
        vec![
            expect_symref("HEAD", &local),
            expect_ref(&local),
            expect_ref(&self.tracking),
            self.push(&self.head, None),
            strings(&["reset", "--keep", &self.head]),
        ]
    }

    pub(crate) fn redo_steps(&self) -> Vec<Vec<String>> {
        let local = self.local_ref();
        vec![
            expect_symref("HEAD", &local),
            expect_ref(&local),
            self.rebase(),
            self.push(&local, None),
        ]
    }

    /// The undo of a rebase whose push failed: the branch back, here only.
    pub(crate) fn local_undo(&self, now: &str) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
        let local = self.local_ref();
        (
            vec![
                expect_symref("HEAD", &local),
                expect_ref_at(&local, now),
                strings(&["reset", "--keep", &self.head]),
            ],
            vec![
                expect_symref("HEAD", &local),
                expect_ref_at(&local, &self.head),
                strings(&["reset", "--keep", now]),
            ],
        )
    }
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
mod tests {
    use super::*;

    #[test]
    fn the_config_is_read_into_branches_and_fork_remotes() {
        let out = "branch.feat/x.remote\nann\0branch.feat/x.merge\nrefs/heads/fix\0\
                   branch.feat/x.pushremote\nann\0branch.main.remote\norigin\0\
                   remote.ann.gitcraftfork\ntrue\0remote.bob.gitcraftfork\ntrue\0";
        let setup = parse_setup(out);
        assert_eq!(setup.branches.len(), 2);
        assert_eq!(setup.branches[0].name, "feat/x");
        assert_eq!(setup.branches[0].push_remote, "ann");
        assert_eq!(setup.fork_remotes, ["ann", "bob"]);
        assert_eq!(setup.unused_forks(), ["bob"]);
        assert_eq!(
            setup.pushing_to("ann", "fix").map(|b| b.name.as_str()),
            Some("feat/x")
        );
    }

    #[test]
    fn remote_names_are_made_of_the_owner() {
        assert_eq!(remote_name("ann"), "ann");
        assert_eq!(remote_name("Zoë Ångström"), "Zo---ngstr-m");
        assert_eq!(remote_name("--"), "fork");
        assert_eq!(remote_name("group/sub"), "group-sub");
    }

    #[test]
    fn urls_of_the_same_repository_are_the_same() {
        assert!(same_repository(
            "git@github.com:Ann/App.git",
            "https://github.com/ann/app"
        ));
        assert!(!same_repository(
            "git@github.com:ann/app.git",
            "https://github.com/ann/other"
        ));
    }

    fn info() -> ForkInfo {
        ForkInfo {
            number: 7,
            cross: true,
            owner: "ann".into(),
            clone_https: "https://example.org/ann/app.git".into(),
            clone_ssh: "git@example.org:ann/app.git".into(),
            branch: "fix".into(),
            base: "main".into(),
            maintainers_can_edit: true,
            ..ForkInfo::default()
        }
    }

    fn remote(name: &str, url: &str) -> Remote {
        Remote {
            name: name.into(),
            url: url.into(),
        }
    }

    #[test]
    fn a_checkout_is_refused_where_the_author_doesnt_allow_it() {
        let mut no = info();
        no.maintainers_can_edit = false;
        assert!(checkout(&no, &[], &Setup::default(), &[], "main").is_err());
        let mut own = info();
        own.cross = false;
        assert!(checkout(&own, &[], &Setup::default(), &[], "main").is_err());
        let mut odd = info();
        odd.branch = "-x".into();
        assert!(checkout(&odd, &[], &Setup::default(), &[], "main").is_err());
    }

    #[test]
    fn the_fork_is_reached_like_origin_and_names_taken_are_avoided() {
        let remotes = [
            remote("origin", "git@example.org:acme/app.git"),
            remote("ann", "https://example.org/someone-else/app.git"),
        ];
        let plan = checkout(
            &info(),
            &remotes,
            &Setup::default(),
            &["fix".into()],
            "main",
        );
        assert!(plan.is_ok(), "{plan:?}");
        let Ok(plan) = plan else { return };
        assert_eq!(plan.url, "git@example.org:ann/app.git");
        assert_eq!(plan.remote, "ann-fork");
        assert_eq!(plan.local, "ann/fix");
        assert!(
            !plan
                .steps
                .iter()
                .any(|s| s.iter().any(|a| a.ends_with(".push")))
        );
        // The fork already a remote: used as it is.
        let remotes = [
            remote("origin", "https://example.org/acme/app.git"),
            remote("annie", "https://example.org/ann/app"),
        ];
        let plan = checkout(&info(), &remotes, &Setup::default(), &[], "main");
        assert!(plan.is_ok_and(|p| p.remote == "annie" && !p.new_remote && p.local == "fix"));
    }
}

#[cfg(test)]
mod fluent_r1_identifier_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_collision_names_preserve_decimal_pr_identity_and_planned_argv() {
        let info = ForkInfo {
            number: 1234,
            cross: true,
            maintainers_can_edit: true,
            owner: "ann".into(),
            branch: "fix".into(),
            clone_https: "https://example.invalid/ann/repo.git".into(),
            ..Default::default()
        };
        let setup = Setup::default();
        let locals = vec!["fix".into(), "ann/fix".into()];
        let expected = checkout(&info, &[], &setup, &locals, "previous").unwrap();
        assert_eq!(expected.local, "pr-1234");
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            let planned = checkout(&info, &[], &setup, &locals, "previous").unwrap();
            assert_eq!(planned.local, expected.local);
            assert_eq!(planned.steps, expected.steps);
            assert_eq!(planned.undo, expected.undo);
            for collision in ["pr-1234", "pr-1234/child"] {
                let mut occupied = locals.clone();
                occupied.push(collision.into());
                let error = checkout(&info, &[], &setup, &occupied, "previous")
                    .err()
                    .unwrap();
                assert!(error.contains("pr-1234"), "{error}");
                assert!(!error.contains("1.234") && !error.contains("1,234"));
            }
        }
        crate::i18n::set_language("en-US");
    }
}
