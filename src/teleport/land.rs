//! The machine the work arrives on: the repository it goes in (found, cloned or made), the
//! checks made before anything is sent, and the landing once the bundle is there. The bundle
//! is verified (`bundle verify`), unpacked (`bundle unbundle`, whose `index-pack` checks every
//! object), and the work checked out in a new worktree: the working tree's snapshot written
//! to disk (`read-tree -u`), then the index's put in the index alone (`read-tree`), so what
//! was staged is staged, what wasn't isn't, and what was untracked is untracked again.
//!
//! What a failed landing made is taken back when its [`Target`] is dropped: the worktree, the
//! branch (or where it was), a repository made for it.

use std::path::{Path, PathBuf};

use super::source::{self, Snapshot, Source};
use super::{Machine, files, protocol, urls};
use crate::git::{self, Call};

/// Where the work goes on the other machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    /// A repository there (any of its worktrees): the work goes in a new worktree of it.
    Repo(PathBuf),
    /// The project cloned there first, from `url` into `path`; then as [`Destination::Repo`].
    Clone { url: String, path: PathBuf },
    /// A new repository at `path`, made from the bundle, with the source's remotes: the work
    /// goes in its own working tree.
    Create(PathBuf),
}

/// How the branch is put on the other machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BranchPlan {
    /// Made there.
    New(String),
    /// There already, on the very commit.
    Same(String),
    /// There already, behind: moved on to the commit.
    FastForward { name: String, from: String },
    /// No branch: the commit, detached.
    Detached,
}

/// The repository the work arrives in, checked, and what landing in it made so far.
pub(super) struct Target {
    pub(super) machine: Machine,
    pub(super) repo: PathBuf,
    /// The folder the work lands in: a new worktree, or the repository's own working tree.
    pub(super) folder: PathBuf,
    pub(super) branch: BranchPlan,
    /// Made for this teleport ([`Destination::Create`]): removed if it fails.
    fresh: bool,
    /// Cloned for this teleport: kept if it fails, as a clone is costly to make again.
    pub(super) cloned: bool,
    /// What was done there, taken back unless it [`Self::landed`].
    worktree_added: bool,
    branch_made: Option<String>,
    branch_moved: Option<(String, String)>,
    landed: bool,
}

/// What the work needs from the other machine that a folder there is checked for.
pub(super) struct Wanted<'a> {
    pub(super) branch: Option<&'a str>,
    /// Where the new worktree goes (unused for [`Destination::Create`]).
    pub(super) folder: &'a Path,
}

impl Target {
    /// The repository `dest` names, made or cloned if it says so, checked for the work of
    /// `src`: same kind of hashes, a branch name that can be used there, a free folder.
    pub(super) fn prepare(dest: &Destination, src: &Source, want: &Wanted) -> Result<Self, String> {
        match dest {
            Destination::Repo(path) => Self::existing(git::toplevel(path)?, src, want, false),
            Destination::Clone { url, path } => {
                let machine = Machine::of(path);
                if files::exists(path)? {
                    return Err(format!(
                        "{} is already there on {}: pick another folder.",
                        crate::remote::git_path(path),
                        machine.label()
                    ));
                }
                git::clone(url, path, None)
                    .map_err(|e| format!("Could not clone {url} on {}: {e}", machine.label()))?;
                Self::existing(git::toplevel(path)?, src, want, true)
            }
            Destination::Create(path) => Self::create(path, src, want),
        }
    }

    fn existing(repo: PathBuf, src: &Source, want: &Wanted, cloned: bool) -> Result<Self, String> {
        let machine = Machine::of(&repo);
        same_hashes(&repo, src, &machine)?;
        let branch = branch_plan(&repo, src, want.branch, &machine)?;
        if files::exists(want.folder)? {
            return Err(format!(
                "{} is already there on {}: pick another folder.",
                crate::remote::git_path(want.folder),
                machine.label()
            ));
        }
        Ok(Self {
            machine,
            repo,
            folder: want.folder.to_path_buf(),
            branch,
            fresh: false,
            cloned,
            worktree_added: false,
            branch_made: None,
            branch_moved: None,
            landed: false,
        })
    }

    fn create(path: &Path, src: &Source, want: &Wanted) -> Result<Self, String> {
        let machine = Machine::of(path);
        if files::exists(path)? {
            return Err(format!(
                "{} is already there on {}: pick another folder.",
                crate::remote::git_path(path),
                machine.label()
            ));
        }
        let branch = match want.branch {
            Some(name) => BranchPlan::New(name.to_owned()),
            None => BranchPlan::Detached,
        };
        // Removed again (when dropped) from here on, if what follows fails.
        let mut target = Self {
            machine,
            repo: path.to_path_buf(),
            folder: path.to_path_buf(),
            branch,
            fresh: true,
            cloned: false,
            worktree_added: false,
            branch_made: None,
            branch_moved: None,
            landed: false,
        };
        let mut args = vec!["init", "-q"];
        if src.object_format != "sha1" {
            args.push("--object-format");
            args.push(&src.object_format);
        }
        crate::remote::create_dir_all(path)
            .map_err(|e| format!("could not make {}: {e}", path.display()))?;
        git::run(Some(path), &args)?;
        // Known by its real path from now on.
        target.repo = git::toplevel(path)?;
        target.folder = target.repo.clone();
        if let Some(name) = want.branch {
            check_branch_name(&target.repo, name, &target.machine)?;
        }
        for (name, url) in &src.remotes {
            git::run(Some(&target.repo), &["remote", "add", "--", name, url])?;
        }
        Ok(target)
    }

    /// The commits the other machine has (its refs' tips), which the bundle can leave out.
    pub(super) fn tips(&self) -> Result<Vec<String>, String> {
        if self.fresh {
            return Ok(Vec::new());
        }
        let out = git::run(
            Some(&self.repo),
            &["for-each-ref", "--format=%(objectname) %(*objectname)"],
        )?;
        Ok(protocol::parse_tips(&out))
    }

    /// Checks the bundle at `bundle` (a path there) and lands the work of `snap` from it; the
    /// folder the work is in.
    pub(super) fn land(
        &mut self,
        bundle: &str,
        src: &Source,
        snap: &Snapshot,
    ) -> Result<PathBuf, String> {
        self.verify(bundle)?;
        git::run_call(
            Some(&self.repo),
            &["bundle", "unbundle", bundle],
            Call {
                long: true,
                ..Call::default()
            },
        )
        .map_err(|e| format!("The bundle couldn't be unpacked on {}: {e}", self.label()))?;
        if !source::has_commit(&self.repo, &snap.worktree) {
            return Err(format!(
                "The bundle didn't hold the work on {}: nothing was changed there.",
                self.label()
            ));
        }
        if self.fresh {
            self.check_out_in_place(&src.head)?;
        } else {
            self.add_worktree(&src.head)?;
        }
        self.restore(snap)?;
        self.follow_upstream(src);
        self.landed = true;
        Ok(self.folder.clone())
    }

    fn label(&self) -> String {
        self.machine.label()
    }

    /// `bundle verify`: it is whole, and what it builds on is there.
    fn verify(&self, bundle: &str) -> Result<(), String> {
        git::run(Some(&self.repo), &["bundle", "verify", bundle])
            .map(|_| ())
            .map_err(|e| format!("The bundle didn't check out on {}: {e}", self.label()))
    }

    /// The branch where it goes and a new worktree for it, nothing checked out yet.
    fn add_worktree(&mut self, head: &str) -> Result<(), String> {
        let folder = crate::remote::git_path(&self.folder);
        let mut args = vec!["worktree", "add", "--no-checkout"];
        match &self.branch {
            BranchPlan::New(name) => {
                args.extend(["-b", name, "--", &folder, head]);
            }
            BranchPlan::Same(name) => args.extend(["--", &folder, name]),
            BranchPlan::FastForward { name, from } => {
                git::run(
                    Some(&self.repo),
                    &["update-ref", &format!("refs/heads/{name}"), head, from],
                )?;
                self.branch_moved = Some((name.clone(), from.clone()));
                args.extend(["--", &folder, name]);
            }
            BranchPlan::Detached => args.extend(["--detach", "--", &folder, head]),
        }
        let added = git::run(Some(&self.repo), &args);
        if let BranchPlan::New(name) = &self.branch
            && source::has_commit(&self.repo, &format!("refs/heads/{name}"))
        {
            self.branch_made = Some(name.clone());
        }
        // A worktree git began to make and then failed counts too: it is taken back.
        self.worktree_added = added.is_ok() || files::exists(&self.folder).unwrap_or(false);
        added.map(|_| ())
    }

    /// In a repository made for this: the branch, checked out in its own working tree.
    fn check_out_in_place(&mut self, head: &str) -> Result<(), String> {
        match &self.branch {
            BranchPlan::Detached => {
                git::run(
                    Some(&self.repo),
                    &["update-ref", "--no-deref", "HEAD", head],
                )?;
            }
            BranchPlan::New(name)
            | BranchPlan::Same(name)
            | BranchPlan::FastForward { name, .. } => {
                let full = format!("refs/heads/{name}");
                git::run(Some(&self.repo), &["update-ref", &full, head, ""])?;
                git::run(Some(&self.repo), &["symbolic-ref", "HEAD", &full])?;
            }
        }
        Ok(())
    }

    /// The working tree and the index as they were on the source.
    fn restore(&self, snap: &Snapshot) -> Result<(), String> {
        let here = Some(self.folder.as_path());
        // Files kept in Git LFS come as their pointers: `LFS: Pull` fetches them.
        let skip_lfs = Call {
            env: vec![("GIT_LFS_SKIP_SMUDGE", "1".to_owned())],
            ..Call::default()
        };
        let tree = |commit: &str| format!("{commit}^{{tree}}");
        git::run_call(
            here,
            &["read-tree", "--reset", "-u", &tree(&snap.worktree)],
            skip_lfs,
        )?;
        git::run(here, &["read-tree", &tree(&snap.index)])?;
        if !snap.intent_to_add.is_empty() {
            git::run_call(
                here,
                &["add", "-N", "--pathspec-from-file=-", "--pathspec-file-nul"],
                Call {
                    stdin: Some(protocol::nul_list(&snap.intent_to_add)),
                    ..Call::default()
                },
            )?;
        }
        // The index learns the files' times and sizes; files that differ make it answer 1.
        git::run_ok_codes(here, &["update-index", "-q", "--refresh"], &[0, 1])?;
        Ok(())
    }

    /// A new branch follows what it followed on the source, when a remote there is the same.
    fn follow_upstream(&self, src: &Source) {
        let (BranchPlan::New(name), Some((remote, merge))) = (&self.branch, &src.upstream) else {
            return;
        };
        let Some((_, url)) = src.remotes.iter().find(|(n, _)| n == remote) else {
            return;
        };
        let Ok(theirs) = git::remotes(&self.repo) else {
            return;
        };
        let key = urls::project_key(url);
        let Some(there) = theirs.iter().find(|r| urls::project_key(&r.url) == key) else {
            return;
        };
        for (field, value) in [("remote", there.name.as_str()), ("merge", merge.as_str())] {
            let _ = git::run(
                Some(&self.repo),
                &["config", &format!("branch.{name}.{field}"), value],
            );
        }
    }

    /// Takes back what a failed landing made there.
    fn undo(&mut self) {
        if self.fresh {
            let _ = files::remove_dir_all(&self.repo);
            return;
        }
        if self.worktree_added {
            let folder = crate::remote::git_path(&self.folder);
            let removed = git::run(
                Some(&self.repo),
                &["worktree", "remove", "--force", "--", &folder],
            );
            if removed.is_err() {
                let _ = files::remove_dir_all(&self.folder);
                let _ = git::run(Some(&self.repo), &["worktree", "prune"]);
            }
        }
        if let Some(name) = self.branch_made.take() {
            let _ = git::run(Some(&self.repo), &["branch", "-D", "--", &name]);
        }
        if let Some((name, from)) = self.branch_moved.take() {
            let _ = git::run(
                Some(&self.repo),
                &["update-ref", &format!("refs/heads/{name}"), &from],
            );
        }
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        if !self.landed {
            self.undo();
        }
    }
}

/// Both repositories name objects with the same hash: a bundle of one can't go in the other.
fn same_hashes(repo: &Path, src: &Source, machine: &Machine) -> Result<(), String> {
    let theirs = source::object_format(repo);
    if theirs == src.object_format {
        Ok(())
    } else {
        Err(format!(
            "The repository on {} uses {theirs} hashes and this one {}: their commits can't be \
             mixed.",
            machine.label(),
            src.object_format
        ))
    }
}

/// A name git takes for a branch (`check-ref-format --branch`), asked where it is used.
fn check_branch_name(dir: &Path, name: &str, machine: &Machine) -> Result<(), String> {
    let refused = || format!("{name} isn't a name git takes for a branch.");
    if name.starts_with('-') {
        return Err(refused());
    }
    match git::run(Some(dir), &["check-ref-format", "--branch", name]) {
        Ok(_) => Ok(()),
        Err(e) if e.contains("not a valid branch name") => Err(refused()),
        Err(e) => Err(format!("Could not ask {}: {e}", machine.label())),
    }
}

/// How `branch` can be put in `repo` for the work of `src`, or why it can't: checked out
/// there already, or there with commits the source doesn't have.
fn branch_plan(
    repo: &Path,
    src: &Source,
    branch: Option<&str>,
    machine: &Machine,
) -> Result<BranchPlan, String> {
    let Some(name) = branch else {
        return Ok(BranchPlan::Detached);
    };
    check_branch_name(repo, name, machine)?;
    let worktrees = git::worktree_list(repo, None)?;
    if let Some(wt) = worktrees.iter().find(|w| w.branch.as_deref() == Some(name)) {
        return Err(format!(
            "{name} is checked out on {} already, in {}: pick another branch name.",
            machine.label(),
            crate::remote::git_path(&wt.path)
        ));
    }
    let full = format!("refs/heads/{name}");
    let Ok(there) = git::run(Some(repo), &["rev-parse", "--verify", "-q", &full]) else {
        return Ok(BranchPlan::New(name.to_owned()));
    };
    let there = there.trim().to_owned();
    if there == src.head {
        return Ok(BranchPlan::Same(name.to_owned()));
    }
    if source::has_commit(&src.repo, &there) && source::is_ancestor(&src.repo, &there, &src.head) {
        return Ok(BranchPlan::FastForward {
            name: name.to_owned(),
            from: there,
        });
    }
    Err(format!(
        "{name} on {} has commits that aren't here: pick another branch name.",
        machine.label()
    ))
}
