//! Teleport: the work of a tab (its branch and commits, the staged and unstaged changes, the
//! untracked files) carried to another machine gitcraft reaches (this one, a WSL distribution,
//! an SSH host), where it arrives in a new worktree. No push, forge or cloud is involved.
//!
//! - [`source`]: the snapshot, taken through a temporary index so nothing of the source is
//!   touched; a `git bundle` of it and of the commits the other machine lacks.
//! - [`files`]: the bundle written there in pieces, through the shells kept open to it (the
//!   same `base64 -d` every command's input goes through), its size checked once it is there.
//! - [`land`]: the repository there (found by its remotes, cloned, or made from the bundle),
//!   checked before anything is sent, and the work landed in it.
//!
//! Everything runs on a thread of its own ([`run`]), never the window's; what it made (the
//! temporary ref and index here, the bundle and a half-made worktree there) is removed again,
//! also when it fails.

pub(crate) mod display;
mod files;
mod land;
mod protocol;
mod source;
mod urls;

#[cfg(test)]
mod tests;

/// For the tests that teleport (here and the window's): one at a time, as they look for
/// leftovers in temporary folders all of them use.
#[cfg(test)]
pub(crate) fn test_serial() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};

pub use land::Destination;
pub(crate) use protocol::LARGE;
pub use source::{Source, inspect};
pub(crate) use urls::{find_clones, shown_url};

use crate::remote::{self, Host};

/// A machine work can go to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Machine {
    /// The one this app runs on.
    Here,
    There(Host),
}

impl Machine {
    /// The machine `path` (a folder as this app names it) is on.
    pub fn of(path: &Path) -> Self {
        remote::parse(path).map_or(Self::Here, |t| Self::There(t.host))
    }

    /// "This machine", "WSL · Ubuntu", "SSH · build".
    pub fn label(&self) -> String {
        match self {
            Self::Here => "this machine".to_owned(),
            Self::There(host) => host.label(),
        }
    }

    /// The folder `there` (a path on this machine) as this app names it.
    pub fn path(&self, there: &str) -> PathBuf {
        match self {
            Self::Here => PathBuf::from(there),
            Self::There(host) => remote::encode(host, there),
        }
    }
}

/// The home folder of `machine`, as a path there: asks it, for one elsewhere.
pub fn home_of(machine: &Machine) -> Result<String, String> {
    match machine {
        Machine::Here => std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| PathBuf::from(h).display().to_string())
            .ok_or_else(|| "could not find the home folder".to_owned()),
        Machine::There(host) => files::home(host),
    }
}

/// The machines the work of `source` can go to, each once, in this order: this one, the WSL
/// distributions, the SSH hosts (`~/.ssh/config`'s, then those of `known` folders); never its
/// own.
pub fn machines(
    source: &Path,
    distros: &[String],
    ssh_hosts: &[String],
    known: &[PathBuf],
) -> Vec<Machine> {
    let own = Machine::of(source);
    let mut list = vec![Machine::Here];
    list.extend(
        distros
            .iter()
            .filter(|d| remote::valid_distro(d))
            .map(|d| Machine::There(Host::Wsl(d.clone()))),
    );
    list.extend(
        ssh_hosts
            .iter()
            .filter(|h| remote::valid_ssh_host(h))
            .map(|h| Machine::There(Host::Ssh(h.clone()))),
    );
    list.extend(known.iter().map(|p| Machine::of(p)));
    let mut seen = Vec::new();
    list.retain(|m| {
        let new = *m != own && !seen.contains(m);
        seen.push(m.clone());
        new
    });
    list
}

/// The folder a new worktree for `branch` (else the commit `head`) goes in beside the
/// repository `repo`: `app` + `feature/x` → `app-feature-x`.
pub fn worktree_folder(repo: &Path, branch: Option<&str>, head: &str) -> PathBuf {
    let suffix = match branch {
        Some(b) => b.replace(['/', '\\', ' ', ':'], "-"),
        None => head.chars().take(8).collect(),
    };
    let machine = Machine::of(repo);
    let there = remote::git_path(repo);
    let there = there.trim_end_matches(['/', '\\']);
    let parent = match &machine {
        Machine::There(_) => remote::parent_there(there).to_owned(),
        Machine::Here => Path::new(there)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let name = Path::new(there).file_name().map_or_else(
        || "worktree".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    match &machine {
        Machine::There(_) => {
            machine.path(&format!("{}/{name}-{suffix}", parent.trim_end_matches('/')))
        }
        Machine::Here => Path::new(&parent).join(format!("{name}-{suffix}")),
    }
}

/// What to teleport, and where.
#[derive(Clone, Debug)]
pub struct Plan {
    /// The tab's repository.
    pub source: PathBuf,
    pub destination: Destination,
    /// The branch the work is on there; `None`: detached.
    pub branch: Option<String>,
    /// The new worktree's folder (for [`Destination::Create`], the repository's own).
    pub folder: PathBuf,
    /// A bundle larger than this waits for [`Job::answer`] before it is sent.
    pub confirm_above: u64,
}

/// How a teleport ended well: where the work is, and what is worth saying about it.
#[derive(Clone, Debug)]
pub struct Landed {
    pub folder: PathBuf,
    /// The bundle's size.
    pub size: u64,
    pub notes: Vec<String>,
}

/// Where a teleport is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Checking,
    /// Making the clone the work goes in.
    Cloning,
    Snapshot,
    Bundling,
    /// The bundle is this large: waiting for the user to say whether to send it.
    Confirm(u64),
    Sending {
        sent: u64,
        total: u64,
    },
    Landing,
    CleaningUp,
}

impl Step {
    /// What the dialog says meanwhile.
    pub fn label(self) -> String {
        match self {
            Step::Checking => "Checking both machines…".to_owned(),
            Step::Cloning => "Cloning the project there…".to_owned(),
            Step::Snapshot => "Taking a snapshot of the work…".to_owned(),
            Step::Bundling => "Bundling the commits…".to_owned(),
            Step::Confirm(size) => format!("The bundle is {}.", protocol::size_label(size)),
            Step::Sending { sent, total } => format!(
                "Sending {} of {}…",
                protocol::size_label(sent),
                protocol::size_label(total)
            ),
            Step::Landing => "Unpacking and checking out…".to_owned(),
            Step::CleaningUp => "Cleaning up…".to_owned(),
        }
    }

    /// Whether cancelling still stops it: once the bundle is being unpacked there, it runs
    /// to its end (or its failure, which takes back what it made).
    pub fn stoppable(self) -> bool {
        !matches!(self, Step::Landing | Step::CleaningUp)
    }
}

/// A running teleport as the window and its thread share it: where it is, whether it is to
/// stop, the user's answer to a large bundle.
pub struct Job {
    step: Mutex<Step>,
    cancel: AtomicBool,
    answer: Mutex<Option<bool>>,
    answered: Condvar,
    repaint: Box<dyn Fn() + Send + Sync>,
}

impl Job {
    /// `repaint`: asks the window to draw again (a step changed).
    pub fn new(repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            step: Mutex::new(Step::Checking),
            cancel: AtomicBool::new(false),
            answer: Mutex::new(None),
            answered: Condvar::new(),
            repaint: Box::new(repaint),
        }
    }

    pub fn step(&self) -> Step {
        *self.step.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stops it at the next step (or piece of the bundle); what it made is taken back.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.answer(false);
    }

    /// Whether it was asked to stop.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Whether to send a large bundle.
    pub fn answer(&self, send: bool) {
        *self.answer.lock().unwrap_or_else(PoisonError::into_inner) = Some(send);
        self.answered.notify_all();
    }

    fn set(&self, step: Step) {
        *self.step.lock().unwrap_or_else(PoisonError::into_inner) = step;
        (self.repaint)();
    }

    fn go_on(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(CANCELLED.to_owned())
        } else {
            Ok(())
        }
    }

    /// Waits for the user to say whether to send a bundle of `size` bytes.
    fn confirm(&self, size: u64) -> Result<(), String> {
        self.set(Step::Confirm(size));
        let mut answer = self.answer.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            match *answer {
                Some(true) => return self.go_on(),
                Some(false) => return Err(CANCELLED.to_owned()),
                None => {
                    answer = self
                        .answered
                        .wait(answer)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
    }
}

/// The error of a teleport the user stopped.
pub const CANCELLED: &str = "Teleport cancelled: nothing was changed.";

/// A word naming what one teleport makes (its ref, its index, its bundle): new every time.
fn new_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:x}-{:x}-{:x}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// How a teleport ended without landing.
#[derive(Clone, Debug)]
pub struct Failed {
    /// What the user is told: why, and what was left.
    pub message: String,
    /// The user stopped it.
    pub cancelled: bool,
}

impl From<Failed> for String {
    fn from(failed: Failed) -> Self {
        failed.message
    }
}

/// Carries out `plan`, saying in `job` where it is. Never on the window's thread: it waits for
/// both machines and for the user. What it made is taken back if it fails, a clone too.
pub fn run(plan: &Plan, job: &Job) -> Result<Landed, Failed> {
    attempt(plan, job).map_err(|message| Failed {
        cancelled: message == CANCELLED,
        message,
    })
}

fn attempt(plan: &Plan, job: &Job) -> Result<Landed, String> {
    job.set(Step::Checking);
    let src = source::inspect(&plan.source)?;
    job.go_on()?;
    let destination = match &plan.destination {
        Destination::Repo(path) => Destination::Repo(path.clone()),
        Destination::Clone { url, path } => Destination::Clone {
            url: url.clone(),
            path: files::expand_home(path)?,
        },
        Destination::Create(path) => Destination::Create(files::expand_home(path)?),
    };
    let folder = match &destination {
        Destination::Create(path) => path.clone(),
        _ => files::expand_home(&plan.folder)?,
    };
    if Machine::of(&folder) != Machine::of(destination_path(&destination)) {
        return Err("The new worktree has to be on the machine of its repository.".to_owned());
    }
    let want = land::Wanted {
        branch: plan.branch.as_deref(),
        folder: &folder,
    };
    if matches!(destination, Destination::Clone { .. }) {
        job.set(Step::Cloning);
    }
    // Taken back when dropped, unless it landed.
    let mut target = land::Target::prepare(&destination, &src, &want)?;
    let landed = carry(plan, job, &src, &mut target);
    job.set(Step::CleaningUp);
    landed
}

/// The folder a destination is about.
fn destination_path(dest: &Destination) -> &Path {
    match dest {
        Destination::Repo(path) | Destination::Create(path) => path,
        Destination::Clone { path, .. } => path,
    }
}

/// Snapshot, bundle, send, land: everything after the other machine's repository is ready.
fn carry(
    plan: &Plan,
    job: &Job,
    src: &Source,
    target: &mut land::Target,
) -> Result<Landed, String> {
    let mut notes = Vec::new();
    job.go_on()?;
    let tips = target.tips()?;
    job.set(Step::Snapshot);
    let id = new_id();
    let mut snap = source::Snapshot::take(src, &id)?;
    job.go_on()?;
    job.set(Step::Bundling);
    let bases = snap.present(&tips)?;
    let bundle = snap.bundle(&bases)?;
    // Nothing of the source is needed any more: its ref and index go before anything is sent.
    if let Err(e) = snap.clean_up() {
        notes.push(format!(
            "Some of the snapshot stayed on this repository's machine: {e}"
        ));
    }
    let size = bundle.len() as u64;
    if size > plan.confirm_above {
        job.confirm(size)?;
    }
    let upload = send(&target.machine, &id, &bundle, job)?;
    job.set(Step::Landing);
    let folder = target.land(upload.there(), src, &snap)?;
    if let Err(e) = upload.remove() {
        notes.push(format!(
            "The bundle stayed on {}: {e}",
            target.machine.label()
        ));
    }
    if snap.lfs {
        notes.push(
            "Files kept in Git LFS arrived as their pointers; the palette's LFS: Pull fetches \
             those that were pushed."
                .to_owned(),
        );
    }
    Ok(Landed {
        folder,
        size,
        notes,
    })
}

/// Writes `bundle` to a new file on `machine`, piece by piece, then checks it is all there.
fn send(machine: &Machine, id: &str, bundle: &[u8], job: &Job) -> Result<files::Upload, String> {
    let total = bundle.len() as u64;
    let pieces = protocol::chunks(bundle.len(), protocol::CHUNK);
    let mut upload: Option<files::Upload> = None;
    for range in pieces {
        job.go_on()?;
        job.set(Step::Sending {
            sent: range.start as u64,
            total,
        });
        let piece = &bundle[range];
        match &mut upload {
            None => upload = Some(files::Upload::start(machine, id, piece)?),
            Some(up) => up.append(piece)?,
        }
    }
    let upload = upload.ok_or("The bundle couldn't be written.")?;
    job.set(Step::Sending { sent: total, total });
    let arrived = upload.size()?;
    if arrived != total {
        return Err(format!(
            "Only {} of the bundle's {} arrived on {}.",
            protocol::size_label(arrived),
            protocol::size_label(total),
            machine.label()
        ));
    }
    Ok(upload)
}
