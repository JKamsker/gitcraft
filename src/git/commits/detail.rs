//! Detail.

use super::*;

#[derive(Clone, Debug)]
pub struct CommitDetail {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub author_date: String,
    pub committer: String,
    pub commit_date: String,
    pub message: String,
    pub files: Vec<ChangedFile>,
    /// Whether the checked-out branch (HEAD) contains it, so its message can be edited.
    pub in_head: bool,
    /// Whether HEAD has merges after it, which rewording it by a rebase would flatten.
    pub merges_after: bool,
}

/// What a read on a thread of its own gave, or that it stopped (it panicked).
pub(super) fn joined<T>(
    handle: std::thread::ScopedJoinHandle<'_, Result<T, String>>,
) -> Result<T, String> {
    handle
        .join()
        .unwrap_or_else(|_| Err(crate::i18n::text("interaction-teleport-error-stopped").into()))
}

pub fn commit_detail(repo: &Path, hash: &str) -> Result<CommitDetail, String> {
    super::super::backend::dispatch(
        repo,
        super::super::backend::Read::CommitDetail,
        |repo| super::super::backend::commit_detail(repo, hash),
        || commit_detail_by_git(repo, hash),
    )
}

/// [`commit_detail`] as git reads it.
pub(in crate::git) fn commit_detail_by_git(
    repo: &Path,
    hash: &str,
) -> Result<CommitDetail, String> {
    // NULs between the fields: no commit has one, unlike the unit separator.
    let format = "--format=%H%x00%P%x00%an%x00%ae%x00%ad%x00%cn%x00%cd%x00%B";
    let header_args = [
        "show",
        NO_SIGNATURE,
        UTF8,
        "-s",
        "--date=format-local:%Y-%m-%d %H:%M:%S",
        format,
        END_OPTS,
        hash,
    ];
    let range = format!("{hash}..HEAD");
    // The reads don't depend on each other: all at once, each of them a process start (the
    // larger part of the wait on Windows) rather than five one after the other.
    let (out, files, in_head, merges) = std::thread::scope(|s| {
        let files = {
            let inherited = crate::tasks::Inherited::capture();
            s.spawn(move || inherited.run(|| commit_files(repo, hash)))
        };
        let in_head = {
            let inherited = crate::tasks::Inherited::capture();
            s.spawn(move || {
                inherited.run(|| run(Some(repo), &["merge-base", "--is-ancestor", hash, "HEAD"]))
            })
        };
        let merges = {
            let inherited = crate::tasks::Inherited::capture();
            let range = &range;
            s.spawn(move || {
                inherited.run(|| run(Some(repo), &["rev-list", "--merges", "--count", range]))
            })
        };
        let out = run(Some(repo), &header_args);
        (out, joined(files), joined(in_head), joined(merges))
    });
    let out = out?;
    let f: Vec<&str> = out.splitn(8, '\0').collect();
    let [h, parents, author, email, adate, committer, cdate, message] = f[..] else {
        return Err(crate::i18n::text("backend-unexpected-output-from-git-show-c43bf4").to_owned());
    };
    let files = files?;
    let in_head = in_head.is_ok();
    let merges_after = in_head && merges.is_ok_and(|n| n.trim() != "0");
    Ok(CommitDetail {
        hash: h.to_owned(),
        parents: parents.split_whitespace().map(str::to_owned).collect(),
        author: author.to_owned(),
        email: email.to_owned(),
        author_date: adate.to_owned(),
        committer: committer.to_owned(),
        commit_date: cdate.to_owned(),
        message: message.trim_end().to_owned(),
        files,
        in_head,
        merges_after,
    })
}

/// Maximum object and decoded message size retained for a graph tooltip.
pub const MAX_TOOLTIP_MESSAGE_BYTES: usize = 256 * 1024;

/// Read one original message without expanding every graph row's compact preview.
/// Check object size before reading it, and cap the decoded text retained by the UI.
pub fn commit_message(repo: &Path, hash: &str) -> Result<String, String> {
    if !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(
            crate::i18n::text("backend-expected-a-full-commit-object-id-edce46").to_owned(),
        );
    }
    let commit = format!("{hash}^{{commit}}");
    let size = run(Some(repo), &["cat-file", "-s", &commit])?
        .trim()
        .parse::<usize>()
        .map_err(|_| crate::i18n::text("backend-invalid-commit-object-size-0ac3de").to_owned())?;
    let too_large = || {
        crate::i18n::text(
            "backend-message-exceeds-the-256-kib-tooltip-limit-select-the-commit-to-re-ebcc66",
        )
        .to_owned()
    };
    if size > MAX_TOOLTIP_MESSAGE_BYTES {
        return Err(too_large());
    }
    let message = run(
        Some(repo),
        &[
            "show",
            NO_SIGNATURE,
            UTF8,
            "-s",
            "--format=%B",
            END_OPTS,
            &commit,
        ],
    )?;
    if message.len() > MAX_TOOLTIP_MESSAGE_BYTES {
        return Err(too_large());
    }
    Ok(message.trim_end().to_owned())
}

/// The full message of HEAD (to amend it).
pub fn head_message(repo: &Path) -> Result<String, String> {
    run(
        Some(repo),
        &["log", NO_SIGNATURE, UTF8, "-1", "--format=%B"],
    )
    .map(|s| s.trim_end().to_owned())
}
