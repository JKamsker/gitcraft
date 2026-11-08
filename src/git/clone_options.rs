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
            format!(
                "Clone exists at {}. Setting its sparse folders failed: {e}. Open it to retry.",
                dest.display()
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
    allow_download: bool,
    credentials: Option<&Credentials>,
) -> Result<(), String> {
    if let Some(folders) = folders {
        validate_folders(folders)?;
    }
    let now = sparse_state(repo)?;
    if !same_sparse_definition(before, &now) {
        return Err(
            "HEAD or sparse-checkout changed since this dialog opened; reopen it".to_owned(),
        );
    }
    ensure_sparse_clean(repo)?;
    if super::status(repo)?.in_progress.is_some() {
        return Err("Finish the operation in progress before changing sparse folders".to_owned());
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
mod tests {
    use super::*;
    #[test]
    fn stale_sparse_definition_refuses_each_changed_input() {
        let before = SparseState {
            head: "old-head".into(),
            enabled: true,
            cone: true,
            patterns: "/*\n!/*/\n/src/\n".into(),
            selected: BTreeSet::new(),
            folders: Vec::new(),
            partial: true,
        };
        assert!(same_sparse_definition(&before, &before));
        for field in 0..4 {
            let mut changed = before.clone();
            match field {
                0 => changed.head = "new-head".into(),
                1 => changed.patterns.push_str("/other/\n"),
                2 => changed.enabled = false,
                _ => changed.cone = false,
            }
            assert!(!same_sparse_definition(&before, &changed));
        }
    }
    #[test]
    fn clone_plan_keeps_paths_separate_and_honors_all_options() -> Result<(), String> {
        let options = CloneOptions {
            depth: Some(12),
            blobless: true,
            folders: Some(vec!["src/space dir".into()]),
        };
        assert_eq!(
            options.args("https://example.invalid/r", "target path")?,
            [
                "clone",
                "--no-local",
                "--depth=12",
                "--filter=blob:none",
                "--sparse",
                "--",
                "https://example.invalid/r",
                "target path"
            ]
        );
        assert!(
            CloneOptions {
                depth: Some(0),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn cone_paths_reject_traversal_controls_and_config_directories() -> Result<(), String> {
        for path in [
            "../x", "/etc", "a/.git/x", "a\\x", "C:/x", "a\nx", "", "a//b",
        ] {
            assert!(validate_folders(&[path.into()]).is_err(), "{path:?}");
        }
        assert!(validate_folders(&["-literal/日本語".into()]).is_ok());
        assert_eq!(
            sparse_args(Some(&[]))?,
            ["sparse-checkout", "set", "--cone", "--skip-checks", "--"]
        );
        Ok(())
    }
    #[test]
    fn consent_is_explicit_scoped_and_restored_on_unwind() {
        assert!(passive_object_read(&[
            "-c",
            "color.ui=false",
            "show",
            "HEAD:a"
        ]));
        assert!(!passive_object_read(&["fetch", "origin"]));
        assert!(!passive_object_read(&["ls-remote", "origin"]));
        assert!(!passive_object_read(&["--version"]));
        download_scope(|| {
            assert!(!passive_object_read(&["cat-file", "blob", "abc"]));
        });
        assert!(!downloads_allowed());
        let _ =
            std::panic::catch_unwind(|| download_scope(|| std::panic::resume_unwind(Box::new(()))));
        assert!(!downloads_allowed());
    }
}

/// Owned real-Git coverage authored for the parent. Compile only while runtime is held.
#[cfg(test)]
mod runtime_held_tests {
    use super::*;
    #[test]
    fn blobless_sparse_history_stays_missing_until_explicit_consent() -> Result<(), String> {
        struct Owned(std::path::PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "gitcraft-clone-policy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let owned = Owned(root);
        let source = owned.0.join("source");
        std::fs::create_dir(&source).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "commit.gpgSign", "false"],
            vec!["config", "uploadpack.allowFilter", "true"],
        ] {
            run(Some(&source), &args)?;
        }
        std::fs::create_dir(source.join("folder")).map_err(|e| e.to_string())?;
        for content in ["old contents", "new contents"] {
            std::fs::write(source.join("folder/file"), content).map_err(|e| e.to_string())?;
            run(Some(&source), &["add", "."])?;
            run(Some(&source), &["commit", "-qm", "fixture"])?;
        }
        let dest = owned.0.join("clone");
        let url = source.to_str().ok_or("non-UTF8 fixture path")?;
        clone_with_options(
            url,
            &dest,
            None,
            &CloneOptions {
                depth: Some(2),
                blobless: true,
                folders: Some(vec![]),
            },
            &super::super::OpControl::new(|| {}),
        )?;
        assert_eq!(
            run(Some(&dest), &["rev-parse", "--is-shallow-repository"])?.trim(),
            "true"
        );
        let missing = || {
            run(
                Some(&dest),
                &["rev-list", "--objects", "--missing=print", "--all"],
            )
        };
        let before = missing()?;
        assert!(before.lines().any(|s| s.starts_with('?')));
        assert!(run(Some(&dest), &["show", "HEAD~:folder/file"]).is_err());
        assert_eq!(
            missing()?,
            before,
            "passive history selection hydrated an object"
        );
        let state = sparse_state(&dest)?;
        assert!(set_sparse(&dest, &state, Some(&["folder".into()]), false, None).is_err());
        let state = sparse_state(&dest)?;
        set_sparse(&dest, &state, Some(&["folder".into()]), true, None)?;
        assert_eq!(
            std::fs::read_to_string(dest.join("folder/file")).map_err(|e| e.to_string())?,
            "new contents"
        );
        assert!(!downloads_allowed());
        Ok(())
    }
}
