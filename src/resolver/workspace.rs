//! Where a resolving agent works: a scratch worktree of the repository (out of the user's way,
//! not listed among the worktrees, removed when done; see `git::Scratch`), with the operation
//! that conflicted replayed in it so that the agent sees the whole of it, and each conflicted
//! file as it is in the user's checkout (what the user already chose there included).
//!
//! The user's checkout is not touched until the agent's work is applied, file by file, and only
//! where the file is still as it was when the agent started.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git::{self, Scratch};

/// The name the scratch worktrees made for this have (see `git::Scratch::add_empty`).
pub const SCRATCH_LABEL: &str = "resolve";

/// A conflicted file the agent is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflicted {
    /// Relative to the top of the repository, as git status names it.
    pub path: String,
    /// The file in the user's checkout when the agent started; `None` when there was none (a
    /// side deleted it).
    original: Option<Vec<u8>>,
    /// How many conflicts it had then.
    pub conflicts: usize,
    /// How long its conflict markers are (`conflict-marker-size`).
    marker: usize,
    /// The agent left a link (or anything but a file) in its place: kept as such.
    agent_not_plain: bool,
}

/// The two sides of the operation, as the agent is told them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sides {
    /// What it is: "merge", "cherry-pick", "revert", "rebase" (empty when gitcraft can't tell).
    pub operation: String,
    /// The commit checked out, and its branch if one.
    pub ours: String,
    pub ours_name: String,
    /// The commit being brought in, and what names it (a branch, a tag).
    pub theirs: Option<String>,
    pub theirs_name: Option<String>,
}

/// The agent's scratch worktree and what it was given; or, once it is done, its work kept
/// (see [`Workspace::keep`]).
pub struct Workspace {
    /// Where the agent's version of each file is.
    dir: PathBuf,
    scratch: Option<Scratch>,
    /// The folder its work is kept in, when it is.
    kept: Option<PathBuf>,
    pub files: Vec<Conflicted>,
    pub sides: Sides,
}

/// A file of a kept workspace, as `workspace.json` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct KeptFile {
    path: String,
    conflicts: usize,
    /// It was in the checkout (under `original/`).
    original: bool,
    /// "file" (under `resolved/`), "deleted" or "not_plain".
    agent: String,
    #[serde(default = "marker_size")]
    marker: usize,
}

/// How long conflict markers are unless the `conflict-marker-size` attribute says otherwise.
pub const MARKER_SIZE: usize = 7;

fn marker_size() -> usize {
    MARKER_SIZE
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct KeptWorkspace {
    files: Vec<KeptFile>,
    sides: Sides,
}

/// How a file stands after the agent's work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileResult {
    /// No conflict marker left: ready to apply.
    Resolved,
    /// The agent removed it (the side that deleted it won).
    Deleted,
    /// Conflict markers are still in it.
    StillConflicted,
    /// It had no markers to resolve (one side deleted or renamed it, or it is binary), and the
    /// agent neither changed it nor said it was done with it: the user decides.
    Untouched,
    /// The user changed it in their checkout since: it isn't applied over their work.
    ChangedMeanwhile,
    /// It, or a folder on its way, isn't a plain file or folder (a link): never applied.
    NotPlain,
}

/// A file as the agent was given it and as it left it (`None`: not there).
pub type BeforeAndAfter<'a> = (Option<&'a [u8]>, Option<Vec<u8>>);

/// What applying wrote, and what it couldn't.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub written: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl FileResult {
    pub fn applies(&self) -> bool {
        matches!(self, FileResult::Resolved | FileResult::Deleted)
    }
}

/// Whether `text` has a conflict marker `size` long (see [`MARKER_SIZE`]) at the start of a
/// line: `<<<<<<< ours`, `>>>>>>> theirs`, or the base's `|||||||`. A lone `=======` isn't
/// taken for one: plenty of text has that line of its own.
pub fn has_markers(text: &[u8], size: usize) -> bool {
    text.split(|&b| b == b'\n')
        .any(|line| b"<>|".iter().any(|&sign| is_marker(line, sign, size)))
}

/// How many conflicts `text` has, its markers `size` long.
pub fn count_conflicts(text: &[u8], size: usize) -> usize {
    text.split(|&b| b == b'\n')
        .filter(|line| is_marker(line, b'<', size))
        .count()
}

/// Whether `line` is `size` of `sign`, alone or before a space.
fn is_marker(line: &[u8], sign: u8, size: usize) -> bool {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    line.len() >= size
        && line[..size].iter().all(|&b| b == sign)
        && matches!(line.get(size), None | Some(b' '))
}

/// How long the conflict markers git writes in each of `paths` of `repo` are.
fn marker_sizes(repo: &Path, paths: &[&str]) -> Result<Vec<usize>, String> {
    let input: String = paths.iter().map(|p| format!("{p}\0")).collect();
    let out = git::run_input(
        Some(repo),
        &["check-attr", "-z", "--stdin", "conflict-marker-size"],
        &input,
    )?;
    // `<path>\0conflict-marker-size\0<value>\0` for each, in their order.
    let fields: Vec<&str> = out.split('\0').collect();
    let sizes: Vec<usize> = fields
        .as_chunks::<3>()
        .0
        .iter()
        .map(|[_, _, value]| value.parse().ok().filter(|n| *n > 0).unwrap_or(MARKER_SIZE))
        .collect();
    if sizes.len() != paths.len() {
        return Err("git didn't say how long the conflict markers are".to_owned());
    }
    Ok(sizes)
}

/// The operation `repo` is in the middle of, its commit and how to replay it in a worktree of
/// its own: `merge`, `cherry-pick`, `revert`, or a rebase's commit (picked again).
fn in_progress(repo: &Path) -> Option<(&'static str, String, Vec<&'static str>)> {
    let head = |name: &str| {
        git::run(Some(repo), &["rev-parse", "-q", "--verify", name])
            .ok()
            .and_then(|out| out.lines().next().map(|l| l.trim().to_owned()))
            .filter(|h| !h.is_empty())
    };
    if let Some(h) = head("MERGE_HEAD") {
        // Histories without a common commit were let be merged, or there would be no merge.
        let merge = vec![
            "merge",
            "--no-commit",
            "--no-ff",
            "-q",
            "--allow-unrelated-histories",
        ];
        return Some(("merge", h, merge));
    }
    if let Some(h) = head("CHERRY_PICK_HEAD") {
        return Some(("cherry-pick", h, vec!["cherry-pick", "--no-commit"]));
    }
    if let Some(h) = head("REVERT_HEAD") {
        return Some(("revert", h, vec!["revert", "--no-commit"]));
    }
    if let Some(h) = head("REBASE_HEAD") {
        return Some(("rebase", h, vec!["cherry-pick", "--no-commit"]));
    }
    None
}

/// The branch, tag or other name of `commit`, if git knows one.
fn name_of(repo: &Path, commit: &str) -> Option<String> {
    git::run(
        Some(repo),
        &["name-rev", "--name-only", "--no-undefined", commit],
    )
    .ok()
    .map(|n| n.trim().to_owned())
    .filter(|n| !n.is_empty())
}

fn sides(repo: &Path) -> (Sides, Option<Vec<String>>) {
    let ours = git::run(Some(repo), &["rev-parse", "HEAD"])
        .map(|h| h.trim().to_owned())
        .unwrap_or_default();
    let ours_name = git::run(Some(repo), &["symbolic-ref", "-q", "--short", "HEAD"])
        .map(|b| b.trim().to_owned())
        .unwrap_or_else(|_| "detached HEAD".to_owned());
    let Some((operation, theirs, replay)) = in_progress(repo) else {
        let sides = Sides {
            ours,
            ours_name,
            ..Sides::default()
        };
        return (sides, None);
    };
    let mut replay: Vec<String> = replay.iter().map(|w| (*w).to_owned()).collect();
    if let Some(parent) = (operation != "merge")
        .then(|| mainline(repo, &theirs))
        .flatten()
    {
        replay.extend(["-m".to_owned(), parent]);
    }
    replay.push(theirs.clone());
    let sides = Sides {
        operation: operation.to_owned(),
        theirs_name: name_of(repo, &theirs),
        theirs: Some(theirs),
        ours,
        ours_name,
    };
    (sides, Some(replay))
}

/// Whether `path` of the checkout at `top` is reached without a link: none of its folders, nor
/// it, is one (an agent could link a file of the checkout to a secret elsewhere, to have it
/// applied and committed). `Ok(false)` when it, or a folder on its way, isn't there.
fn plain(top: &Path, path: &str) -> Result<bool, String> {
    let mut at = top.to_path_buf();
    // The parts the system goes through to open it: on Unix a `\` is part of a name.
    for part in Path::new(path).components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("{path} goes through a link"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("couldn't look at {path}: {e}")),
        }
    }
    Ok(true)
}

/// Reads `path` of the checkout at `top`: `None` when there is no such file. One reached
/// through a link, or that isn't a file, is an error.
fn read(top: &Path, path: &str) -> Result<Option<Vec<u8>>, String> {
    if !plain(top, path)? {
        return Ok(None);
    }
    // Read so that what was looked at is what is read: a link swapped in on the way since
    // (by something the agent left running) isn't followed.
    crate::safe_read::read_inside(top, path, u64::MAX - 1)
        .map(Some)
        .ok_or_else(|| format!("{path} isn't a plain file, or couldn't be read"))
}

/// Writes `bytes` to `path` of the checkout at `top` (removes it for `None`), never through a
/// link.
fn write(top: &Path, path: &str, bytes: Option<&[u8]>) -> Result<(), String> {
    plain(top, path)?;
    let file = top.join(path);
    match bytes {
        Some(bytes) => {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("couldn't make the folder of {path}: {e}"))?;
            }
            replace(&file, bytes).map_err(|e| format!("couldn't write {path}: {e}"))
        }
        None => match std::fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("couldn't remove {path}: {e}")),
        },
    }
}

/// Writes `bytes` to `file` whole or not at all: into a file beside it first, then put in its
/// place, with its permissions (an executable stays one). One that fails leaves it as it was.
fn replace(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = file.with_file_name(format!(".gitcraft-{}-{n}", std::process::id()));
    replace_with_temp(file, &temp, bytes)
}

fn replace_with_temp(file: &Path, temp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    // Only an opened file belongs to us. A create_new collision must remain untouched.
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)?;
    let written = (|| {
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        // Only Unix has more to them than read-only (which would keep it from being replaced).
        #[cfg(unix)]
        if let Ok(meta) = std::fs::symlink_metadata(file) {
            std::fs::set_permissions(temp, meta.permissions())?;
        }
        std::fs::rename(temp, file)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    written
}

/// A path git status gave, checked to stay inside the checkout.
fn checked(path: &str) -> Result<&str, String> {
    crate::mcp::args::repo_path(path)
}

/// git in the agent's scratch worktree, without the repository's hooks nor its recorded
/// resolutions: that copy is gitcraft's own, and nothing the user set up to run on a checkout
/// or a merge is for it.
fn hookless(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec![
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "rerere.enabled=false",
    ];
    all.extend_from_slice(args);
    git::run(Some(dir), &all)
}

/// The files the index of the worktree at `dir` has in conflict.
fn unmerged(dir: &Path) -> Result<Vec<String>, String> {
    let out = git::run(Some(dir), &["diff", "--name-only", "-z", "--diff-filter=U"])?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Replays the operation in progress (`replay`, see [`sides`]) in the scratch worktree at `dir`.
/// It conflicts there too (that is why it is replayed): what matters is that the rest of it is
/// in place. One git refused outright is an error: the agent would be told of an operation that
/// isn't there.
fn replay(dir: &Path, replay: &[String]) -> Result<(), String> {
    let args: Vec<&str> = replay.iter().map(String::as_str).collect();
    match hookless(dir, &args) {
        Ok(_) => Ok(()),
        Err(_) if !unmerged(dir)?.is_empty() => Ok(()),
        Err(e) => Err(format!(
            "The operation couldn't be replayed in the agent's worktree: {e}"
        )),
    }
}

/// The files the replay left in conflict in the scratch worktree at `dir` that the agent isn't
/// given: put there as they are in the checkout at `repo`, the user's resolution included, and
/// marked resolved there when they are in the checkout. The agent sees what the user decided.
fn take_resolved(repo: &Path, dir: &Path, given: &[Conflicted]) -> Result<(), String> {
    let still = unmerged(repo)?;
    let mut resolved = Vec::new();
    for path in unmerged(dir)? {
        if given.iter().any(|f| f.path == path) {
            continue;
        }
        // One that isn't a plain file in the checkout stays as the replay left it.
        let Some(bytes) = checked(&path).ok().and_then(|p| read(repo, p).ok()) else {
            continue;
        };
        write(dir, &path, bytes.as_deref())?;
        if !still.contains(&path) {
            resolved.push(path);
        }
    }
    if resolved.is_empty() {
        return Ok(());
    }
    let mut add = vec!["--literal-pathspecs", "add", "-A", "--"];
    add.extend(resolved.iter().map(String::as_str));
    hookless(dir, &add).map(|_| ())
}

/// The parent a cherry-pick or revert of `commit` of `repo` goes by (`-m`), when it is a merge:
/// the one git kept for the sequence in progress, else the first (as nearly always).
fn mainline(repo: &Path, commit: &str) -> Option<String> {
    let parents = git::run(Some(repo), &["rev-list", "--parents", "-n", "1", commit]).ok()?;
    if parents.split_whitespace().count() < 3 {
        return None;
    }
    let kept = git::run(Some(repo), &["rev-parse", "--git-path", "sequencer/opts"])
        .ok()
        .and_then(|opts| {
            git::run(
                Some(repo),
                &["config", "-f", opts.trim(), "--get", "options.mainline"],
            )
            .ok()
        })
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()));
    Some(kept.unwrap_or_else(|| "1".to_owned()))
}

impl Workspace {
    /// Makes the scratch worktree for resolving `files` of the checkout at `repo` (on this
    /// machine), replays the operation in progress there, and puts each file in it as it is
    /// in the checkout.
    pub fn prepare(repo: &Path, files: &[String]) -> Result<Workspace, String> {
        if crate::remote::is_remote(repo) {
            return Err(
                "Resolving in the background works for repositories on this machine \
                        only, for now"
                    .to_owned(),
            );
        }
        if files.is_empty() {
            return Err("There is no conflicted file to resolve".to_owned());
        }
        let paths = files
            .iter()
            .map(|p| checked(p))
            .collect::<Result<Vec<&str>, String>>()?;
        let sizes = marker_sizes(repo, &paths)?;
        let mut given = Vec::with_capacity(files.len());
        for (path, marker) in paths.into_iter().zip(sizes) {
            let original = read(repo, path)?;
            let conflicts = original
                .as_deref()
                .map_or(0, |text| count_conflicts(text, marker));
            given.push(Conflicted {
                path: path.to_owned(),
                original,
                conflicts,
                marker,
                agent_not_plain: false,
            });
        }
        let (sides, replay_args) = sides(repo);
        let scratch = Scratch::add_empty(repo, SCRATCH_LABEL, "HEAD")?;
        hookless(
            &scratch.path,
            &["checkout", "--detach", "--force", "-q", "HEAD"],
        )
        .map_err(|e| format!("couldn't check out HEAD in the agent's worktree: {e}"))?;
        if let Some(args) = replay_args {
            replay(&scratch.path, &args)?;
            take_resolved(repo, &scratch.path, &given)?;
        }
        for file in &given {
            write(&scratch.path, &file.path, file.original.as_deref())?;
        }
        Ok(Workspace {
            dir: scratch.path.clone(),
            scratch: Some(scratch),
            kept: None,
            files: given,
            sides,
        })
    }

    /// Whether this is the agent's work kept (see [`Workspace::keep`]), not its scratch
    /// worktree.
    pub fn is_kept(&self) -> bool {
        self.kept.is_some()
    }

    /// The folder the agent works in (or its work is kept in).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How each file stands: the agent's version, against the checkout at `repo` now when one
    /// is given (`None`: while the operation is put aside, the checkout has none of it).
    /// `done`: the files (or steps named after them) the agent reported done.
    pub fn check(&self, repo: Option<&Path>, done: &[String]) -> Vec<(String, FileResult)> {
        self.files
            .iter()
            .map(|file| (file.path.clone(), self.check_one(repo, file, done)))
            .collect()
    }

    fn check_one(&self, repo: Option<&Path>, file: &Conflicted, done: &[String]) -> FileResult {
        if let Some(repo) = repo {
            let Ok(now) = read(repo, &file.path) else {
                return FileResult::NotPlain;
            };
            if now != file.original {
                return FileResult::ChangedMeanwhile;
            }
        }
        if file.agent_not_plain {
            return FileResult::NotPlain;
        }
        let Ok(theirs) = read(self.dir(), &file.path) else {
            return FileResult::NotPlain;
        };
        // A file with markers is resolved once they are gone; one without is only when the
        // agent changed it or said it is done with it, and put none in it.
        let decided = theirs != file.original || done.contains(&file.path);
        let had = (file.original.as_deref()).is_some_and(|o| has_markers(o, file.marker));
        match theirs {
            Some(text) if (file.conflicts > 0 || !had) && has_markers(&text, file.marker) => {
                FileResult::StillConflicted
            }
            _ if file.conflicts == 0 && !decided => FileResult::Untouched,
            Some(_) => FileResult::Resolved,
            None => FileResult::Deleted,
        }
    }

    /// Writes the agent's version of each of `paths` that is ready into the checkout at `repo`,
    /// each on its own: one that fails doesn't stop the others. One the user changed
    /// meanwhile, or that isn't resolved, is left as it is.
    pub fn apply(&self, repo: &Path, paths: &[String], done: &[String]) -> Applied {
        let mut applied = Applied::default();
        for file in self.files.iter().filter(|f| paths.contains(&f.path)) {
            // Each looked at just before it is written, not all of them first: an edit the user
            // saved meanwhile is kept.
            if !self.check_one(Some(repo), file, done).applies() {
                continue;
            }
            let path = &file.path;
            let copied =
                read(self.dir(), path).and_then(|theirs| write(repo, path, theirs.as_deref()));
            match copied {
                Ok(()) => applied.written.push(path.clone()),
                Err(e) => applied.failed.push((path.clone(), e)),
            }
        }
        applied
    }

    /// `path` as the agent was given it, and as it left it, for showing what it did: `None`
    /// for one that isn't there; an error for one that isn't a plain file.
    pub fn before_and_after(&self, path: &str) -> Result<BeforeAndAfter<'_>, String> {
        let file = self
            .files
            .iter()
            .find(|f| f.path == path)
            .ok_or_else(|| format!("{path} wasn't given to the agent"))?;
        Ok((file.original.as_deref(), read(self.dir(), path)?))
    }

    /// Keeps the agent's work in the folder `to` (of the repository's git folder: it outlasts
    /// gitcraft, unlike the scratch worktree): each file as it was given (`original/`) and as
    /// the agent left it (`resolved/`), and what is known of them (`workspace.json`). The
    /// workspace of what is kept.
    pub fn keep(&self, to: &Path) -> Result<Workspace, String> {
        let (originals, resolved) = (to.join("original"), to.join("resolved"));
        std::fs::create_dir_all(&resolved)
            .map_err(|e| format!("couldn't make {}: {e}", resolved.display()))?;
        let mut listed = Vec::new();
        let mut files = Vec::new();
        for file in &self.files {
            if let Some(bytes) = &file.original {
                write(&originals, &file.path, Some(bytes))?;
            }
            let agent = match read(self.dir(), &file.path) {
                _ if file.agent_not_plain => "not_plain",
                Ok(Some(bytes)) => {
                    write(&resolved, &file.path, Some(&bytes))?;
                    "file"
                }
                Ok(None) => "deleted",
                Err(_) => "not_plain",
            };
            // Nothing of an earlier keep is left to be taken for the agent's file.
            if agent != "file" {
                write(&resolved, &file.path, None)?;
            }
            listed.push(KeptFile {
                path: file.path.clone(),
                conflicts: file.conflicts,
                original: file.original.is_some(),
                agent: agent.to_owned(),
                marker: file.marker,
            });
            files.push(Conflicted {
                agent_not_plain: agent == "not_plain",
                ..file.clone()
            });
        }
        let kept = KeptWorkspace {
            files: listed,
            sides: self.sides.clone(),
        };
        let text = serde_json::to_string_pretty(&kept).map_err(|e| e.to_string())?;
        std::fs::write(to.join("workspace.json"), text)
            .map_err(|e| format!("couldn't keep the agent's work: {e}"))?;
        Ok(Workspace {
            dir: resolved,
            scratch: None,
            kept: Some(to.to_path_buf()),
            files,
            sides: self.sides.clone(),
        })
    }

    /// The agent's work kept in `from` by [`Workspace::keep`].
    pub fn load(from: &Path) -> Result<Workspace, String> {
        let text = std::fs::read_to_string(from.join("workspace.json"))
            .map_err(|e| format!("the agent's work isn't there: {e}"))?;
        let kept: KeptWorkspace = serde_json::from_str(&text)
            .map_err(|e| format!("the agent's work can't be read: {e}"))?;
        let mut files = Vec::with_capacity(kept.files.len());
        for f in kept.files {
            let path = checked(&f.path)?.to_owned();
            // What is listed must be there, and only that: a file missing is never taken for
            // one the agent deleted, nor one the user had none of.
            let damaged = || format!("the agent's work on {path} isn't all there");
            let original = if f.original {
                Some(read(&from.join("original"), &path)?.ok_or_else(damaged)?)
            } else {
                None
            };
            let left = plain(&from.join("resolved"), &path)?;
            match (f.agent.as_str(), left) {
                ("file", true) | ("deleted" | "not_plain", false) => {}
                _ => return Err(damaged()),
            }
            files.push(Conflicted {
                path,
                original,
                conflicts: f.conflicts,
                marker: f.marker,
                agent_not_plain: f.agent == "not_plain",
            });
        }
        Ok(Workspace {
            dir: from.join("resolved"),
            scratch: None,
            kept: Some(from.to_path_buf()),
            files,
            sides: kept.sides,
        })
    }

    /// Removes the scratch worktree (or the work kept) now, saying why if it couldn't be.
    pub fn remove(self) -> Result<(), String> {
        match (self.scratch, self.kept) {
            (Some(scratch), _) => scratch.remove(),
            (None, Some(kept)) => match std::fs::remove_dir_all(&kept) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    Err(format!("couldn't remove {}: {e}", kept.display()))
                }
                _ => Ok(()),
            },
            (None, None) => Ok(()),
        }
    }

    /// Removes it, trying again for a while: on Windows a program that just ended may still
    /// hold its folder for a moment.
    pub fn remove_patiently(mut self) -> Result<(), String> {
        match self.scratch.take() {
            Some(scratch) => scratch.remove_retrying(5, std::time::Duration::from_millis(400)),
            None => self.remove(),
        }
    }
}

#[cfg(test)]
mod tests;
