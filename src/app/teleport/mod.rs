//! Teleport (a tab's menu, or the palette): the dialog that picks a machine and where the work
//! goes there, and the teleport itself, run on a thread of its own (see `crate::teleport`).

mod view;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::panels::Toast;
use super::{App, Tab};
use crate::remote;
use crate::teleport::{self, Destination, Failed, Job, Landed, Machine, Plan, Source};

/// Where on the picked machine the work goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// One of the clones of the project found there.
    Found(usize),
    /// A repository there the user names.
    Other,
    /// A clone made there first.
    Clone,
    /// A new repository made from the bundle.
    Create,
}

/// What the background answers, once it has.
enum Loading<T> {
    Waiting(Receiver<Result<T, String>>),
    Ready(T),
    Failed(String),
}

impl<T> Loading<T> {
    fn start(ctx: &egui::Context, work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        Self::Waiting(rx)
    }

    /// Takes in the answer if it came.
    fn poll(&mut self) {
        if let Self::Waiting(rx) = self {
            match rx.try_recv() {
                Ok(Ok(value)) => *self = Self::Ready(value),
                Ok(Err(e)) => *self = Self::Failed(e),
                Err(TryRecvError::Disconnected) => {
                    *self = Self::Failed("It stopped unexpectedly.".to_owned());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// A teleport under way.
struct Running {
    job: Arc<Job>,
    rx: Receiver<Result<Landed, Failed>>,
    machine: Machine,
}

pub(super) struct TeleportForm {
    source: PathBuf,
    /// The source's branch, commit and remotes, or why its work can't go.
    info: Loading<Source>,
    /// The WSL distributions, while `wsl.exe` lists them.
    distros: Option<Receiver<Vec<String>>>,
    wsl: Vec<String>,
    ssh_hosts: Vec<String>,
    /// An SSH host typed (one not in `~/.ssh/config`).
    typed_host: String,
    /// Folders known on any machine (recent and open repositories), where clones are looked for.
    known: Vec<PathBuf>,
    machine: Option<Machine>,
    /// The picked machine's home folder, where a clone or a new repository goes by default.
    home: Loading<String>,
    /// The clones of the project found on `machine`.
    clones: Loading<Vec<PathBuf>>,
    place: Place,
    /// The user picked the place: the clones found later don't change it.
    place_picked: bool,
    /// Paths on the picked machine, as typed.
    other_repo: String,
    new_repo: String,
    clone_url: String,
    branch: String,
    folder: String,
    /// The user typed the worktree's folder: it isn't derived any more.
    folder_edited: bool,
    error: Option<String>,
    running: Option<Running>,
}

impl TeleportForm {
    fn new(source: PathBuf, known: Vec<PathBuf>, ctx: &egui::Context) -> Self {
        let path = source.clone();
        let info = Loading::start(ctx, move || teleport::inspect(&path));
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(remote::wsl_distros());
            ctx2.request_repaint();
        });
        Self {
            source,
            info,
            distros: Some(rx),
            wsl: Vec::new(),
            ssh_hosts: remote::ssh_config_hosts(),
            typed_host: String::new(),
            known,
            machine: None,
            home: Loading::Ready(String::new()),
            clones: Loading::Ready(Vec::new()),
            place: Place::Create,
            place_picked: false,
            other_repo: String::new(),
            new_repo: String::new(),
            clone_url: String::new(),
            branch: String::new(),
            folder: String::new(),
            folder_edited: false,
            error: None,
            running: None,
        }
    }

    /// Takes in what the background answered: the source, the distributions, the machine's
    /// home, the clones.
    fn poll(&mut self) {
        let was_waiting = matches!(self.info, Loading::Waiting(_));
        self.info.poll();
        if was_waiting && let Some(info) = self.info.ready() {
            self.branch = info.branch.clone().unwrap_or_default();
            self.clone_url = info
                .remotes
                .iter()
                .find(|(name, _)| name == "origin")
                .or(info.remotes.first())
                .map(|(_, url)| url.clone())
                .unwrap_or_default();
        }
        if let Some(rx) = &self.distros
            && let Ok(distros) = rx.try_recv()
        {
            self.wsl = distros;
            self.distros = None;
        }
        let was_asking = matches!(self.home, Loading::Waiting(_));
        self.home.poll();
        if was_asking && let Some(home) = self.home.ready() {
            if self.new_repo.is_empty() {
                self.new_repo = join_there(self.machine.as_ref(), home, &self.project_name());
            }
            self.derive_folder();
        }
        let was_looking = matches!(self.clones, Loading::Waiting(_));
        self.clones.poll();
        if was_looking {
            if !self.place_picked && self.clones.ready().is_some_and(|c| !c.is_empty()) {
                self.place = Place::Found(0);
            }
            self.derive_folder();
        }
    }

    /// The machines the work can go to.
    fn machines(&self) -> Vec<Machine> {
        teleport::machines(&self.source, &self.wsl, &self.ssh_hosts, &self.known)
    }

    /// Whether the background is still finding something the form needs.
    fn looking(&self) -> bool {
        matches!(self.clones, Loading::Waiting(_)) || matches!(self.home, Loading::Waiting(_))
    }

    /// The project's name, for a clone or a new repository: the remote's, else the folder's.
    fn project_name(&self) -> String {
        let from_url = crate::git::clone_dir_name(&self.clone_url);
        if !from_url.is_empty() {
            return from_url;
        }
        self.source.file_name().map_or_else(
            || "repository".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Picks `machine`: the clones of the project there are looked for among the folders known
    /// on it, its home folder is asked, and the paths offered are of that machine.
    fn pick(&mut self, machine: Machine, ctx: &egui::Context) {
        if self.machine.as_ref() == Some(&machine) {
            return;
        }
        let candidates: Vec<PathBuf> = self
            .known
            .iter()
            .filter(|p| Machine::of(p) == machine)
            .cloned()
            .collect();
        let urls: Vec<String> = self
            .info
            .ready()
            .map(|i| i.remotes.iter().map(|(_, url)| url.clone()).collect())
            .unwrap_or_default();
        self.clones = Loading::start(ctx, move || Ok(teleport::find_clones(&candidates, &urls)));
        let asked = machine.clone();
        self.home = Loading::start(ctx, move || teleport::home_of(&asked));
        self.place = if self.clone_url.is_empty() {
            Place::Create
        } else {
            Place::Clone
        };
        self.place_picked = false;
        self.new_repo.clear();
        self.other_repo.clear();
        self.folder_edited = false;
        self.error = None;
        self.machine = Some(machine);
        self.derive_folder();
    }

    /// The repository the work goes in, as this app names it, for the place picked.
    fn repo_path(&self) -> Option<PathBuf> {
        let machine = self.machine.as_ref()?;
        let typed = |s: &str| (!s.trim().is_empty()).then(|| machine.path(s.trim()));
        match self.place {
            Place::Found(i) => self.clones.ready()?.get(i).cloned(),
            Place::Other => typed(&self.other_repo),
            Place::Clone | Place::Create => typed(&self.new_repo),
        }
    }

    /// The new worktree's folder, beside the repository, unless the user typed one.
    fn derive_folder(&mut self) {
        if self.folder_edited {
            return;
        }
        let head = self
            .info
            .ready()
            .map(|i| i.head.clone())
            .unwrap_or_default();
        let branch = self.branch.trim();
        let branch = (!branch.is_empty()).then_some(branch);
        self.folder = self
            .repo_path()
            .map(|repo| remote::git_path(&teleport::worktree_folder(&repo, branch, &head)))
            .unwrap_or_default();
    }

    /// `typed` (a field of the form, `what` it is) as a folder of the picked machine, or why it
    /// isn't one: a full path there; `~/…` over SSH, where the home folder is asked for it.
    fn folder_there(&self, typed: &str, what: &str) -> Result<PathBuf, String> {
        let machine = self
            .machine
            .as_ref()
            .ok_or("Pick a machine to teleport to.")?;
        let s = typed.trim();
        if s.is_empty() {
            return Err(format!("Type a folder for {what}."));
        }
        let fits = match machine {
            Machine::There(remote::Host::Ssh(_)) => {
                s.starts_with('/') || s == "~" || s.starts_with("~/")
            }
            Machine::There(remote::Host::Wsl(_)) => s.starts_with('/'),
            Machine::Here => Path::new(s).is_absolute() && !remote::is_remote(Path::new(s)),
        };
        if fits {
            return Ok(machine.path(s));
        }
        Err(match machine {
            Machine::There(remote::Host::Ssh(_)) => format!(
                "Give {what} as a full path on {}: /home/me/x or ~/x.",
                machine.label()
            ),
            Machine::There(remote::Host::Wsl(_)) => format!(
                "Give {what} as a full path on {}: /home/me/x.",
                machine.label()
            ),
            Machine::Here => format!("Give {what} as a full path on this machine."),
        })
    }

    /// The plan the form says, or why it says none.
    fn plan(&self) -> Result<Plan, String> {
        let destination = match self.place {
            Place::Found(_) => Destination::Repo(
                self.repo_path()
                    .ok_or("Pick the repository the work goes in.")?,
            ),
            Place::Other => {
                Destination::Repo(self.folder_there(&self.other_repo, "the repository there")?)
            }
            Place::Clone => Destination::Clone {
                url: self.clone_url.trim().to_owned(),
                path: self.folder_there(&self.new_repo, "the clone")?,
            },
            Place::Create => {
                Destination::Create(self.folder_there(&self.new_repo, "the new repository")?)
            }
        };
        let folder = match self.place {
            Place::Create => PathBuf::new(),
            _ => self.folder_there(&self.folder, "the new worktree")?,
        };
        let branch = self.branch.trim();
        Ok(Plan {
            source: self.source.clone(),
            destination,
            branch: (!branch.is_empty()).then(|| branch.to_owned()),
            folder,
            confirm_above: teleport::LARGE,
        })
    }

    /// Whether Teleport can be pressed: the source is read, a machine picked, and what the
    /// form waits for (the clones there, its home folder) has come.
    fn can_start(&self) -> bool {
        self.info.ready().is_some() && self.machine.is_some() && !self.looking()
    }

    /// Starts the teleport the form says, in the background.
    fn start(&mut self, ctx: &egui::Context) {
        if self.running.is_some() || !self.can_start() {
            return;
        }
        let plan = match self.plan() {
            Ok(plan) => plan,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let Some(machine) = self.machine.clone() else {
            return;
        };
        let repaint = ctx.clone();
        let job = Arc::new(Job::new(move || repaint.request_repaint()));
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        let worker = Arc::clone(&job);
        std::thread::spawn(move || {
            let landed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                teleport::run(&plan, &worker)
            }))
            .unwrap_or_else(|_| Err(stopped()));
            let _ = tx.send(landed);
            ctx2.request_repaint();
        });
        self.error = None;
        self.running = Some(Running { job, rx, machine });
    }

    /// Back to the form after a teleport that failed, saying why.
    fn failed(&mut self, failed: Failed) {
        self.error = Some(format!("Teleport failed: {}", failed.message));
    }
}

/// A teleport whose thread ended without saying how.
fn stopped() -> Failed {
    Failed {
        message: "it stopped unexpectedly.".to_owned(),
        cancelled: false,
    }
}

/// `name` in the folder `home` of `machine`, as a path there.
fn join_there(machine: Option<&Machine>, home: &str, name: &str) -> String {
    match machine {
        Some(Machine::There(_)) => format!("{}/{name}", home.trim_end_matches('/')),
        _ => Path::new(home).join(name).display().to_string(),
    }
}

impl App {
    /// Opens the teleport dialog for the repository tab of `source`.
    pub(super) fn start_teleport(&mut self, source: PathBuf, ctx: &egui::Context) {
        if self.teleport.as_ref().is_some_and(|f| f.running.is_some()) {
            return;
        }
        let mut known: Vec<PathBuf> = self.recent.clone();
        for tab in &self.tabs {
            if let Tab::Repo(r) = tab
                && !known.contains(&r.path)
            {
                known.push(r.path.clone());
            }
        }
        self.teleport = Some(TeleportForm::new(source, known, ctx));
    }

    /// The dialog, and the end of a teleport: its worktree opened in a tab.
    pub(super) fn teleport_dialog(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.teleport else {
            return;
        };
        form.poll();
        let ended = form.running.as_ref().and_then(|r| match r.rx.try_recv() {
            Ok(result) => Some((result, r.machine.clone())),
            Err(TryRecvError::Disconnected) => Some((Err(stopped()), r.machine.clone())),
            Err(TryRecvError::Empty) => None,
        });
        if let Some((result, machine)) = ended {
            form.running = None;
            match result {
                Ok(landed) => {
                    self.teleport = None;
                    self.landed(landed, &machine, ctx);
                }
                Err(failed) if failed.cancelled => {
                    self.teleport = None;
                    self.toasts.push(Toast::new(failed.message, false));
                }
                Err(failed) => form.failed(failed),
            }
            return;
        }
        if view::show(form, ctx) {
            self.teleport = None;
        }
    }

    /// When the window closes: a running teleport is stopped and given a while to take back
    /// what it made (a temporary ref in the source, a half-made worktree there), as ending
    /// the process would leave them.
    pub(super) fn stop_teleport(&mut self) {
        let Some(running) = self.teleport.as_mut().and_then(|f| f.running.take()) else {
            return;
        };
        running.job.cancel();
        let _ = running.rx.recv_timeout(std::time::Duration::from_secs(20));
    }

    /// Says where the work arrived, and opens it.
    fn landed(&mut self, landed: Landed, machine: &Machine, ctx: &egui::Context) {
        let mut text = format!(
            "Teleported to {} ({}): {}",
            machine.label(),
            crate::git::human_size(landed.size),
            remote::git_path(&landed.folder)
        );
        for note in &landed.notes {
            text.push('\n');
            text.push_str(note);
        }
        self.toasts.push(Toast::new(text, false));
        self.open_repo(&landed.folder, ctx);
    }
}

#[cfg(all(test, unix))]
mod tests;
