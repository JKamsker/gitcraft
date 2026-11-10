//! Descriptor and metadata ownership proofs for deferred replay dispatch.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DispatchBinding {
    pub(super) metadata: PathBuf,
    pub(super) metadata_identity: String,
    pub(super) common: PathBuf,
    pub(super) common_identity: String,
    pub(super) link_identity: String,
    pub(super) link_text: String,
}

pub(super) struct Ownership {
    pub(super) binding: DispatchBinding,
    pub(super) root: native_files::Directory,
    pub(super) metadata: native_files::Directory,
    pub(super) common: native_files::Directory,
    pub(super) link: native_files::Snapshot,
}

impl Ownership {
    pub(super) fn unchanged(&self) -> bool {
        self.root.unchanged()
            && self.metadata.unchanged()
            && self.common.unchanged()
            && self.link.same_at(&self.root, ".git")
    }
}

#[cfg(test)]
pub(super) fn owned_command(
    s: &Step,
    binding: &DispatchBinding,
    observed: &DispatchBinding,
    args: &[&str],
) -> Result<Vec<String>, String> {
    owned_command_at(
        &s.worktree,
        s.root_identity.as_deref(),
        binding,
        observed,
        args,
    )
}

pub(super) fn owned_command_at(
    worktree: &Path,
    root_identity: Option<&str>,
    binding: &DispatchBinding,
    observed: &DispatchBinding,
    args: &[&str],
) -> Result<Vec<String>, String> {
    if !cfg!(unix) {
        return Err(crate::i18n::text(
            "backend-deferred-replay-requires-native-unix-descriptor-ownership-no-repl-4a8f0e",
        )
        .into());
    }
    if binding != observed {
        return Err(crate::i18n::text(
            "backend-deferred-metadata-link-identity-changed-no-replay-was-dispatched-65b3b5",
        )
        .into());
    }
    let pin = root_identity.ok_or(crate::i18n::text(
        "backend-no-owned-directory-identity-e958ff",
    ))?;
    Ok([
        "-c",
        OWNED_ALIAS,
        "gitcraft-owned-replay",
        pin,
        worktree.to_str().ok_or(crate::i18n::text(
            "backend-worktree-path-must-be-utf-8-20c4a0",
        ))?,
        binding.metadata.to_str().ok_or(crate::i18n::text(
            "backend-metadata-path-must-be-utf-8-95f687",
        ))?,
        &binding.metadata_identity,
        binding.common.to_str().ok_or(crate::i18n::text(
            "backend-common-path-must-be-utf-8-169a0d",
        ))?,
        &binding.common_identity,
        &binding.link_identity,
        &binding.link_text,
    ]
    .into_iter()
    .chain(args.iter().copied())
    .map(str::to_owned)
    .collect())
}

pub(super) fn owned_run(
    r: &Runner<'_>,
    common: &Path,
    s: &Step,
    args: &[&str],
) -> Result<String, String> {
    registered_run(
        r,
        common,
        &s.worktree,
        s.root_identity.as_deref().ok_or(crate::i18n::text(
            "backend-no-owned-directory-identity-e958ff",
        ))?,
        args,
    )
}

/// Shared only with the subtree candidate consumer. The original retained-directory,
/// Git-link and registration checks remain in one implementation.
pub(in crate::git) fn registered_run(
    r: &Runner<'_>,
    common: &Path,
    worktree: &Path,
    identity: &str,
    args: &[&str],
) -> Result<String, String> {
    let ownership = owned_at(r.repo, common, worktree, Some(identity))?;
    let observed = filesystem_owned_at(common, worktree, Some(identity))?;
    let words = owned_command_at(
        worktree,
        Some(identity),
        &ownership.binding,
        &observed.binding,
        args,
    )?;
    if !ownership.unchanged() {
        return Err(crate::i18n::text(
            "backend-deferred-metadata-link-changed-before-dispatch-no-replay-was-disp-3d62f9",
        )
        .into());
    }
    let result = run(
        r,
        r.repo,
        &words.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    if !ownership.unchanged() {
        return Err(crate::i18n::text("backend-deferred-ownership-changed").into());
    }
    result
}

pub(super) fn run(r: &Runner<'_>, path: &Path, args: &[&str]) -> Result<String, String> {
    let mut words = vec![
        "--no-lazy-fetch",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "rebase.updateRefs=false",
        "-c",
        "rebase.autoStash=false",
    ];
    words.extend(args);
    r.in_repo(path, &words)
}

// Check filesystem provenance before asking Git to enter a retained candidate. A path
// and a registration alone do not prove that its .git still belongs to this repository.
#[cfg(any(unix, test))]
pub(super) fn ownership_paths(
    common: &Path,
    worktree: &Path,
    gitfile: &[u8],
    backlink: &[u8],
) -> Result<PathBuf, String> {
    let text = |b: &[u8]| {
        std::str::from_utf8(b)
            .ok()
            .map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
    };
    let gitfile =
        text(gitfile).ok_or(crate::i18n::text("backend-invalid-owned-git-link-80fea5"))?;
    let gitdir = PathBuf::from(gitfile.strip_prefix("gitdir: ").ok_or(crate::i18n::text(
        "backend-candidate-is-not-a-linked-worktree-41ee36",
    ))?);
    let relative = gitdir
        .strip_prefix(common.join("worktrees"))
        .map_err(|_| crate::i18n::text("backend-candidate-belongs-to-another-repository-621077"))?;
    if relative.components().count() != 1
        || !matches!(
            relative.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err(crate::i18n::text(
            "backend-candidate-metadata-is-outside-its-owned-registration-ee927f",
        )
        .into());
    }
    let backlink = text(backlink).ok_or(crate::i18n::text(
        "backend-invalid-owned-git-backlink-2a6784",
    ))?;
    if Path::new(&backlink) != worktree.join(".git") {
        return Err(
            crate::i18n::text("backend-candidate-registration-backlink-changed-73c0cc").into(),
        );
    }
    Ok(gitdir)
}

pub(super) fn root_identity(path: &Path) -> Result<String, String> {
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, open};
        std::fs::File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?,
        )
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .map_err(|e| e.to_string())?
    };
    #[cfg(any(unix, windows))]
    return crate::git::recovery::native_file_identity(&file).map(|v| v.0);
    #[cfg(not(any(unix, windows)))]
    Err(crate::i18n::text(
        "backend-stable-worktree-directory-identities-are-unavailable-on-this-plat-d2adb7",
    )
    .into())
}

#[cfg(unix)]
pub(super) fn filesystem_owned_at(
    common: &Path,
    worktree: &Path,
    identity: Option<&str>,
) -> Result<Ownership, String> {
    use native_files::{Directory, Snapshot};
    if std::fs::canonicalize(worktree).map_err(|e| e.to_string())? != worktree {
        return Err(
            crate::i18n::text("backend-deferred-worktree-physical-path-changed-898b40").into(),
        );
    }
    let root = Directory::open(worktree).map_err(|e| e.to_string())?;
    if identity
        != Some(
            crate::git::recovery::native_file_identity(root.file())?
                .0
                .as_str(),
        )
    {
        return Err(crate::i18n::text("backend-deferred-worktree-identity-changed-c453de").into());
    }
    let link = Snapshot::read(&root, ".git", 8192)?.ok_or(crate::i18n::text(
        "backend-candidate-git-link-missing-504a57",
    ))?;
    let text = std::str::from_utf8(&link.bytes)
        .map_err(|_| crate::i18n::text("backend-invalid-owned-git-link-80fea5"))?;
    if text.contains('\0') {
        return Err(crate::i18n::text("backend-invalid-owned-git-link-80fea5").into());
    }
    let path = PathBuf::from(
        text.trim_end_matches(['\r', '\n'])
            .strip_prefix("gitdir: ")
            .ok_or(crate::i18n::text(
                "backend-candidate-is-not-a-linked-worktree-41ee36",
            ))?,
    );
    // Validate its single registration component before opening its files.
    ownership_paths(
        common,
        worktree,
        &link.bytes,
        worktree
            .join(".git")
            .to_str()
            .ok_or(crate::i18n::text("backend-non-utf-8-worktree-c28e6c"))?
            .as_bytes(),
    )?;
    if std::fs::canonicalize(&path).map_err(|e| e.to_string())? != path
        || std::fs::canonicalize(common).map_err(|e| e.to_string())? != common
    {
        return Err(crate::i18n::text(
            "backend-deferred-metadata-common-directory-is-linked-8d7429",
        )
        .into());
    }
    let metadata = Directory::open(&path).map_err(|e| e.to_string())?;
    let common_dir = Directory::open(common).map_err(|e| e.to_string())?;
    let backlink = Snapshot::read(&metadata, "gitdir", 8192)?.ok_or(crate::i18n::text(
        "backend-candidate-backlink-missing-25d58b",
    ))?;
    ownership_paths(common, worktree, &link.bytes, &backlink.bytes)?;
    let common_link = Snapshot::read(&metadata, "commondir", 8192)?.ok_or(crate::i18n::text(
        "backend-candidate-common-directory-link-missing-0140f5",
    ))?;
    let common_text = std::str::from_utf8(&common_link.bytes)
        .map_err(|_| crate::i18n::text("backend-invalid-common-link-720f23"))?
        .trim_end_matches(['\r', '\n']);
    if std::fs::canonicalize(path.join(common_text)).map_err(|e| e.to_string())? != common {
        return Err(crate::i18n::text("backend-candidate-common-directory-changed-2dc85f").into());
    }
    let binding = DispatchBinding {
        metadata: path,
        metadata_identity: crate::git::recovery::native_file_identity(metadata.file())?.0,
        common: common.to_owned(),
        common_identity: crate::git::recovery::native_file_identity(common_dir.file())?.0,
        link_identity: link.identity_token()?,
        link_text: text.trim_end_matches('\n').into(),
    };
    let owned = Ownership {
        binding,
        root,
        metadata,
        common: common_dir,
        link,
    };
    if !owned.unchanged()
        || !backlink.same_at(&owned.metadata, "gitdir")
        || !common_link.same_at(&owned.metadata, "commondir")
    {
        return Err(crate::i18n::text(
            "backend-deferred-ownership-moved-while-observing-it-f456a7",
        )
        .into());
    }
    Ok(owned)
}

#[cfg(not(unix))]
pub(super) fn filesystem_owned_at(
    _: &Path,
    _: &Path,
    _: Option<&str>,
) -> Result<Ownership, String> {
    Err(crate::i18n::text(
        "backend-deferred-native-descriptor-ownership-unavailable-on-this-platform-6866ca",
    )
    .into())
}

pub(super) fn owned(repo: &Path, common: &Path, s: &Step) -> Result<Ownership, String> {
    owned_at(repo, common, &s.worktree, s.root_identity.as_deref())
}

pub(super) fn owned_at(
    repo: &Path,
    common: &Path,
    worktree: &Path,
    identity: Option<&str>,
) -> Result<Ownership, String> {
    let ownership = filesystem_owned_at(common, worktree, identity)?;
    let wt = git::worktree_list(repo, None)?
        .into_iter()
        .find(|w| w.path == worktree)
        .ok_or(crate::i18n::text(
            "backend-deferred-worktree-is-missing-or-no-longer-registered-no-replay-wa-ed9a59",
        ))?;
    if wt.branch.is_some() {
        return Err(crate::i18n::text(
            "backend-deferred-replay-worktree-is-attached-to-a-branch-detach-it-before-bc5b06",
        )
        .into());
    }
    if !ownership.unchanged() {
        return Err(crate::i18n::text(
            "backend-deferred-ownership-changed-while-reading-registrations-no-replay-08ff21",
        )
        .into());
    }
    Ok(ownership)
}

/// Called only after an acknowledged creation at a previously absent candidate path.
pub(in crate::git) fn registered_identity(
    repo: &Path,
    common: &Path,
    worktree: &Path,
) -> Result<String, String> {
    let identity = root_identity(worktree)?;
    owned_at(repo, common, worktree, Some(&identity))?;
    Ok(identity)
}
