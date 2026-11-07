//! Clone options and cone-mode sparse checkouts. Arguments stay separate on every machine.

use std::collections::BTreeSet;
use std::path::Path;

use super::{Credentials, END_OPTS, clone_from, run, run_with};

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
            return Err("Depth must be at least 1".to_owned());
        }
        if let Some(folders) = &self.folders {
            validate_folders(folders)?;
        }
        Ok(())
    }

    fn args(&self, url: &str, dest: &str) -> Result<Vec<String>, String> {
        self.validate()?;
        if url.is_empty() || url.contains(['\0', '\n', '\r']) {
            return Err("Enter a repository URL or path on one line".to_owned());
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

pub fn clone_with_options(
    url: &str,
    dest: &Path,
    creds: Option<&Credentials>,
    options: &CloneOptions,
) -> Result<(), String> {
    let (from, there) = match crate::remote::parse(dest) {
        Some(target) => {
            let (from, there) = clone_from(&target.path);
            (Some(crate::remote::encode(&target.host, from)), there)
        }
        None => (None, dest.to_string_lossy().into_owned()),
    };
    let args = options.args(url, &there)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_with(from.as_deref(), &args, creds)?;
    if let Some(folders) = &options.folders {
        // The initial sparse clone has root files only; add the selected folders.
        // Local checkout hooks/templates may already have created ignored user data.
        let apply = || {
            ensure_sparse_clean(dest)?;
            let args = sparse_args(Some(folders))?;
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            run_with(Some(dest), &args, creds).map(|_| ())
        };
        apply().map_err(|e| {
            format!("Clone was created at {} but setting its folders failed: {e}. Open it and use Sparse checkout… to try again.", dest.display())
        })?;
    }
    Ok(())
}

/// Cone paths are literal repository-relative directories, never patterns or arguments.
pub fn validate_folders(folders: &[String]) -> Result<(), String> {
    if folders.len() > 10_000 {
        return Err("Choose at most 10000 folders".to_owned());
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
            return Err(format!("{folder:?} is not a repository-relative folder"));
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

/// Inspect a file's blob without triggering Git's lazy fetch. --missing=print is essential:
/// plain cat-file -e can download the very object the caller is asking about.
pub fn file_needs_download(repo: &Path, revision: &str, path: &str) -> Result<bool, String> {
    let tree = run(
        Some(repo),
        &[
            super::LITERAL_PATHS,
            "ls-tree",
            "-z",
            "--full-tree",
            END_OPTS,
            revision,
            "--",
            path,
        ],
    )?;
    for record in tree.split('\0').filter(|s| !s.is_empty()) {
        let metadata = record.split_once('\t').map(|(m, _)| m).unwrap_or_default();
        let fields: Vec<&str> = metadata.split_whitespace().collect();
        if let [_, "blob", oid] = fields.as_slice() {
            let answer = run(
                Some(repo),
                &[
                    "rev-list",
                    "--objects",
                    "--missing=print",
                    "--no-object-names",
                    END_OPTS,
                    oid,
                ],
            )?;
            if answer
                .lines()
                .any(|line| line.strip_prefix('?') == Some(*oid))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
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
    if crate::remote::try_is_link(&sparse_file).map_err(|e| e.to_string())? {
        return Err(
            "The sparse-checkout file is a symbolic link; use the terminal to inspect it"
                .to_owned(),
        );
    }
    let patterns = match crate::remote::read_to_string(&sparse_file) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read sparse-checkout: {e}")),
    };
    let selected = if enabled && cone {
        run(
            Some(repo),
            &["-c", "core.quotePath=false", "sparse-checkout", "list"],
        )?
        .lines()
        .map(|s| {
            if s.starts_with('"') {
                serde_json::from_str::<String>(s).map_err(|_| "Cannot read a quoted sparse folder; use git sparse-checkout in the terminal".to_owned())
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
        return Err(
            "This tree has over 20000 folders; use git sparse-checkout in the terminal".to_owned(),
        );
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
) -> Result<(), String> {
    if let Some(folders) = folders {
        validate_folders(folders)?;
    }
    let now = sparse_state(repo)?;
    if now.head != before.head
        || now.patterns != before.patterns
        || now.enabled != before.enabled
        || now.cone != before.cone
    {
        return Err(
            "HEAD or sparse-checkout changed since this dialog opened; reopen it".to_owned(),
        );
    }
    ensure_sparse_clean(repo)?;
    if super::status(repo)?.in_progress.is_some() {
        return Err("Finish the operation in progress before changing sparse folders".to_owned());
    }
    let args = sparse_args(folders)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(Some(repo), &args).map(|_| ())
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
        return Err(
            "Move or back up ignored files outside the working tree before changing sparse folders. Git can delete ignored files during sparse checkout."
                .to_owned(),
        );
    }
    if !status.is_empty() {
        return Err(
            "Commit, stash or move uncommitted and untracked files before changing sparse folders"
                .to_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
