//! Clone options and cone-mode sparse checkouts. Arguments stay separate on every machine.

use std::collections::BTreeSet;
use std::path::Path;

use super::{Credentials, END_OPTS, clone_from, run, run_watched};

mod patterns;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloneOptions {
    pub depth: Option<u32>,
    pub blobless: bool,
    /// None checks out everything; Some(empty) checks out root files only.
    pub folders: Option<Vec<String>>,
}

impl CloneOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.depth == Some(0) {
            return Err(crate::i18n::text("backend-depth-must-be-at-least-1-77f396").to_owned());
        }
        if let Some(folders) = &self.folders {
            validate_folders(folders)?;
        }
        Ok(())
    }

    fn args(&self, url: &str, dest: &str) -> Result<Vec<String>, String> {
        self.validate()?;
        if url.is_empty() || url.contains(['\0', '\n', '\r']) {
            return Err(crate::i18n::text(
                "backend-enter-a-repository-url-or-path-on-one-line-44a616",
            )
            .to_owned());
        }
        let mut args = vec!["clone".to_owned()];
        if let Some(depth) = self.depth {
            // Local-path clones otherwise silently ignore --depth and --filter.
            args.extend(["--no-local".to_owned(), format!("--depth={depth}")]);
        } else if self.blobless {
            args.push("--no-local".to_owned());
        }
        if self.blobless {
            args.push("--filter=blob:none".to_owned());
        }
        if self.folders.is_some() {
            // Cone mode always includes root files; Git checks out only those initially.
            args.push("--sparse".to_owned());
        }
        args.extend(["--".to_owned(), url.to_owned(), dest.to_owned()]);
        Ok(args)
    }
}

/// The clone button is consent to the initial checkout's downloads, including selected cones.
pub fn clone_with_options(
    url: &str,
    dest: &Path,
    creds: Option<&Credentials>,
    options: &CloneOptions,
    control: &std::sync::Arc<super::OpControl>,
) -> Result<(), String> {
    options.validate()?;
    let (from, there) = match crate::remote::parse(dest) {
        Some(target) => {
            let (from, there) = clone_from(&target.path);
            (Some(crate::remote::encode(&target.host, from)), there)
        }
        None => (None, dest.to_string_lossy().into_owned()),
    };
    let args = options.args(url, &there)?;
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    // Keep existing cancellation/credential/profile handling; no new subprocess route.
    let existed = super::dest_exists(dest);
    let cloned = run_watched(from.as_deref(), &refs, creds, false, control);
    if cloned.is_err() && control.is_cancelled() && existed == Some(false) {
        super::remove_cancelled_clone(dest);
    }
    cloned?;
    if let Some(folders) = &options.folders {
        ensure_sparse_clean(dest)?;
        let args = sparse_args(Some(folders))?;
        let refs: Vec<_> = args.iter().map(String::as_str).collect();
        download_scope(|| run_watched(Some(dest), &refs, creds, false, control)).map_err(|e| {
            crate::i18n::message(
                "backend-clone-exists-at-setting-its-sparse-folders-failed-e-open-it-to-re-d27a4c",
                &[
                    ("path", (dest.display()).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })?;
    }
    Ok(())
}

// Deliberately thread-local: consent applies only to this synchronous operation, never to
// queued/passive readers. RAII restores the policy even on unwind.
std::thread_local! { static DOWNLOADS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(super) fn downloads_allowed() -> bool {
    DOWNLOADS.with(|v| v.get())
}
pub(super) fn download_environment(args: &[&str]) -> &'static str {
    if downloads_allowed() || super::command::after_options(args).first() == Some(&"clone") {
        "0"
    } else {
        "1"
    }
}
fn download_scope<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            DOWNLOADS.with(|v| v.set(self.0));
        }
    }
    let _restore = Restore(DOWNLOADS.with(|v| v.replace(true)));
    f()
}

/// Modern Git must fail closed instead of silently downloading on a passive blob read.
/// Metadata/explicit network commands keep their compatibility and credential protocols.
pub(super) fn passive_object_read(args: &[&str]) -> bool {
    let rest = super::command::after_options(args);
    !rest.is_empty()
        && !matches!(
            rest.first().copied(),
            Some(
                "config"
                    | "init"
                    | "clone"
                    | "fetch"
                    | "pull"
                    | "push"
                    | "credential"
                    | "version"
                    | "help"
                    | "remote"
                    | "ls-remote"
                    | "lfs"
            )
        )
        && !downloads_allowed()
}

/// Cone paths are literal repository-relative directories, never patterns or arguments.
pub fn validate_folders(folders: &[String]) -> Result<(), String> {
    if folders.len() > 10_000 {
        return Err(crate::i18n::text("backend-choose-at-most-10000-folders-d49b66").to_owned());
    }
    for folder in folders {
        if folder.is_empty()
            || folder.len() > 4096
            || folder.contains(['\0', '\r', '\n', '\\'])
            || folder
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".." || p.eq_ignore_ascii_case(".git"))
            || folder.as_bytes().get(1) == Some(&b':')
        {
            return Err(crate::i18n::message(
                "backend-folder-is-not-a-repository-relative-folder-98306d",
                &[("folder", format!("{:?}", folder).into())],
            ));
        }
    }
    Ok(())
}

fn sparse_args(folders: Option<&[String]>) -> Result<Vec<String>, String> {
    match folders {
        Some(folders) => {
            validate_folders(folders)?;
            let mut args = vec!["sparse-checkout", "set", "--cone", "--skip-checks", "--"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            args.extend(folders.iter().cloned());
            Ok(args)
        }
        None => Ok(vec!["sparse-checkout".to_owned(), "disable".to_owned()]),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparseState {
    pub head: String,
    pub enabled: bool,
    pub cone: bool,
    /// Actual sparse file, used as a stale-write guard (including non-cone patterns).
    pub patterns: String,
    pub selected: BTreeSet<String>,
    pub folders: Vec<String>,
    pub partial: bool,
}

pub fn is_partial_clone(repo: &Path) -> bool {
    run(
        Some(repo),
        &[
            "config",
            "--bool",
            "--get-regexp",
            r"^remote\..*\.promisor$",
        ],
    )
    .is_ok_and(|s| {
        s.lines()
            .any(|line| line.split_whitespace().last() == Some("true"))
    }) || run(Some(repo), &["config", "--get", "extensions.partialClone"])
        .is_ok_and(|s| !s.trim().is_empty())
}

pub fn sparse_state(repo: &Path) -> Result<SparseState, String> {
    let head = super::resolve(repo, "HEAD")?;
    let enabled = super::run_ok_codes(
        Some(repo),
        &["config", "--bool", "--get", "core.sparseCheckout"],
        &[0, 1],
    )?
    .trim()
        == "true";
    let cone = super::run_ok_codes(
        Some(repo),
        &["config", "--bool", "--get", "core.sparseCheckoutCone"],
        &[0, 1],
    )?
    .trim()
        == "true";
    let sparse_file = run(
        Some(repo),
        &["rev-parse", "--git-path", "info/sparse-checkout"],
    )?;
    let path = sparse_file.trim_end_matches(['\n', '\r']);
    let sparse_file = crate::remote::resolve(repo, path);
    let patterns = patterns::read(&sparse_file)?;
    let selected = if enabled && cone {
        run(
            Some(repo),
            &["-c", "core.quotePath=false", "sparse-checkout", "list"],
        )?
        .lines()
        .map(|s| {
            if s.starts_with('"') {
                serde_json::from_str::<String>(s).map_err(|_| crate::i18n::text("backend-cannot-read-a-quoted-sparse-folder-use-git-sparse-checkout-in-the-627b53").to_owned())
            } else { Ok(s.to_owned()) }
        })
        .collect::<Result<BTreeSet<_>, _>>()?
    } else {
        BTreeSet::new()
    };
    let folders = run(
        Some(repo),
        &[
            "ls-tree",
            "-d",
            "-r",
            "-z",
            "--name-only",
            "--full-tree",
            END_OPTS,
            &head,
        ],
    )?
    .split('\0')
    .filter(|s| !s.is_empty())
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if folders.len() > 20_000 {
        return Err(crate::i18n::text(
            "backend-this-tree-has-over-20000-folders-use-git-sparse-checkout-in-the-t-d7a454",
        )
        .to_owned());
    }
    Ok(SparseState {
        head,
        enabled,
        cone,
        patterns,
        selected,
        folders,
        partial: is_partial_clone(repo),
    })
}

/// Never silently overwrite edits or change a sparse definition read before HEAD moved.
pub fn set_sparse(
    repo: &Path,
    before: &SparseState,
    folders: Option<&[String]>,
    allow_download: bool,
    credentials: Option<&Credentials>,
) -> Result<(), String> {
    if let Some(folders) = folders {
        validate_folders(folders)?;
    }
    let now = sparse_state(repo)?;
    if !same_sparse_definition(before, &now) {
        return Err(crate::i18n::text(
            "backend-head-or-sparse-checkout-changed-since-this-dialog-opened-reopen-i-15a1d8",
        )
        .to_owned());
    }
    ensure_sparse_clean(repo)?;
    if super::status(repo)?.in_progress.is_some() {
        return Err(crate::i18n::text(
            "backend-finish-the-operation-in-progress-before-changing-sparse-folders-804713",
        )
        .to_owned());
    }
    sparse_mutation(repo, folders, allow_download, credentials, super::run_with)
}

fn same_sparse_definition(before: &SparseState, now: &SparseState) -> bool {
    before.head == now.head
        && before.patterns == now.patterns
        && before.enabled == now.enabled
        && before.cone == now.cone
}

/// Only the consented mutation receives the initiating tab's account. The surrounding
/// metadata/cleanliness reads stay credential-free and cannot download missing objects.
pub(super) fn sparse_mutation(
    repo: &Path,
    folders: Option<&[String]>,
    allow_download: bool,
    credentials: Option<&Credentials>,
    execute: impl FnOnce(Option<&Path>, &[&str], Option<&Credentials>) -> Result<String, String>,
) -> Result<(), String> {
    let args = sparse_args(folders)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if allow_download {
        download_scope(|| execute(Some(repo), &args, credentials)).map(|_| ())
    } else {
        execute(Some(repo), &args, None).map(|_| ())
    }
}

fn ensure_sparse_clean(repo: &Path) -> Result<(), String> {
    // Git can remove ignored files along with a directory excluded by sparse-checkout.
    // Include ignored directories too, without enumerating their potentially large contents.
    // Refuse conservatively even for retained folders: disable/expand/non-cone transitions
    // also have collision semantics, and ignored data has no recoverable Git snapshot.
    let status = run(
        Some(repo),
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignored=matching",
        ],
    )?;
    if status.split('\0').any(|entry| entry.starts_with("!! ")) {
        return Err(crate::i18n::text(
            "backend-move-or-back-up-ignored-files-outside-the-working-tree-before-cha-cfc50b",
        )
        .to_owned());
    }
    if !status.is_empty() {
        return Err(crate::i18n::text(
            "backend-commit-stash-or-move-uncommitted-and-untracked-files-before-chang-ec2b84",
        )
        .to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// Owned real-Git coverage authored for the parent. Compile only while runtime is held.
#[cfg(test)]
mod runtime_held_tests;
