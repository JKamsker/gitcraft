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
    let name = name.trim_matches(['-', '.']).to_owned();
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
        return Err(format!(
            "#{}'s branch is in this repository: check it out as any branch",
            info.number
        ));
    }
    if !info.maintainers_can_edit {
        return Err(format!(
            "{} doesn't let maintainers push to #{}'s branch",
            info.owner, info.number
        ));
    }
    if !plain_branch(&info.branch) {
        return Err(format!(
            "#{}'s branch name ({}) can't be used as it is",
            info.number, info.branch
        ));
    }
    let origin = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or_else(|| remotes.first());
    let ssh = origin.is_some_and(|r| r.url.starts_with("git@") || r.url.starts_with("ssh://"));
    let url = if ssh && !info.clone_ssh.is_empty() {
        info.clone_ssh.clone()
    } else {
        info.clone_https.clone()
    };
    if url.is_empty() {
        return Err(format!(
            "The forge didn't say where {}'s fork is",
            info.owner
        ));
    }
    let existing = remotes.iter().find(|r| {
        same_repository(&r.url, &info.clone_https) || same_repository(&r.url, &info.clone_ssh)
    });
    let (remote, new_remote) = match existing {
        Some(r) => (r.name.clone(), false),
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
    let free = |n: &str| !locals.iter().any(|l| l == n);
    let local = [
        info.branch.clone(),
        format!("{}/{}", remote_name(&info.owner), info.branch),
        format!("pr-{}", info.number),
    ]
    .into_iter()
    .find(|n| free(n))
    .ok_or_else(|| {
        format!(
            "{}, {}/{} and pr-{} are all branches here already",
            info.branch, info.owner, info.branch, info.number
        )
    })?;
    steps.push(strings(&["checkout", "-b", &local, "--track", &tracking]));
    steps.push(strings(&[
        "config",
        &format!("branch.{local}.pushRemote"),
        &remote,
    ]));
    let refspec = format!("refs/heads/{local}:refs/heads/{}", info.branch);
    if local != info.branch {
        // `push.default=simple` pushes to a branch of the same name only: this names the one.
        steps.push(strings(&[
            "config",
            "--add",
            &format!("remote.{remote}.push"),
            &refspec,
        ]));
    }
    // Back to what was checked out, the branch deleted only while it has nothing new.
    undo.push(strings(&["checkout", previous, "--"]));
    undo.push(expect_ref(&format!("refs/heads/{local}")));
    undo.push(strings(&["branch", "-D", "--", &local]));
    if new_remote {
        undo.push(strings(&["remote", "remove", "--", &remote]));
    } else if local != info.branch {
        undo.push(strings(&[
            "config",
            "--fixed-value",
            "--unset",
            &format!("remote.{remote}.push"),
            &refspec,
        ]));
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
    let full = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    let local = full
        .trim()
        .strip_prefix("refs/heads/")
        .unwrap_or_default()
        .to_owned();
    let b = setup
        .branches
        .iter()
        .find(|b| b.name == local)
        .filter(|b| b.merge == format!("refs/heads/{}", info.branch))
        .ok_or_else(|| {
            format!(
                "Check out #{}'s branch to push to {} first",
                info.number, info.owner
            )
        })?;
    let fork = if b.push_remote.is_empty() {
        b.remote.clone()
    } else {
        b.push_remote.clone()
    };
    let dirty = git::run(
        Some(repo),
        &["status", "--porcelain", "--untracked-files=no"],
    )?;
    if !dirty.trim().is_empty() {
        return Err("Commit or stash your changes first: this rebases the branch".to_owned());
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
        return Err(format!(
            "{fork}/{} has {missing} commit{} your {local} hasn't: pull them first, or the push \
             would throw them away",
            info.branch,
            if missing == 1 { "" } else { "s" }
        ));
    }
    let behind = count(repo, &format!("{head}..{onto}"))?;
    if behind == 0 {
        return Err(format!(
            "{local} already has every commit of {upstream}/{}",
            info.base
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
        .parse()
        .map_err(|e| format!("unexpected count: {e}"))
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
            plan.steps
                .iter()
                .any(|s| s.contains(&"remote.ann-fork.push".to_owned()))
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
