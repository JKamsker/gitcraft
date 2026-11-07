//! Create-only restoration. A retry accepts only a private, in-memory receipt for the same
//! definition and unchanged owned clone/config. Imported previous paths are display-only.
use super::{Repository, leaf};
use crate::git;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub root: PathBuf,
    id: String,
    definition: Vec<u8>,
    config: Vec<u8>,
    main_done: bool,
    worktrees: BTreeMap<String, Vec<u8>>,
    work_root: Option<PathBuf>,
}
#[derive(Clone, Debug)]
pub(crate) struct Outcome {
    pub receipt: Option<Receipt>,
    pub messages: Vec<String>,
    pub complete: bool,
}
fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit as u64 {
        return Err("Import receipt/config was replaced or exceeds limits".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Import receipt/config exceeds limits".into());
    }
    Ok(bytes)
}
impl Receipt {
    fn verify(&self, definition: &[u8]) -> Result<(), String> {
        if self.definition != definition {
            return Err("Import definition changed; use a new destination".into());
        }
        for dir in [&self.root, &self.root.join(".git")] {
            let m = std::fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err("The import repository path was replaced".into());
            }
        }
        if self.root.canonicalize().map_err(|e| e.to_string())? != self.root {
            return Err("The import repository path was replaced".into());
        }
        if read_file(&self.root.join(".git/gitcraft-setup-import"), 256)? != self.id.as_bytes()
            || read_file(&self.root.join(".git/config"), 1024 * 1024)? != self.config
        {
            return Err(
                "The import clone/config changed; finish manually instead of retrying".into(),
            );
        }
        Ok(())
    }
}
fn mkdir(path: &Path) -> Result<(), String> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
        .create(path)
        .map_err(|e| format!("Choose a new unused destination: {e}"))
}
pub(super) fn branch_ref(
    repo: &Path,
    remotes: &[super::Remote],
    branch: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    let local = format!("refs/heads/{branch}");
    let bytes = git::setup_run(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
        active,
    )?;
    let names = String::from_utf8_lossy(&bytes);
    if names.lines().any(|n| n == local) {
        return Ok(local);
    }
    for remote in remotes {
        let upstream = format!("refs/remotes/{}/{branch}", remote.name);
        if names.lines().any(|n| n == upstream) {
            return Ok(upstream);
        }
    }
    Err(format!(
        "Branch {branch} is missing; no substitute was checked out"
    ))
}

pub(super) fn worktree_is_locked(list: &[u8], path: &Path) -> Result<bool, String> {
    // Git prints C:/... while Rust canonical paths may be \\?\C:\... . Compare the
    // actual local directory identity, also handling UNC paths and directory aliases.
    let same = crate::remote::is_same_dir_as(path);
    let mut here = false;
    let mut locked = false;
    for field in list.split(|b| *b == 0) {
        if let Some(p) = field.strip_prefix(b"worktree ") {
            let p = std::str::from_utf8(p).map_err(|_| "Worktree path is not UTF-8")?;
            here = same(Path::new(p));
        } else if here && (field == b"locked" || field.starts_with(b"locked ")) {
            locked = true;
        }
    }
    Ok(locked)
}

/// Local sources are selected by the current-machine folder picker, never deserialized from
/// the setup file. Network clones use the initiating ready profile, captured by the caller.
pub(crate) fn restore(
    repository: &Repository,
    parent: &Path,
    local_source: Option<&Path>,
    previous: Option<Receipt>,
    active: &dyn Fn() -> bool,
) -> Outcome {
    let mut receipt = previous;
    let mut messages = Vec::new();
    let result = (|| -> Result<(), String> {
        repository.validate()?;
        leaf(&repository.name)?;
        if crate::remote::is_remote(parent) || !parent.is_absolute() {
            return Err("Import targets must be local absolute folders on this machine".into());
        }
        let parent = parent.canonicalize().map_err(|e| e.to_string())?;
        if parent.to_str().is_none() {
            return Err("Choose a destination with a UTF-8 path".into());
        }
        let definition = serde_json::to_vec(repository).map_err(|e| e.to_string())?;
        if let Some(r) = &receipt {
            r.verify(&definition)?;
            if r.root != parent.join(&repository.name) {
                return Err("Retry destination changed".into());
            }
        } else {
            if !active() {
                return Err("Import cancelled".into());
            }
            let source = if let Some(path) = local_source {
                if !path.is_absolute() || crate::remote::is_remote(path) {
                    return Err("Choose a local absolute source folder on this machine".into());
                }
                path.canonicalize()
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("Choose a local source with a UTF-8 path")?
                    .to_owned()
            } else {
                repository
                    .remotes
                    .first()
                    .ok_or("No portable remote; select a local source folder on this machine")?
                    .fetch
                    .clone()
            };
            if local_source.is_none()
                && let Some(pause) = crate::network::refused()
            {
                return Err(pause.why().into());
            }
            let target = parent.join(&repository.name);
            mkdir(&target)?;
            let remote = repository
                .remotes
                .first()
                .map_or("origin", |r| r.name.as_str());
            git::setup_clone(
                &parent,
                &source,
                &target,
                remote,
                local_source.is_some(),
                active,
            )
            .map_err(|e| format!("Clone failed; destination retained for inspection: {e}"))?;
            let root = target.canonicalize().map_err(|e| e.to_string())?;
            let id = format!(
                "gitcraft-setup-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
            );
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join(".git/gitcraft-setup-import"))
                .and_then(|mut f| f.write_all(id.as_bytes()))
                .map_err(|e| e.to_string())?;
            let config = read_file(&root.join(".git/config"), 1024 * 1024)?;
            receipt = Some(Receipt {
                root,
                id,
                definition,
                config,
                main_done: false,
                worktrees: BTreeMap::new(),
                work_root: None,
            });
        }
        let r = receipt.as_mut().ok_or("Missing import receipt")?;
        let repo = &r.root;
        let steps = (|| -> Result<(), String> {
            if !r.main_done {
                let existing = git::setup_run(repo, &["remote"], active)?;
                let existing = String::from_utf8_lossy(&existing);
                for entry in &repository.remotes {
                    if existing.lines().any(|name| name == entry.name) {
                        git::setup_run(
                            repo,
                            &["remote", "set-url", "--", &entry.name, &entry.fetch],
                            active,
                        )?;
                    } else {
                        git::setup_run(
                            repo,
                            &["remote", "add", "--", &entry.name, &entry.fetch],
                            active,
                        )?;
                    }
                    for (i, url) in entry.push.iter().enumerate() {
                        let key = format!("remote.{}.pushurl", entry.name);
                        git::setup_run(
                            repo,
                            &[
                                "config",
                                "--local",
                                if i == 0 { "--replace-all" } else { "--add" },
                                &key,
                                url,
                            ],
                            active,
                        )?;
                    }
                }
                // A local source is selected explicitly instead of contacting imported URLs.
                // Network imports fetch secondary heads too, so linked worktrees do not
                // silently disappear when their branches belong to another remote.
                if local_source.is_none() {
                    for entry in repository.remotes.iter().skip(1) {
                        git::setup_fetch(repo, &entry.fetch, &entry.name, active)?;
                    }
                }
                for (key, value) in [
                    ("user.name", &repository.author_name),
                    ("user.email", &repository.author_email),
                ] {
                    if let Some(value) = value {
                        git::setup_run(repo, &["config", "--local", key, value], active)?;
                    }
                }
                for (key, value) in &repository.config {
                    git::setup_run(
                        repo,
                        &["config", "--local", "--replace-all", key, value],
                        active,
                    )?;
                }
                match (&repository.branch, &repository.commit) {
                    (Some(branch), Some(_)) => {
                        let from = branch_ref(repo, &repository.remotes, branch, active)?;
                        if from.starts_with("refs/heads/") {
                            git::setup_run(
                                repo,
                                &["checkout", "--no-guess", branch, "--"],
                                active,
                            )?;
                        } else {
                            git::setup_run(
                                repo,
                                &["checkout", "--no-guess", "-b", branch, &from, "--"],
                                active,
                            )?;
                        }
                    }
                    (None, Some(commit)) => {
                        git::setup_run(
                            repo,
                            &[
                                "rev-parse",
                                "--verify",
                                "--end-of-options",
                                &format!("{commit}^{{commit}}"),
                            ],
                            active,
                        )
                        .map_err(|_| {
                            "Detached commit is unavailable; no substitute was used".to_owned()
                        })?;
                        git::setup_run(repo, &["checkout", "--detach", commit, "--"], active)?;
                    }
                    (Some(branch), None) => {
                        git::setup_run(
                            repo,
                            &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
                            active,
                        )?;
                    }
                    _ => (),
                }
                r.main_done = true;
                if let Some(expected) = &repository.commit {
                    let actual =
                        git::setup_run(repo, &["rev-parse", "--revs-only", "HEAD"], active)?;
                    if String::from_utf8_lossy(&actual).trim() != expected {
                        messages.push("The named branch uses the selected source's current tip. Local-only commits and working files are not included in setup files.".into());
                    }
                }
            }
            for w in &repository.worktrees {
                if !active() {
                    return Err("Import cancelled; completed clone/worktrees were retained".into());
                }
                let root = if let Some(path) = &r.work_root {
                    path.clone()
                } else {
                    let path = parent.join(format!("{}-worktrees", repository.name));
                    mkdir(&path)?;
                    r.work_root = Some(path.clone());
                    path
                };
                if root.canonicalize().map_err(|e| e.to_string())? != root
                    || std::fs::symlink_metadata(&root)
                        .map_err(|e| e.to_string())?
                        .file_type()
                        .is_symlink()
                {
                    return Err("Worktree destination root was replaced".into());
                }
                let path = root.join(&w.name);
                let already_added = if let Some(gitfile) = r.worktrees.get(&w.name) {
                    let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                    if !metadata.is_dir()
                        || metadata.file_type().is_symlink()
                        || read_file(&path.join(".git"), 8192)? != *gitfile
                    {
                        return Err(format!(
                            "Previously restored worktree {} was replaced; finish manually",
                            w.name
                        ));
                    }
                    true
                } else {
                    if std::fs::symlink_metadata(&path).is_ok() {
                        return Err(format!(
                            "Worktree {} already exists and was not overwritten",
                            w.name
                        ));
                    }
                    false
                };
                if !already_added {
                    match &w.branch {
                        Some(branch) => {
                            let from = branch_ref(repo, &repository.remotes, branch, active)?;
                            if from.starts_with("refs/heads/") {
                                git::setup_run(
                                    repo,
                                    &["worktree", "add", "--", &path.to_string_lossy(), branch],
                                    active,
                                )?;
                            } else {
                                git::setup_run(
                                    repo,
                                    &[
                                        "worktree",
                                        "add",
                                        "-b",
                                        branch,
                                        "--",
                                        &path.to_string_lossy(),
                                        &from,
                                    ],
                                    active,
                                )?;
                            }
                        }
                        None => {
                            git::setup_run(
                                repo,
                                &[
                                    "rev-parse",
                                    "--verify",
                                    "--end-of-options",
                                    &format!("{}^{{commit}}", w.commit),
                                ],
                                active,
                            )
                            .map_err(|_| {
                                format!(
                                    "Worktree {} commit is unavailable; no substitute was used",
                                    w.name
                                )
                            })?;
                            git::setup_run(
                                repo,
                                &[
                                    "worktree",
                                    "add",
                                    "--detach",
                                    "--",
                                    &path.to_string_lossy(),
                                    &w.commit,
                                ],
                                active,
                            )?;
                        }
                    }
                    let gitfile = read_file(&path.join(".git"), 8192)?;
                    r.worktrees.insert(w.name.clone(), gitfile);
                }
                if w.was_locked {
                    // The preceding add is retained even when cancellation interrupts locking.
                    let list =
                        git::setup_run(repo, &["worktree", "list", "--porcelain", "-z"], active)?;
                    if !worktree_is_locked(&list, &path)? {
                        git::setup_run(
                            repo,
                            &["worktree", "lock", "--", &path.to_string_lossy()],
                            active,
                        )?;
                    }
                }
            }
            Ok(())
        })();
        // Retain only the configuration produced by this attempt for later exact retries.
        r.config = read_file(&r.root.join(".git/config"), 1024 * 1024)?;
        steps
    })();
    match result {
        Ok(()) => {
            messages.push("Selected checkout, remotes, author and worktree steps completed. Review any notices above.".into());
            Outcome {
                receipt,
                messages,
                complete: true,
            }
        }
        Err(e) => {
            messages.push(e);
            Outcome {
                receipt,
                messages,
                complete: false,
            }
        }
    }
}
