//! Repository-wide worktree locations and local ignores for worktrees inside a checkout.

use std::path::{Path, PathBuf};

use crate::remote;

pub const WORKTREE_ROOT_KEY: &str = "gitcraft.worktreeRoot";

/// Kept in the common config, so opening a linked worktree uses the same root.
pub fn read_worktree_root(repo: &Path) -> Result<String, String> {
    super::run_ok_codes(
        Some(repo),
        &["config", "--local", "--get", WORKTREE_ROOT_KEY],
        &[0, 1],
    )
    .map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
}

/// A sibling folder of the main checkout: `/code/app` → `/code/app.wt`.
pub fn default_worktree_root(main: &Path) -> PathBuf {
    PathBuf::from(format!("{}.wt", main.display()))
}

/// Relative roots are always relative to the main checkout, including from a linked one.
pub fn resolve_worktree_root(main: &Path, root: &str) -> Result<PathBuf, String> {
    if root.trim().is_empty() {
        return Ok(default_worktree_root(main));
    }
    let root = remote::folder_for_git(main, root)?;
    if root == "~" || root.starts_with("~/") {
        return Err("Use a full path or a path relative to the main checkout".to_owned());
    }
    Ok(remote::resolve(main, &root))
}

/// Preserve branch namespaces as folders. Invalid input while typing must never escape
/// the root; git still validates the actual branch name when adding the worktree.
pub fn branch_worktree_folder(root: &Path, branch: &str) -> PathBuf {
    let parts: Vec<String> = branch
        .trim()
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| match part {
            "." | ".." => "-".to_owned(),
            _ => part.replace(['\\', ':', ' '], "-"),
        })
        .collect();
    let suffix = if parts.is_empty() {
        "worktree".to_owned()
    } else {
        parts.join("/")
    };
    remote::join(root, &suffix)
}

/// Before `worktree add`, create its parents and ignore an internal root locally. This runs
/// in the operation worker for manual worktrees, PRs, issues and agent tasks alike.
pub fn prepare_worktree_folder(repo: &Path, folder: &str) -> Result<(), String> {
    let destination = remote::resolve(repo, folder);
    let parent = destination
        .parent()
        .ok_or("The worktree needs a parent folder")?;
    remote::create_dir_all(parent)
        .map_err(|e| format!("Could not create worktree folders: {e}"))?;
    // Resolve symlinks and `..` on the repository's machine, after the parents exist.
    let parent = canonical_dir(parent)?;
    let destination = remote::join(
        &parent,
        &destination
            .file_name()
            .ok_or("The worktree needs a folder name")?
            .to_string_lossy(),
    );
    let list = super::worktree_list(repo, None)?;
    let main = list.first().map_or(repo, |w| w.path.as_path());
    let configured = resolve_worktree_root(main, &read_worktree_root(repo)?)?;
    let root = canonical_dir(&configured).ok();
    for wt in list.iter().filter(|w| !w.bare && w.prunable.is_none()) {
        let checkout = match canonical_dir(&wt.path) {
            Ok(checkout) => checkout,
            // A locked worktree can be missing without being marked prunable.
            Err(_) if remote::try_is_dir(&wt.path).is_ok_and(|exists| !exists) => continue,
            Err(e) => return Err(e),
        };
        let Ok(relative) = destination.strip_prefix(&checkout) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        // Never ignore the entire checkout, even when the root was set to `.` or `..`.
        let ignored = root
            .as_deref()
            .filter(|r| destination.starts_with(r))
            .and_then(|r| r.strip_prefix(&checkout).ok())
            .filter(|r| !r.as_os_str().is_empty())
            .unwrap_or(relative);
        exclude_folder(repo, ignored)?;
        let folder = ignored
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        // A repository's .gitignore can override info/exclude with a negation. Say so before
        // creating a nested checkout that would otherwise show up as an untracked repository.
        super::run(
            Some(&wt.path),
            &["check-ignore", "--no-index", "--quiet", "--", &format!("{folder}/")],
        ).map_err(|_| format!(
            "Could not ignore {folder}: check for a .gitignore rule that overrides local excludes. \
             Ignore this folder in .gitignore or choose a root outside the checkout."
        ))?;
    }
    Ok(())
}

fn canonical_dir(dir: &Path) -> Result<PathBuf, String> {
    if let Some(target) = remote::parse(dir) {
        let output = remote::exec(
            &target.host,
            &remote::Request {
                script: format!("cd -- {} && pwd -P", remote::sh_path(&target.path)),
                ..Default::default()
            },
        )?;
        if output.code != 0 {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        let path = String::from_utf8_lossy(&output.stdout);
        return Ok(remote::encode(
            &target.host,
            path.trim_end_matches(['\r', '\n']),
        ));
    }
    dir.canonicalize()
        .map_err(|e| format!("Could not resolve {}: {e}", dir.display()))
}

fn exclude_folder(repo: &Path, relative: &Path) -> Result<(), String> {
    // Anchored, literal directory pattern, including roots with spaces or glob characters.
    let name = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if name.contains(['\r', '\n']) {
        return Err("A worktree folder inside a checkout cannot contain a line break".to_owned());
    }
    let mut pattern = String::from("/");
    for c in name.chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | ']' | ' ') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('/');
    let printed = super::run(Some(repo), &["rev-parse", "--git-path", "info/exclude"])?;
    let file = remote::resolve(repo, printed.trim_end_matches(['\r', '\n']));
    let mut text = match remote::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read info/exclude: {e}")),
    };
    if text.lines().any(|line| line == pattern) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("# Worktree folders managed by gitcraft\n");
    text.push_str(&pattern);
    text.push('\n');
    if let Some(parent) = file.parent() {
        remote::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    remote::write(&file, text.as_bytes()).map_err(|e| format!("Could not write info/exclude: {e}"))
}
