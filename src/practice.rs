//! Real learning repositories, always made in a new local temporary folder. No setup or goal
//! check writes to an existing repository. Successful sandboxes are left for the OS to clean.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::git;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Scenario {
    Conflict,
    Diverged,
    Detached,
    Published,
}

impl Scenario {
    pub const ALL: [Self; 4] = [
        Self::Conflict,
        Self::Diverged,
        Self::Detached,
        Self::Published,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Conflict => "Resolve a merge conflict",
            Self::Diverged => "Merge diverged branches",
            Self::Detached => "Rescue a detached HEAD",
            Self::Published => "Undo a published commit",
        }
    }

    pub fn goal(self) -> &'static str {
        match self {
            Self::Conflict => {
                "Resolve greeting.txt, stage it and finish the merge on main. Keep both branches in the history and leave no conflict markers or uncommitted changes."
            }
            Self::Diverged => {
                "Merge feature into main, keeping the commits from both branches. Finish on main with no uncommitted changes."
            }
            Self::Detached => {
                "Create and check out a new branch at the detached commit, so the rescue note stays reachable. Leave no uncommitted changes."
            }
            Self::Published => {
                "Revert the published mistake on main, then push the new commit to origin. Keep the published history and restore the original files. Origin is a local practice repository; no server is contacted."
            }
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Conflict => {
                "Open greeting.txt under Conflicts. Choose or edit the greeting, save and stage it, then commit with the prepared merge message in the Changes pane."
            }
            Self::Diverged => {
                "While main is checked out, right-click feature and choose Merge into main."
            }
            Self::Detached => {
                "Click Branch in the toolbar, give it a name and keep Check it out selected."
            }
            Self::Published => {
                "Right-click the commit named A published mistake, choose Revert commit, then Push. Reset would rewrite the history people already have."
            }
        }
    }
}

/// The fixture's original commits, kept outside its working tree so solving it stays clean.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Session {
    version: u8,
    pub scenario: Scenario,
    before: String,
    target: String,
    original_tree: String,
}

const MARKER: &str = ".git/gitcraft-practice.json";

/// Setup and checks deliberately ignore profiles and inherited git directory/config
/// overrides: a hook's environment or a signing default must not touch the caller's repo.
fn run(repo: &Path, args: &[&str]) -> Result<String, String> {
    git::practice_run(repo, args)
}

pub(crate) fn create(scenario: Scenario) -> Result<PathBuf, String> {
    create_in(&std::env::temp_dir(), scenario)
}

fn create_in(parent: &Path, scenario: Scenario) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    // create_dir is exclusive: a collision never reuses or removes somebody else's folder.
    let root = parent.join(format!(
        "gitcraft-practice-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).map_err(|e| format!("Could not make a practice folder: {e}"))?;
    let result = setup(&root, scenario);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    result
}

fn setup(root: &Path, scenario: Scenario) -> Result<PathBuf, String> {
    let repo = root.join("repository");
    std::fs::create_dir(&repo).map_err(|e| e.to_string())?;
    run(&repo, &["init", "--template=", "-b", "main"])?;
    let hooks = repo.join(".git/disabled-hooks");
    std::fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    for (key, value) in [
        ("user.name", "Gitcraft Practice"),
        ("user.email", "practice@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.hooksPath", &hooks.to_string_lossy()),
        ("core.attributesFile", ""),
        ("core.autocrlf", "false"),
        ("core.fsmonitor", "false"),
        ("protocol.file.allow", "always"),
        ("merge.conflictStyle", "diff3"),
    ] {
        run(&repo, &["config", "--local", key, value])?;
    }
    write(&repo, "greeting.txt", "Hello, world!\n")?;
    commit(&repo, "The original greeting")?;
    let original_tree = oid(&repo, "HEAD^{tree}")?;
    let (before, target) = match scenario {
        Scenario::Conflict | Scenario::Diverged => {
            run(&repo, &["checkout", "-b", "feature"])?;
            if scenario == Scenario::Conflict {
                write(&repo, "greeting.txt", "Hello from feature!\n")?;
            } else {
                write(&repo, "feature.txt", "A feature worth keeping.\n")?;
            }
            commit(&repo, "A greeting from feature")?;
            let target = oid(&repo, "HEAD")?;
            run(&repo, &["checkout", "main"])?;
            if scenario == Scenario::Conflict {
                write(&repo, "greeting.txt", "Hello from main!\n")?;
            } else {
                write(&repo, "main.txt", "Main has moved on too.\n")?;
            }
            commit(&repo, "Main moves on")?;
            let before = oid(&repo, "HEAD")?;
            if scenario == Scenario::Conflict {
                // This merge is expected to stop, but any other failure is a setup error.
                let merged = run(&repo, &["merge", "--no-edit", "feature"]);
                if merged.is_ok() || !run(&repo, &["ls-files", "-u"])?.contains("greeting.txt") {
                    return Err("Could not prepare the practice merge conflict".to_owned());
                }
            }
            (before, target)
        }
        Scenario::Detached => {
            let before = oid(&repo, "HEAD")?;
            run(&repo, &["checkout", "--detach"])?;
            write(
                &repo,
                "rescue.txt",
                "Save this detached commit on a new branch.\n",
            )?;
            commit(&repo, "A rescue note without a branch")?;
            (before, oid(&repo, "HEAD")?)
        }
        Scenario::Published => {
            let before = oid(&repo, "HEAD")?;
            write(
                &repo,
                "greeting.txt",
                "Oops, this greeting was a mistake!\n",
            )?;
            commit(&repo, "A published mistake")?;
            let target = oid(&repo, "HEAD")?;
            let origin = root.join("origin.git");
            std::fs::create_dir(&origin).map_err(|e| e.to_string())?;
            run(&origin, &["init", "--bare", "--template=", "-b", "main"])?;
            run(
                &origin,
                &[
                    "config",
                    "--local",
                    "core.hooksPath",
                    &hooks.to_string_lossy(),
                ],
            )?;
            run(
                &repo,
                &["remote", "add", "origin", &origin.to_string_lossy()],
            )?;
            run(
                &repo,
                &[
                    "-c",
                    "protocol.file.allow=always",
                    "push",
                    "-u",
                    "origin",
                    "main",
                ],
            )?;
            (before, target)
        }
    };
    let session = Session {
        version: 1,
        scenario,
        before,
        target,
        original_tree,
    };
    let json = serde_json::to_vec(&session).map_err(|e| e.to_string())?;
    std::fs::write(repo.join(MARKER), json).map_err(|e| e.to_string())?;
    Ok(repo)
}

fn write(repo: &Path, path: &str, text: &str) -> Result<(), String> {
    std::fs::write(repo.join(path), text).map_err(|e| e.to_string())
}

fn commit(repo: &Path, message: &str) -> Result<(), String> {
    run(repo, &["add", "--all"])?;
    run(repo, &["commit", "-m", message]).map(|_| ())
}

fn oid(repo: &Path, rev: &str) -> Result<String, String> {
    run(repo, &["rev-parse", "--verify", rev]).map(|s| s.trim().to_owned())
}

/// A marker alone grants no write capability. It is only recognized in our local sandbox
/// layout, bounded and without following any marker/.git symlinks.
pub(crate) fn load(repo: &Path) -> Option<Session> {
    if crate::remote::is_remote(repo)
        || repo.file_name()? != "repository"
        || !repo
            .parent()?
            .file_name()?
            .to_str()?
            .starts_with("gitcraft-practice-")
    {
        return None;
    }
    let bytes = crate::safe_read::read_inside(repo, MARKER, 8192)?;
    let session: Session = serde_json::from_slice(&bytes).ok()?;
    let valid_oid =
        |s: &str| matches!(s.len(), 40 | 64) && s.bytes().all(|c| c.is_ascii_hexdigit());
    (session.version == 1
        && [&session.before, &session.target, &session.original_tree]
            .into_iter()
            .all(|s| valid_oid(s)))
    .then_some(session)
}

impl Session {
    /// Read-only, no network, and all errors stay errors rather than pretending success.
    pub fn check(&self, repo: &Path) -> Result<bool, String> {
        if load(repo).as_ref() != Some(self) {
            return Err("This practice repository no longer matches its scenario. Start a fresh one from Practice.".to_owned());
        }
        if !run(repo, &["status", "--porcelain"])?.trim().is_empty()
            || [
                "MERGE_HEAD",
                "CHERRY_PICK_HEAD",
                "REVERT_HEAD",
                "rebase-merge",
                "rebase-apply",
            ]
            .iter()
            .any(|name| repo.join(".git").join(name).exists())
        {
            return Ok(false);
        }
        let current =
            run(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default();
        let current = current.trim();
        let head = oid(repo, "HEAD")?;
        let ancestor = |older: &str| -> Result<bool, String> {
            Ok(run(repo, &["merge-base", older, &head])?.trim() == older)
        };
        match self.scenario {
            Scenario::Conflict | Scenario::Diverged => {
                if current != "main" || !ancestor(&self.before)? || !ancestor(&self.target)? {
                    return Ok(false);
                }
                if self.scenario == Scenario::Conflict {
                    let greeting = run(repo, &["show", "HEAD:greeting.txt"])?;
                    if greeting.lines().any(|l| {
                        l.starts_with("<<<<<<<")
                            || l.starts_with("|||||||")
                            || l.starts_with("=======")
                            || l.starts_with(">>>>>>>")
                    }) {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Scenario::Detached => {
                Ok(!current.is_empty() && current != "main" && ancestor(&self.target)?)
            }
            Scenario::Published => {
                if current != "main"
                    || head == self.target
                    || !ancestor(&self.target)?
                    || oid(repo, "HEAD^{tree}")? != self.original_tree
                {
                    return Ok(false);
                }
                let parent = repo.parent().ok_or("Practice folder is missing")?;
                Ok(oid(&parent.join("origin.git"), "refs/heads/main")? == head)
            }
        }
    }
}

#[cfg(test)]
mod tests;
