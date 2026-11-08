//! A link made a permalink: the revision it's at turned into its commit's hash, the forge of the
//! remote found, and a warning when what it points at isn't on that remote yet.

use std::path::Path;

use super::{Place, Target, upstream_remote, url};
use crate::forge::{self, gitea};
use crate::git::{self, Remote};

/// A link to copy or open, and why it might not work yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    /// What isn't on the remote (as far as its last fetch tells): the page won't be found
    /// until it's pushed.
    pub warning: Option<String>,
}

/// The link to `target` on the forge `remote` (a name among `remotes`) is on. Runs git (and reads
/// this machine's `tea` logins): off the window's thread.
pub fn resolve(
    repo: &Path,
    remotes: &[Remote],
    remote: &str,
    target: &Target,
) -> Result<Link, String> {
    let chosen = remotes.iter().find(|r| r.name == remote).ok_or_else(|| {
        crate::i18n::message(
            "backend-the-remote-remote-is-gone-275dba",
            &[("remote", (remote).to_string().into())],
        )
    })?;
    let mut config = forge::gitcraft_config(repo);
    // `gitcraft.forge` says what `origin` (the remote detection reads) is on, not every remote:
    // another remote goes by `gitcraft.<host>.forge` and its host's name.
    let primary = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or(remotes.first());
    if primary.is_some_and(|r| r.name != remote) {
        config.remove("gitcraft.forge");
    }
    let hosting = forge::detect_with(std::slice::from_ref(chosen), &config, &gitea::tea_servers())
        .ok_or_else(|| unknown_forge(chosen))?;
    let (place, warning) = match target {
        Target::Commit { rev } => {
            let sha = commit_sha(repo, rev)?;
            let warning = unpushed(repo, remotes, remote, &sha)?;
            return Ok(Link {
                url: url(&hosting, &Place::Commit(&sha)),
                warning,
            });
        }
        Target::Branch { name } => (Place::Branch(name), branch_missing(repo, remote, name)?),
        Target::File { rev, path, lines } => {
            let sha = commit_sha(repo, rev)?;
            if git::run(Some(repo), &["cat-file", "-e", &format!("{sha}:{path}")]).is_err() {
                return Err(crate::i18n::message(
                    "backend-path-isn-t-in-commit-920013",
                    &[
                        ("path", (path).to_string().into()),
                        ("sha", (short(&sha)).to_string().into()),
                    ],
                ));
            }
            let warning = unpushed(repo, remotes, remote, &sha)?;
            let link = url(
                &hosting,
                &Place::File {
                    sha: &sha,
                    path,
                    lines: *lines,
                },
            );
            return Ok(Link { url: link, warning });
        }
    };
    Ok(Link {
        url: url(&hosting, &place),
        warning,
    })
}

/// Why `remote` has no link: its host isn't one gitcraft knows a forge by.
fn unknown_forge(remote: &Remote) -> String {
    let host = forge::location(&remote.url).map_or_else(|| remote.url.clone(), |(host, _)| host);
    crate::i18n::message(
        "backend-gitcraft-doesn-t-know-which-forge-host-remote-is-if-it-s-github-g-1df5a2",
        &[
            ("host", (host).to_string().into()),
            ("name", (remote.name).to_string().into()),
        ],
    )
}

/// The full hash of the commit `rev` names: a link to it keeps showing the same thing when its
/// branch moves on.
fn commit_sha(repo: &Path, rev: &str) -> Result<String, String> {
    let out = git::run_ok_codes(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
        &[0, 1],
    )?;
    let sha = out.trim();
    if sha.is_empty() {
        return Err(match rev.strip_suffix('^') {
            Some(child) => crate::i18n::message(
                "backend-has-no-parent-commit-99ef4b",
                &[("child", (short(child)).to_string().into())],
            ),
            None => crate::i18n::message(
                "backend-isn-t-a-commit-in-this-repository-733379",
                &[("rev", (short(rev)).to_string().into())],
            ),
        });
    }
    Ok(sha.to_owned())
}

/// What to warn of when `sha` isn't on `remote`: by its remote-tracking branches, as of the last
/// fetch.
fn unpushed(
    repo: &Path,
    remotes: &[Remote],
    remote: &str,
    sha: &str,
) -> Result<Option<String>, String> {
    let out = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("--contains={sha}"),
            "refs/remotes/",
        ],
    )?;
    let mut holders: Vec<String> = out
        .lines()
        .filter_map(|r| r.strip_prefix("refs/remotes/"))
        .filter_map(|r| upstream_remote(r, remotes).map(|(name, _)| name))
        .collect();
    holders.sort();
    holders.dedup();
    if holders.iter().any(|r| r == remote) {
        return Ok(None);
    }
    let commit = short(sha);
    Ok(Some(if holders.is_empty() {
        crate::i18n::message(
            "backend-commit-isn-t-pushed-to-any-remote-yet-so-the-link-won-t-work-unti-d88428",
            &[("commit", (commit).to_string().into())],
        )
    } else {
        crate::i18n::message(
            "backend-commit-isn-t-on-remote-yet-only-on-so-the-link-won-t-work-until-i-0be5b9",
            &[
                ("commit", (commit).to_string().into()),
                ("remote", (remote).to_string().into()),
                ("holders", (holders.join(", ")).to_string().into()),
            ],
        )
    }))
}

/// What to warn of when `remote` has no branch `name`, as of the last fetch.
fn branch_missing(repo: &Path, remote: &str, name: &str) -> Result<Option<String>, String> {
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/remotes/{remote}/{name}"),
        ],
        &[0, 1],
    )?;
    Ok(out.trim().is_empty().then(|| {
        crate::i18n::message(
            "backend-name-isn-t-on-remote-yet-so-the-link-won-t-work-until-it-s-pushed-8a4f97",
            &[
                ("name", (name).to_string().into()),
                ("remote", (remote).to_string().into()),
            ],
        )
    }))
}

fn short(rev: &str) -> &str {
    rev.get(..8).unwrap_or(rev)
}
