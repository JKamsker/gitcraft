//! Validate PR commit targets and assemble forge-specific fetches.
use super::*;

/// The changes a pull request proposes, as its web page shows them: from the merge base.
#[derive(Clone, Debug)]
pub struct PrFiles {
    pub from: String,
    pub to: String,
    pub files: Vec<ChangedFile>,
}

/// Finds the changes of `details`, fetching its commits first if they aren't here yet.
pub fn files(
    repo: &Path,
    hosting: &forge::Hosting,
    details: &PrDetails,
    remotes: &[Remote],
    credentials: Option<&Credentials>,
) -> Result<PrFiles, String> {
    // What the forge says goes to git as refspecs: anything but a commit id or a branch name could
    // make the fetch write a ref (`+x:refs/heads/main`) or read as an option.
    if !is_oid(&details.head_oid) || !is_oid(&details.base_oid) {
        return Err("the forge named commits that aren't commit ids".to_owned());
    }
    if !is_branch_name(&details.base_branch) {
        return Err(format!(
            "the forge named a base branch git can't take: {:?}",
            details.base_branch
        ));
    }
    // The native rev-parse verifier checks commit presence without starting Git.
    let has = |oid: &str| git::resolve(repo, oid).is_ok();
    if !has(&details.head_oid) || !has(&details.base_oid) {
        let remote = remote_for(remotes, &details.url, hosting)
            .ok_or("no remote of this repository is the one the pull request belongs to")?;
        if details.forge == Some(Forge::Bitbucket) {
            for (args, use_destination_credentials) in bitbucket_fetches(details, remote)? {
                let args = args.iter().map(String::as_str).collect::<Vec<_>>();
                git::run_with(
                    Some(repo),
                    &args,
                    if use_destination_credentials {
                        credentials
                    } else {
                        None
                    },
                )?;
            }
        } else {
            // No refs and no FETCH_HEAD: nothing here changes but the objects being available.
            let base = format!("refs/heads/{}", details.base_branch);
            let mut args = vec!["fetch", "--no-tags", "--no-write-fetch-head", remote];
            if is_branch_ref(&details.head_ref) {
                args.push(&details.head_ref);
            }
            args.push(&base);
            // The branches may have moved on since the details were read (a force push even leaves
            // the old commits out), so the very commits are asked for too.
            args.extend([details.head_oid.as_str(), details.base_oid.as_str()]);
            git::run_with(Some(repo), &args, credentials)?;
        }
    }
    let from = git::merge_base(repo, &details.base_oid, &details.head_oid)?;
    let files = git::diff_files(repo, &from, Some(&details.head_oid))?;
    Ok(PrFiles {
        from,
        to: details.head_oid.clone(),
        files,
    })
}

/// Cloud PRs have no documented Server-style pull-request ref. Fetch each native
/// repository's branch and displayed commit without creating refs or rebinding credentials.
pub(super) fn bitbucket_fetches(
    d: &PrDetails,
    remote: &str,
) -> Result<Vec<(Vec<String>, bool)>, String> {
    let (host, destination) = repo_of_url(&d.url).ok_or("Invalid Bitbucket PR URL")?;
    if host != "bitbucket.org" || !d.url.starts_with("https://bitbucket.org/") {
        return Err("Bitbucket source belongs to a different service".into());
    }
    let source = d
        .extras
        .head_repo
        .as_deref()
        .ok_or("Bitbucket PR has no source repository")?;
    let parts = source.split('/').collect::<Vec<_>>();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || p.starts_with(['-', '.'])
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
        || !is_branch_name(&d.head_branch)
        || !is_branch_name(&d.base_branch)
        || !is_oid(&d.head_oid)
        || !is_oid(&d.base_oid)
    {
        return Err("Invalid Bitbucket source repository or revision".into());
    }
    let same = source.eq_ignore_ascii_case(&destination);
    let source_url = if same {
        remote.into()
    } else {
        format!("https://bitbucket.org/{source}.git")
    };
    let fetch = |url: &str, branch: &str, oid: &str| {
        vec![
            "fetch".into(),
            "--no-tags".into(),
            "--no-write-fetch-head".into(),
            url.into(),
            format!("refs/heads/{branch}"),
            oid.into(),
        ]
    };
    Ok(vec![
        (fetch(remote, &d.base_branch, &d.base_oid), true),
        (fetch(&source_url, &d.head_branch, &d.head_oid), same),
    ])
}

/// Whether `oid` is a whole commit id (SHA-1 or SHA-256), nothing else.
pub(crate) fn is_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `name` is a branch name git takes, and nothing a refspec or an option could be made of:
/// `git check-ref-format`'s rules.
pub(crate) fn is_branch_name(name: &str) -> bool {
    let forbidden = |c: char| c.is_ascii_control() || " ~^:?*[\\".contains(c);
    !name.is_empty()
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.split('/').any(|part| part.ends_with(".lock"))
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.contains("/.")
        && !name.chars().any(forbidden)
}

/// Whether `r` is a ref the fetch may name, like the `refs/pull/12/head` the forges' modules set.
pub(super) fn is_branch_ref(r: &str) -> bool {
    r.strip_prefix("refs/").is_some_and(is_branch_name)
}
