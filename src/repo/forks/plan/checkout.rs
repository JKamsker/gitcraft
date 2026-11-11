//! Fork remote selection and guarded checkout assembly.

use super::*;

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

/// Whether a remote URL is reached over SSH: `ssh://…`, or `user@host:path`.
pub(super) fn is_ssh(url: &str) -> bool {
    url.starts_with("ssh://")
        || (!url.contains("://")
            && url
                .split_once(':')
                .is_some_and(|(left, _)| left.contains('@') && !left.contains('/')))
}

/// The remote that is `info`'s fork (by its URL, however written), if there is one.
pub(crate) fn fork_remote(
    info: &ForkInfo,
    remotes: &[Remote],
    hosting: Option<&Hosting>,
) -> Option<String> {
    let fork = hosting
        .filter(|_| !info.repo_path.is_empty())
        .map(|h| Hosting {
            path: info.repo_path.clone(),
            ..h.clone()
        });
    remotes
        .iter()
        .find(|r| {
            // Exact advertised URLs also support owned local fixtures. Otherwise use the
            // selected forge's identity; an HTTP remote cannot bypass its port via clone_ssh.
            [&info.clone_https, &info.clone_ssh].iter().any(|url| {
                !url.is_empty() && r.url.trim_end_matches('/') == url.trim_end_matches('/')
            }) || fork.as_ref().is_some_and(|h| h.matches_git_remote(&r.url))
        })
        .map(|r| r.name.clone())
}

/// A remote name made of `owner`: what git takes in a name, nothing else.
pub(super) fn remote_name(owner: &str) -> String {
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
    hosting: Option<&Hosting>,
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
        .find(|r| hosting.is_some_and(|h| h.matches_git_remote(&r.url)))
        .or_else(|| remotes.iter().find(|r| r.name == "origin"))
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
    if let Some(hosting) = hosting {
        let fork = Hosting {
            path: info.repo_path.clone(),
            ..hosting.clone()
        };
        if !fork.matches_git_remote(&url) {
            return Err("The advertised fork clone URL does not match this primary forge".into());
        }
    }
    let (remote, new_remote) = match fork_remote(info, remotes, hosting) {
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
    if let Some(hosting) = hosting {
        let fork = Hosting {
            path: info.repo_path.clone(),
            ..hosting.clone()
        };
        let guard = CheckoutDestination::new(&remote, &url, new_remote, &fork).step();
        steps.push(guard.clone());
        undo.push(guard);
    }
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
pub(super) fn plain_branch(name: &str) -> bool {
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
