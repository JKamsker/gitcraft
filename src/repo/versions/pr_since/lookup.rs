//! Pull-request version lookup, guarded fetching, and reviewed-ref persistence.

use super::*;

pub(super) fn pull_ref(forge: Option<Forge>, number: u64) -> String {
    match forge {
        Some(Forge::GitLab) => format!("refs/merge-requests/{number}/head"),
        _ => format!("refs/pull/{number}/head"),
    }
}

/// Whether `oid` is a whole commit id, as the forges give them.
pub(super) fn is_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `r` is a ref a fetch may be given as it is: never an option or a `src:dst` refspec.
pub(super) fn is_plain_ref(r: &str) -> bool {
    r.starts_with("refs/")
        && !r.contains("..")
        && !r
            .chars()
            .any(|c| c.is_ascii_control() || " ~^:?*[\\".contains(c))
}

/// Makes sure the commits of `pr` are here, fetching them from the forge if not.
pub(super) fn have_head(
    repo: &Path,
    pr: &PrHead,
    creds: Option<&Credentials>,
) -> Result<(), String> {
    if !is_oid(&pr.head) {
        return Err(crate::i18n::text("repo-workflows-versions-have-head-54ea0ca1").to_owned());
    }
    let has = |oid: &str| {
        git::run(
            Some(repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    };
    if has(&pr.head) {
        return Ok(());
    }
    let remote = pr.remote.as_deref().ok_or(crate::i18n::text(
        "repo-workflows-versions-have-head-c36591b2",
    ))?;
    // Nobody waits on this: it asks nobody to sign in.
    let fetch = |what: &str| {
        let args = ["fetch", "--no-tags", "--no-write-fetch-head", remote, what];
        git::run_unattended(Some(repo), &args, creds)
    };
    // The forge's ref first (every server gives it); the commit itself only when the ref has
    // moved on since it was read, as not every server hands out a commit asked by its id.
    let by_ref = if is_plain_ref(&pr.fetch_ref) {
        fetch(&pr.fetch_ref)
    } else {
        Err(crate::i18n::text("repo-workflows-versions-have-head-5c35312b").to_owned())
    };
    if !has(&pr.head) {
        fetch(&pr.head).map_err(|e| by_ref.err().unwrap_or(e))?;
    }
    if has(&pr.head) {
        Ok(())
    } else {
        Err(crate::i18n::message(
            "repo-workflows-versions-have-head-36c411a3",
            &[("short-pr-head-1", (short(&pr.head)).into())],
        ))
    }
}

/// Where the pull request's commits start: where its head forks off its base branch (as the
/// forge names it, else the remote's default branch), else where the version last looked at
/// started or ended. None found is an error rather than an empty version, which would make
/// every later comparison all "added".
pub(super) fn base_of(repo: &Path, pr: &PrHead, seen: Option<&Series>) -> Result<String, String> {
    let default = pr.remote.as_ref().map(|r| format!("refs/remotes/{r}/HEAD"));
    let candidates = pr
        .base
        .iter()
        .filter(|b| is_oid(b))
        .cloned()
        .chain(default)
        .chain(seen.filter(|s| s.base != s.tip).map(|s| s.base.clone()))
        .chain(seen.map(|s| s.tip.clone()));
    for candidate in candidates {
        if let Ok(base) = git::merge_base(repo, &candidate, &pr.head) {
            return Ok(base);
        }
    }
    Err(crate::i18n::message(
        "repo-workflows-versions-base-of-e9c2679e",
        &[("pr-number-1", (pr.number).to_string().into())],
    ))
}

/// Looks at `pr`: how it changed since the version last looked at, remembering this one as
/// seen when none was.
pub(super) fn look(
    repo: &Path,
    repo_key: &str,
    pr: &PrHead,
    creds: Option<&Credentials>,
) -> Result<Found, String> {
    let seen = git::reviewed(repo, repo_key, pr.number)?;
    if seen.as_ref().is_some_and(|s| s.tip == pr.head) {
        return Ok(Found::Same);
    }
    have_head(repo, pr, creds)?;
    let new = Series::new(&base_of(repo, pr, seen.as_ref())?, &pr.head);
    let Some(old) = seen else {
        git::mark_reviewed(repo, repo_key, pr.number, &new)?;
        return Ok(Found::First);
    };
    let counts = git::range_diff(repo, &old, &new)?.counts();
    let rewritten = !git::is_ancestor(repo, &old.tip, &new.tip);
    Ok(Found::Moved(Since {
        number: pr.number,
        old,
        new,
        counts,
        rewritten,
    }))
}

/// Remembers the version of `pr` now as the one seen; gives it.
pub(super) fn mark_seen(
    repo: &Path,
    repo_key: &str,
    pr: &PrHead,
    creds: Option<&Credentials>,
) -> Result<Series, String> {
    have_head(repo, pr, creds)?;
    let seen = git::reviewed(repo, repo_key, pr.number)?;
    let new = Series::new(&base_of(repo, pr, seen.as_ref())?, &pr.head);
    git::mark_reviewed(repo, repo_key, pr.number, &new)?;
    Ok(new)
}
