//! The machine the work leaves: what it is (branch, commit, remotes), whether it can go, and
//! its snapshot, taken without touching it. Two trees are written through a temporary index
//! (`GIT_INDEX_FILE`, a copy of the real one): the index as it is, then the working tree on
//! top of it with `add -A` (so untracked files that aren't ignored too). Each becomes a commit
//! (`commit-tree`), the working tree's on the index's on the checked-out one, held by a
//! temporary ref while it is bundled.

use std::path::{Path, PathBuf};

use super::protocol;
use crate::git::{self, Call};

/// The repository the work leaves from.
#[derive(Clone, Debug)]
pub struct Source {
    /// Its top folder, as this app names it.
    pub repo: PathBuf,
    /// The commit checked out.
    pub head: String,
    /// The branch checked out; `None` when detached.
    pub branch: Option<String>,
    /// The branch's upstream: the remote's name and the branch there (`refs/heads/x`).
    pub upstream: Option<(String, String)>,
    /// Its remotes: (name, URL).
    pub remotes: Vec<(String, String)>,
    /// `sha1` or `sha256`.
    pub object_format: String,
}

/// Who the snapshot's commits are by: they never become a branch's, and a repository without
/// a name and email set can still take them.
const SNAPSHOT_AUTHOR: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "gitcraft teleport"),
    ("GIT_AUTHOR_EMAIL", "teleport@gitcraft.invalid"),
    ("GIT_COMMITTER_NAME", "gitcraft teleport"),
    ("GIT_COMMITTER_EMAIL", "teleport@gitcraft.invalid"),
];

fn git_text(repo: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(repo), args)
}

/// What `repo` (a working tree) is, or why its work can't be teleported.
pub fn inspect(repo: &Path) -> Result<Source, String> {
    let repo = git::toplevel(repo)?;
    let head = git_text(&repo, &["rev-parse", "--verify", "-q", "HEAD^{commit}"])
        .map(|h| h.trim().to_owned())
        .map_err(|_| "Nothing is committed here yet: commit once, then teleport.".to_owned())?;
    if let Some(op) = git::in_progress(&repo)? {
        return Err(format!(
            "A {} is in progress here: finish or abort it first.",
            op.label().to_lowercase()
        ));
    }
    let unmerged = git::run_bytes(Some(&repo), &["ls-files", "-u", "-z"])?;
    if !unmerged.is_empty() {
        return Err("Some files have conflicts: resolve them first.".to_owned());
    }
    if git_text(&repo, &["rev-parse", "--is-shallow-repository"]).is_ok_and(|s| s.trim() == "true")
    {
        return Err(
            "This is a shallow clone: its history stops short, and the repository there would \
             be missing the commits it builds on. Fetch the whole history first (git fetch \
             --unshallow)."
                .to_owned(),
        );
    }
    refuse_hidden_changes(&repo)?;
    let staged = git::run_bytes(Some(&repo), &["ls-files", "-s", "-z"])?;
    if staged
        .split(|&b| b == 0)
        .any(|entry| entry.starts_with(b"160000 "))
    {
        return Err("This repository has submodules, which teleport doesn't carry.".to_owned());
    }
    let branch = git_text(&repo, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .map(|b| b.trim().to_owned())
        .filter(|b| !b.is_empty());
    let upstream = branch.as_deref().and_then(|b| upstream_of(&repo, b));
    let remotes = git::remotes(&repo)?
        .into_iter()
        .map(|r| (r.name, r.url))
        .collect();
    let object_format = object_format(&repo);
    Ok(Source {
        repo,
        head,
        branch,
        upstream,
        remotes,
        object_format,
    })
}

/// Files git is told to look away from (`update-index --assume-unchanged`, or
/// `--skip-worktree` outside a sparse checkout): `add -A` doesn't see their changes, which
/// would stay behind without a word.
fn refuse_hidden_changes(repo: &Path) -> Result<(), String> {
    let listed = git::run_bytes(Some(repo), &["ls-files", "-v", "-z"])?;
    let sparse = git_text(repo, &["config", "--bool", "core.sparseCheckout"])
        .is_ok_and(|v| v.trim() == "true");
    for entry in listed.split(|&b| b == 0).filter(|e| e.len() > 2) {
        let path = String::from_utf8_lossy(&entry[2..]);
        let hidden = match entry[0] {
            b'S' => !sparse,
            tag => tag.is_ascii_lowercase(),
        };
        if hidden {
            return Err(format!(
                "{path} is marked so that git doesn't look at its changes (assume-unchanged \
                 or skip-worktree), and they wouldn't be carried. Clear the mark first."
            ));
        }
    }
    Ok(())
}

/// `sha1` or `sha256` (git before 2.29 knows only the first, and doesn't answer).
pub(super) fn object_format(repo: &Path) -> String {
    git_text(repo, &["rev-parse", "--show-object-format"])
        .map(|f| f.trim().to_owned())
        .ok()
        .filter(|f| !f.is_empty() && !f.starts_with("--"))
        .unwrap_or_else(|| "sha1".to_owned())
}

/// The remote and the branch there that `branch` follows, if it follows one.
fn upstream_of(repo: &Path, branch: &str) -> Option<(String, String)> {
    let get = |key: &str| {
        git_text(
            repo,
            &["config", "--get", &format!("branch.{branch}.{key}")],
        )
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
    };
    let remote = get("remote")?;
    // `.` is the repository itself: nothing to follow on another machine.
    (remote != ".").then_some(())?;
    Some((remote, get("merge")?))
}

/// The work of a [`Source`], held by a temporary ref until it is bundled. Dropped, it removes
/// what it made on the source (the ref, the temporary index); [`Snapshot::clean_up`] does so
/// and says what failed.
pub struct Snapshot {
    repo: PathBuf,
    /// The temporary index, as this app names it.
    index_file: PathBuf,
    /// The ref that holds [`Self::worktree`].
    pub(super) ref_name: String,
    /// The commit of the index, on the one checked out.
    pub index: String,
    /// The commit of the working tree, on the index's.
    pub worktree: String,
    /// Files added with `add -N` (intent to add): in no tree of the index, but known to it.
    /// As git names them, byte for byte.
    pub intent_to_add: Vec<Vec<u8>>,
    /// Some committed files are kept in Git LFS: on the other machine they arrive as their
    /// pointers.
    pub lfs: bool,
    cleaned: bool,
}

impl Snapshot {
    /// Takes the snapshot of `src`, `id` naming what it makes, or says why it can't.
    pub fn take(src: &Source, id: &str) -> Result<Self, String> {
        let repo = &src.repo;
        let path_of = |name: &str| -> Result<PathBuf, String> {
            let printed = git_text(repo, &["rev-parse", "--git-path", name])?;
            Ok(crate::remote::resolve(
                repo,
                printed.trim_end_matches(['\n', '\r']),
            ))
        };
        let real_index = path_of("index")?;
        let index_file = path_of(&protocol::temp_index_name(id))?;
        let mut snap = Self {
            repo: repo.clone(),
            index_file,
            ref_name: protocol::snapshot_ref(id),
            index: String::new(),
            worktree: String::new(),
            intent_to_add: Vec::new(),
            lfs: false,
            cleaned: false,
        };
        // No index at all is an empty one, as git reads it.
        super::files::copy_if_there(&real_index, &snap.index_file)?;
        snap.write(src)?;
        Ok(snap)
    }

    /// The variables that point git at the temporary index.
    fn env(&self) -> Vec<(&'static str, String)> {
        vec![("GIT_INDEX_FILE", crate::remote::git_path(&self.index_file))]
    }

    /// git on the temporary index, its output.
    fn git(&self, args: &[&str], stdin: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
        let mut env = self.env();
        env.extend(SNAPSHOT_AUTHOR.map(|(n, v)| (n, v.to_owned())));
        git::run_call(
            Some(&self.repo),
            args,
            Call {
                env,
                stdin,
                long: false,
            },
        )
    }

    fn git_line(&self, args: &[&str]) -> Result<String, String> {
        Ok(String::from_utf8_lossy(&self.git(args, None)?)
            .trim()
            .to_owned())
    }

    /// The trees, their commits, the checks on what they hold, then the ref.
    fn write(&mut self, src: &Source) -> Result<(), String> {
        // The index copied is of the commit the checks were made for: one made or checked out
        // since (in a terminal) would be told apart wrongly into staged changes.
        let head_now = self.git_line(&["rev-parse", "--verify", "-q", "HEAD^{commit}"]);
        let branch_now = git_text(&self.repo, &["symbolic-ref", "-q", "--short", "HEAD"])
            .ok()
            .map(|b| b.trim().to_owned())
            .filter(|b| !b.is_empty());
        if head_now.as_deref() != Ok(src.head.as_str()) || branch_now != src.branch {
            return Err(
                "The checked-out commit changed while the teleport got ready: try again."
                    .to_owned(),
            );
        }
        let index_tree = self.git_line(&["write-tree"])?;
        self.intent_to_add = protocol::parse_raw_paths(&self.git(
            &["diff-files", "--name-only", "-z", "--diff-filter=A"],
            None,
        )?);
        // Before `add -A`, whose LFS filter would put their contents in this repository's LFS
        // store.
        self.check_lfs()?;
        self.git(&["add", "-A"], None)
            .map_err(|e| format!("Couldn't take a snapshot of the working tree: {e}"))?;
        let worktree_tree = self.git_line(&["write-tree"])?;
        let message = "gitcraft teleport: staged changes";
        self.index = self.git_line(&[
            "commit-tree",
            "--no-gpg-sign",
            "-p",
            &src.head,
            "-m",
            message,
            &index_tree,
        ])?;
        self.worktree = self.git_line(&[
            "commit-tree",
            "--no-gpg-sign",
            "-p",
            &self.index,
            "-m",
            "gitcraft teleport: working tree",
            &worktree_tree,
        ])?;
        self.refuse_nested_repositories(&src.head)?;
        self.git(&["update-ref", &self.ref_name, &self.worktree, ""], None)?;
        Ok(())
    }

    /// A folder with a repository of its own in it, which `add -A` takes for a submodule.
    fn refuse_nested_repositories(&self, head: &str) -> Result<(), String> {
        let out = self.git(
            &[
                "diff-tree",
                "-r",
                "-z",
                "--no-renames",
                head,
                &self.worktree,
            ],
            None,
        )?;
        match protocol::parse_gitlinks(&out).first() {
            Some(path) => Err(format!(
                "{path} is a repository of its own inside this one, which teleport doesn't \
                 carry: move it out or ignore it first."
            )),
            None => Ok(()),
        }
    }

    /// Changed files kept in Git LFS: their contents are in this machine's LFS store only, and
    /// would arrive as pointers to nothing. Committed ones arrive as pointers, fetched there.
    /// Asked of git for every changed path, so that any `.gitattributes` (a folder's own,
    /// `info/attributes`, the user's) counts.
    fn check_lfs(&mut self) -> Result<(), String> {
        self.lfs = git::uses_lfs(&self.repo);
        let mut changed = protocol::parse_raw_paths(&self.git(
            &["diff-index", "--cached", "--name-only", "-z", "HEAD"],
            None,
        )?);
        changed.extend(protocol::parse_raw_paths(&self.git(
            &[
                "ls-files",
                "-z",
                "--modified",
                "--others",
                "--exclude-standard",
            ],
            None,
        )?));
        changed.sort();
        changed.dedup();
        if changed.is_empty() {
            return Ok(());
        }
        let out = self.git(
            &["check-attr", "-z", "--stdin", "filter"],
            Some(protocol::nul_list(&changed)),
        )?;
        match protocol::parse_lfs_paths(&out).first() {
            Some(path) => Err(format!(
                "{path} is kept in Git LFS and has changes that aren't committed: their \
                 contents can't be carried. Commit and push them first."
            )),
            None => Ok(()),
        }
    }

    /// The bundle of the snapshot, without the commits in `bases` (the other machine has them)
    /// and what they reach.
    pub fn bundle(&self, bases: &[String]) -> Result<Vec<u8>, String> {
        let revs = protocol::bundle_revs(&self.ref_name, bases);
        git::run_call(
            Some(&self.repo),
            &["bundle", "create", "-", "--stdin"],
            Call {
                env: Vec::new(),
                stdin: Some(revs.into_bytes()),
                long: true,
            },
        )
    }

    /// Of `tips` (commits the other machine has), those this one has too.
    pub fn present(&self, tips: &[String]) -> Result<Vec<String>, String> {
        if tips.is_empty() {
            return Ok(Vec::new());
        }
        let mut input = tips.join("\n");
        input.push('\n');
        let out = git::run_call(
            Some(&self.repo),
            &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
            Call {
                stdin: Some(input.into_bytes()),
                ..Call::default()
            },
        )?;
        Ok(protocol::parse_present(&String::from_utf8_lossy(&out)))
    }

    /// Removes the ref and the temporary index, saying what couldn't be.
    pub fn clean_up(&mut self) -> Result<(), String> {
        if std::mem::replace(&mut self.cleaned, true) {
            return Ok(());
        }
        self.remove()
    }

    fn remove(&self) -> Result<(), String> {
        // One never made (the snapshot failed before) is no error.
        let ref_gone =
            git::run(Some(&self.repo), &["update-ref", "-d", &self.ref_name]).map(|_| ());
        let index_gone = super::files::remove_file(&self.index_file);
        // A lock git left on the temporary index (it was stopped), never the real one's.
        let lock = PathBuf::from(format!("{}.lock", self.index_file.display()));
        let _ = super::files::remove_file(&lock);
        ref_gone.and(index_gone)
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.remove();
        }
    }
}

/// Whether `ancestor` is in the history of `commit` in `repo` (both known to it).
pub(super) fn is_ancestor(repo: &Path, ancestor: &str, commit: &str) -> bool {
    git::run(
        Some(repo),
        &["merge-base", "--is-ancestor", ancestor, commit],
    )
    .is_ok()
}

/// Whether `repo` has the commit `hash`.
pub(super) fn has_commit(repo: &Path, hash: &str) -> bool {
    git::run(
        Some(repo),
        &["cat-file", "-e", &format!("{hash}^{{commit}}")],
    )
    .is_ok()
}
