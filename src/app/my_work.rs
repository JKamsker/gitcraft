//! crate::i18n::text("interaction-my-work-title"): one panel over every repository gitcraft knows (recent ones of every
//! profile, the open tabs, the workspaces, and those found on a machine), listing what exists
//! only there, with Open, and Push where a branch has a place to go. Each repository is looked
//! at in one round trip, a few at a time and a couple per other machine, in the background
//! (see [`crate::across::runner`]).

mod view;
use view::*;
mod controls;
use controls::*;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use egui::{RichText, Stroke, Ui};

use super::App;
use crate::across::discover::{self, Found, Machine};
use crate::across::my_work::{self, Work};
use crate::across::{self, runner};
use crate::git::Overview;
use crate::icons;
use crate::theme::{self, Tokens};

/// A push asked from the panel: the repository and the branch.
type PushKey = (PathBuf, String);

/// A push that ended: what it was, its machine, and how it went.
type Pushed = (PushKey, String, Result<(), String>);

/// What a search of a machine found, or why it couldn't.
type Searched = (Machine, Result<Found, String>);

/// How a search went: how many were found, and whether it stopped at its most; or why not.
type SearchSummary = Result<(usize, bool), String>;

/// The looks of one batch: each repository with the request it answers.
type Batch = (Vec<(PathBuf, u64)>, Receiver<runner::Done<Overview>>);

/// The panel's state while the window is open; what it found on machines is saved
/// ([`App::found_repos`]).
#[derive(Default)]
pub(super) struct MyWork {
    pub(super) open: bool,
    /// The repositories of the panel, in order, and the same as a set.
    looked: Vec<PathBuf>,
    listed: HashSet<PathBuf>,
    /// Each repository's last answer, kept while a newer look runs.
    seen: HashMap<PathBuf, Result<Overview, String>>,
    /// The latest request of each repository: an answer to an older one is dropped.
    asked: HashMap<PathBuf, u64>,
    last_ask: u64,
    /// The repositories whose latest look hasn't answered yet.
    pending: HashSet<PathBuf>,
    /// Looks asked since the last poll, started there in a batch of their own.
    waiting: Vec<(PathBuf, u64)>,
    batches: Vec<Batch>,
    /// Of the looks since the panel was last idle: how many were asked, and answered.
    round: (usize, usize),
    /// Pushes: `None` while waiting or running.
    pushes: HashMap<PushKey, Option<Result<(), String>>>,
    /// Pushes waiting for room on their machine, with whether each goes to its upstream.
    push_queue: Vec<(PushKey, bool)>,
    /// Pushes running, by machine ([`across::machine_key`]).
    pushing: HashMap<String, usize>,
    push_done: Option<(Sender<Pushed>, Receiver<Pushed>)>,
    machines: Option<Vec<Machine>>,
    machines_rx: Option<Receiver<Vec<Machine>>>,
    /// Machines searched: `None` while searching.
    searches: HashMap<Machine, Option<SearchSummary>>,
    search_rx: Vec<Receiver<Searched>>,
}

/// What a click in the panel asked for.
enum Ask {
    Open(PathBuf),
    Push(PathBuf, String, bool),
    LookAll,
    LookAt(Vec<PathBuf>),
    Search(Machine),
    Forget(PathBuf),
    Close,
}

impl MyWork {
    fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Lists `repo` in the panel, without looking at it.
    fn list(&mut self, repo: &Path) {
        if self.listed.insert(repo.to_owned()) {
            self.looked.push(repo.to_owned());
        }
    }

    /// Takes `repo` out of the panel.
    fn forget(&mut self, repo: &Path) {
        if self.listed.remove(repo) {
            self.looked.retain(|p| p != repo);
        }
        self.seen.remove(repo);
        self.pending.remove(repo);
        self.asked.remove(repo);
    }

    /// Asks a look at each of `repos` not being looked at already; it starts at the next
    /// poll, beside any look running. Their last answers stay until the new ones come.
    fn look(&mut self, repos: Vec<PathBuf>) {
        for repo in repos {
            if self.pending.contains(&repo) {
                continue;
            }
            if self.pending.is_empty() {
                self.round = (0, 0);
            }
            self.last_ask += 1;
            self.asked.insert(repo.clone(), self.last_ask);
            self.pending.insert(repo.clone());
            self.list(&repo);
            self.round.0 += 1;
            self.waiting.push((repo, self.last_ask));
        }
    }

    /// Takes in what came back and starts what was asked. Returns what machine searches
    /// found.
    fn poll(&mut self, ctx: &egui::Context) -> Vec<(Machine, Found)> {
        if !self.waiting.is_empty() {
            let batch = std::mem::take(&mut self.waiting);
            let paths: Vec<PathBuf> = batch.iter().map(|(p, _)| p.clone()).collect();
            let wake = ctx.clone();
            let rx = runner::run_bounded(
                paths,
                across::LOOKS,
                across::LOOK_WAIT,
                |p: &PathBuf| across::machine_key(p),
                |p: PathBuf| across::look(&p),
                move || wake.request_repaint(),
            );
            self.batches.push((batch, rx));
        }
        let mut answers = Vec::new();
        let mut ended = Vec::new();
        for (n, (paths, rx)) in self.batches.iter().enumerate() {
            loop {
                match rx.try_recv() {
                    Ok((i, result)) => {
                        if let Some(asked) = paths.get(i) {
                            answers.push((asked.clone(), result));
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        ended.push(n);
                        break;
                    }
                }
            }
        }
        for ((repo, ask), result) in answers {
            self.answer(repo, ask, result);
        }
        for n in ended.into_iter().rev() {
            let (paths, _) = self.batches.remove(n);
            // What it never answered (it can't, but a look mustn't wait for ever).
            for (repo, ask) in paths {
                if self.asked.get(&repo) == Some(&ask) {
                    self.pending.remove(&repo);
                }
            }
        }
        self.take_pushes(ctx);
        if let Some(rx) = &self.machines_rx
            && let Ok(list) = rx.try_recv()
        {
            self.machines = Some(list);
            self.machines_rx = None;
        }
        let mut found = Vec::new();
        for rx in &self.search_rx {
            for (machine, result) in rx.try_iter() {
                let summary = match result {
                    Ok(f) => {
                        let summary = (f.repos.len(), f.cut);
                        found.push((machine.clone(), f));
                        Ok(summary)
                    }
                    Err(e) => Err(e),
                };
                self.searches.insert(machine, Some(summary));
            }
        }
        if self.busy() || !self.pushing.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        found
    }

    /// One look's answer, unless a newer look of the same repository was asked since.
    fn answer(&mut self, repo: PathBuf, ask: u64, result: Result<Overview, String>) {
        if self.asked.get(&repo) != Some(&ask) {
            return;
        }
        self.pending.remove(&repo);
        self.round.1 += 1;
        // A failed push said what was; the new look says what is.
        self.pushes
            .retain(|(r, _), state| r != &repo || !matches!(state, Some(Err(_))));
        self.seen.insert(repo, result);
    }

    fn push(&mut self, repo: PathBuf, branch: String, has_upstream: bool, ctx: &egui::Context) {
        let key = (repo, branch);
        if matches!(self.pushes.get(&key), Some(None)) {
            return;
        }
        self.pushes.insert(key.clone(), None);
        self.push_queue.push((key, has_upstream));
        self.start_pushes(ctx);
    }

    /// Starts the pushes waiting whose machine has room: as many at once there as fetches
    /// (two of its three shells kept for network operations, so a tab's own still gets one).
    fn start_pushes(&mut self, ctx: &egui::Context) {
        let (tx, _) = self.push_done.get_or_insert_with(channel);
        let tx = tx.clone();
        let queue = std::mem::take(&mut self.push_queue);
        for (key, has_upstream) in queue {
            let machine = across::machine_key(&key.0);
            let running = self.pushing.entry(machine.clone()).or_default();
            if *running >= across::FETCHES.per_group {
                self.push_queue.push((key, has_upstream));
                continue;
            }
            *running += 1;
            let (tx, ctx) = (tx.clone(), ctx.clone());
            crate::connections::spawn(move || {
                let result = my_work::push_branch(&key.0, &key.1, has_upstream);
                let _ = tx.send((key, machine, result));
                ctx.request_repaint();
            });
        }
    }

    /// Takes in the pushes that ended; a branch pushed gets its repository looked at again.
    fn take_pushes(&mut self, ctx: &egui::Context) {
        let Some((_, rx)) = &self.push_done else {
            return;
        };
        let done: Vec<Pushed> = rx.try_iter().collect();
        if done.is_empty() {
            return;
        }
        let mut pushed = Vec::new();
        for (key, machine, result) in done {
            if let Some(n) = self.pushing.get_mut(&machine) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    self.pushing.remove(&machine);
                }
            }
            if result.is_ok() {
                pushed.push(key.0.clone());
            }
            self.pushes.insert(key, Some(result));
        }
        self.look(pushed);
        self.start_pushes(ctx);
    }

    fn search(&mut self, machine: Machine, ctx: &egui::Context) {
        if matches!(self.searches.get(&machine), Some(None)) {
            return;
        }
        self.searches.insert(machine.clone(), None);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        crate::connections::spawn(move || {
            let result = discover::find_repos(&machine);
            let _ = tx.send((machine, result));
            ctx.request_repaint();
        });
        self.search_rx.push(rx);
    }

    /// The machines that can be searched, asked once in the background (`wsl.exe` can be slow).
    fn want_machines(&mut self, ctx: &egui::Context) {
        if self.machines.is_some() || self.machines_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        crate::connections::spawn(move || {
            let _ = tx.send(discover::machines());
            ctx.request_repaint();
        });
        self.machines_rx = Some(rx);
    }
}

/// Of `repos`, those that can be looked at without asking: here, or on a machine connected now.
fn reachable(repos: &[PathBuf]) -> Vec<PathBuf> {
    repos
        .iter()
        .filter(|p| crate::remote::parse(p).is_none_or(|t| crate::remote::is_connected(&t.host)))
        .cloned()
        .collect()
}

/// `found` (a search's repositories) in the spelling gitcraft already knows each by, when it
/// knows it: the same folder elsewhere can be written more than one way (`\\wsl$\…` and
/// `\\wsl.localhost\…`, a `/` at the end).
fn known_spelling(found: Vec<PathBuf>, known: &[PathBuf]) -> Vec<PathBuf> {
    let by_target: HashMap<(crate::remote::Host, String), &PathBuf> = known
        .iter()
        .filter_map(|p| crate::remote::parse(p).map(|t| ((t.host, t.path), p)))
        .collect();
    found
        .into_iter()
        .map(|p| match crate::remote::parse(&p) {
            Some(t) => by_target
                .get(&(t.host, t.path))
                .map_or(p, |known| (*known).clone()),
            None => p,
        })
        .collect()
}

impl App {
    /// Every repository gitcraft knows of: recent ones of every profile, the open tabs, the
    /// workspaces' and those found on machines.
    fn known_repos(&self) -> Vec<PathBuf> {
        let open = self.open_repos().0;
        let others: Vec<PathBuf> = self
            .profile_states
            .values()
            .flat_map(|s| s.recent.iter().chain(&s.open).cloned())
            .collect();
        let workspaces = self.workspaces.all_repos();
        my_work::known_repos([
            self.recent.as_slice(),
            open.as_slice(),
            others.as_slice(),
            workspaces.as_slice(),
            self.found_repos.as_slice(),
        ])
    }

    /// Opens the panel and looks at every repository reachable without asking.
    pub(super) fn open_my_work(&mut self, ctx: &egui::Context) {
        self.my_work.open = true;
        let known = self.known_repos();
        self.my_work.look(reachable(&known));
        for repo in &known {
            self.my_work.list(repo);
        }
        self.my_work.want_machines(ctx);
    }

    /// What a search of `machine` found replaces what its last one did; what is new is looked
    /// at.
    fn take_search(&mut self, machine: &Machine, found: Found) {
        let known = self.known_repos();
        let found = known_spelling(found.repos, &known);
        let found_set: HashSet<&PathBuf> = found.iter().collect();
        let known_set: HashSet<&PathBuf> = known.iter().collect();
        let dropped: Vec<PathBuf> = self
            .found_repos
            .iter()
            .filter(|p| &Machine::of(p) == machine && !found_set.contains(p))
            .cloned()
            .collect();
        self.found_repos.retain(|p| !dropped.contains(p));
        let new: Vec<PathBuf> = found
            .iter()
            .filter(|p| !known_set.contains(p))
            .cloned()
            .collect();
        self.found_repos.extend(new.iter().cloned());
        if !dropped.is_empty() {
            let still: HashSet<PathBuf> = self.known_repos().into_iter().collect();
            for p in dropped.iter().filter(|p| !still.contains(*p)) {
                self.my_work.forget(p);
            }
        }
        self.my_work.look(new);
    }

    /// Takes `repo` out of the repositories found on machines (its folder is gone).
    fn forget_found(&mut self, repo: &Path) {
        self.found_repos.retain(|p| p != repo);
        if !self.known_repos().iter().any(|p| p == repo) {
            self.my_work.forget(repo);
        }
    }

    pub(super) fn poll_my_work(&mut self, ctx: &egui::Context) {
        for (machine, found) in self.my_work.poll(ctx) {
            self.take_search(&machine, found);
        }
    }

    pub(super) fn my_work_ui(&mut self, ctx: &egui::Context) {
        if !self.my_work.open {
            return;
        }
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut ask = None;
        let resp = egui::Modal::new(egui::Id::new("my-work"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(720.0);
                ask = panel_header(ui, &self.my_work, &t);
                let max = (ctx.content_rect().height() - 180.0).max(240.0);
                egui::ScrollArea::vertical()
                    .max_height(max)
                    .min_scrolled_height(max.min(420.0))
                    .show(ui, |ui| {
                        if let Some(a) = results(ui, &self.my_work, &self.found_repos, &t) {
                            ask = Some(a);
                        }
                        ui.add_space(12.0);
                        if let Some(a) = machines_line(ui, &self.my_work, &t) {
                            ask = Some(a);
                        }
                    });
            });
        if resp.should_close() {
            ask = Some(Ask::Close);
        }
        match ask {
            Some(Ask::Open(path)) => {
                self.my_work.open = false;
                self.open_repo(&path, ctx);
            }
            Some(Ask::Push(repo, branch, up)) => self.my_work.push(repo, branch, up, ctx),
            Some(Ask::LookAll) => {
                let known = self.known_repos();
                self.my_work.look(reachable(&known));
            }
            Some(Ask::LookAt(repos)) => self.my_work.look(repos),
            Some(Ask::Search(machine)) => self.my_work.search(machine, ctx),
            Some(Ask::Forget(repo)) => self.forget_found(&repo),
            Some(Ask::Close) => self.my_work.open = false,
            None => {}
        }
    }
}

#[cfg(test)]
#[path = "my_work_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "my_work/localization_tests.rs"]
mod localization_tests;
