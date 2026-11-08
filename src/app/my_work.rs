//! crate::i18n::text("interaction-my-work-title"): one panel over every repository gitcraft knows (recent ones of every
//! profile, the open tabs, the workspaces, and those found on a machine), listing what exists
//! only there, with Open, and Push where a branch has a place to go. Each repository is looked
//! at in one round trip, a few at a time and a couple per other machine, in the background
//! (see [`crate::across::runner`]).

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

/// The title, what the panel does, how far the look is, and Look again.
fn panel_header(ui: &mut Ui, w: &MyWork, t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text("interaction-my-work-title"))
                .font(theme::semibold(16.0)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", crate::i18n::text("interaction-my-work-close"))
                .clicked()
            {
                ask = Some(Ask::Close);
            }
            if icons::small_button(
                ui,
                "refresh-cw",
                crate::i18n::text("interaction-my-work-look"),
            )
            .clicked()
            {
                ask = Some(Ask::LookAll);
            }
        });
    });
    ui.label(RichText::new(crate::i18n::text("interaction-my-work-intro")).color(t.text_dim));
    if w.busy() {
        let (total, done) = w.round;
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new().size(12.0));
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-my-work-progress",
                    &[("done", done.into()), ("total", total.into())],
                ))
                .color(t.text_faint),
            );
        });
    }
    ui.add_space(8.0);
    ask
}

/// The repositories of one machine, sorted out by what their looks said.
#[derive(Default)]
struct MachineRows<'a> {
    with_work: Vec<(&'a PathBuf, Vec<Work>)>,
    unlooked: Vec<PathBuf>,
}

/// The repositories with work only there, by machine; then those with none, those not looked
/// at and those that couldn't be.
fn results(ui: &mut Ui, w: &MyWork, found: &[PathBuf], t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    let mut machines: Vec<(String, MachineRows)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut clean = 0;
    let mut unlooked_in_all = 0;
    let mut failed: Vec<(&PathBuf, &String)> = Vec::new();
    for p in &w.looked {
        let m = my_work::machine_of(p);
        let at = *index.entry(m.clone()).or_insert_with(|| {
            machines.push((m, MachineRows::default()));
            machines.len() - 1
        });
        let rows = &mut machines[at].1;
        match w.seen.get(p) {
            Some(Ok(o)) => {
                let work = my_work::work_of(o);
                if work.is_empty() {
                    clean += 1;
                } else {
                    rows.with_work.push((p, work));
                }
            }
            Some(Err(e)) => failed.push((p, e)),
            None if !w.pending.contains(p) => {
                unlooked_in_all += 1;
                rows.unlooked.push(p.clone());
            }
            None => {}
        }
    }
    let mut any_work = false;
    for (machine, rows) in &machines {
        if rows.with_work.is_empty() && rows.unlooked.is_empty() {
            continue;
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(if machine == "This computer" {
                crate::i18n::text("interaction-my-work-this-computer").to_uppercase()
            } else {
                machine.to_uppercase()
            })
            .font(theme::semibold(10.5))
            .color(t.text_faint),
        );
        if !rows.unlooked.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "interaction-my-work-unlooked",
                        &[("count", rows.unlooked.len().into())],
                    ))
                    .color(t.text_dim),
                );
                if ui
                    .button(crate::i18n::text("interaction-my-work-connect-look"))
                    .clicked()
                {
                    ask = Some(Ask::LookAt(rows.unlooked.clone()));
                }
            });
        }
        for (repo, work) in &rows.with_work {
            any_work = true;
            if let Some(a) = repo_block(ui, w, repo, work, t) {
                ask = Some(a);
            }
        }
    }
    ui.add_space(8.0);
    if !any_work && !w.busy() && !w.looked.is_empty() && failed.is_empty() {
        ui.horizontal(|ui| {
            icons::inline(ui, "check", 14.0, t.success);
            if unlooked_in_all == 0 {
                ui.label(crate::i18n::text("interaction-my-work-clean"));
            } else {
                ui.label(crate::i18n::message(
                    "interaction-my-work-clean-looked",
                    &[("count", clean.into())],
                ));
            }
        });
    }
    if w.looked.is_empty() {
        ui.label(RichText::new(crate::i18n::text("interaction-my-work-empty")).color(t.text_dim));
    }
    if clean > 0 {
        ui.label(
            RichText::new(crate::i18n::message(
                "interaction-my-work-clean-count",
                &[("count", clean.into())],
            ))
            .color(t.text_faint),
        );
    }
    if !failed.is_empty() {
        egui::CollapsingHeader::new(
            RichText::new(crate::i18n::message(
                "interaction-my-work-failed-look",
                &[("count", failed.len().into())],
            ))
            .color(t.danger),
        )
        .id_salt("my-work-failed")
        .show(ui, |ui| {
            for (path, e) in failed {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {}",
                            crate::remote::display(path),
                            across::display_error(e)
                        ))
                        .color(t.text_dim)
                        .small(),
                    );
                    if found.contains(path)
                        && matches!(
                            across::short_reason(e),
                            "folder not found" | "not a repository"
                        )
                        && icons::small_button(
                            ui,
                            "x",
                            crate::i18n::text("interaction-my-work-forget"),
                        )
                        .clicked()
                    {
                        ask = Some(Ask::Forget(path.clone()));
                    }
                });
            }
        });
    }
    ask
}

/// One repository: its name, Open, and a row per piece of work.
fn repo_block(ui: &mut Ui, w: &MyWork, repo: &Path, work: &[Work], t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    egui::Frame::new()
        .fill(t.dock)
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(super::workspaces::repo_name(repo)).font(theme::semibold(13.0)),
                );
                ui.label(
                    RichText::new(crate::remote::display(repo))
                        .color(t.text_faint)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(crate::i18n::text("interaction-my-work-open"))
                        .clicked()
                    {
                        ask = Some(Ask::Open(repo.to_owned()));
                    }
                });
            });
            for item in work {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    icons::inline(ui, item.icon(), 13.0, t.icon);
                    ui.label(item.describe());
                    if let Some((branch, up)) = item.push() {
                        if let Some(a) = push_button(ui, w, repo, branch, up, t) {
                            ask = Some(a);
                        }
                    } else if let Some(why) = item.why_no_push() {
                        ui.label(
                            RichText::new(crate::i18n::text("interaction-my-work-open-push"))
                                .color(t.text_faint),
                        )
                        .on_hover_text(why);
                    }
                });
            }
        });
    ui.add_space(4.0);
    ask
}

/// Push for one branch, or how its push went.
fn push_button(
    ui: &mut Ui,
    w: &MyWork,
    repo: &Path,
    branch: &str,
    has_upstream: bool,
    t: &Tokens,
) -> Option<Ask> {
    let key = (repo.to_owned(), branch.to_owned());
    let ask = || Ask::Push(repo.to_owned(), branch.to_owned(), has_upstream);
    // Asking another machine connects to it, which may ask for a password: only when said so.
    let far = crate::remote::parse(repo)
        .map(|t| t.host)
        .filter(|h| !crate::remote::is_connected(h));
    let label = if far.is_some() {
        crate::i18n::text("interaction-my-work-connect-push")
    } else {
        crate::i18n::text("interaction-my-work-push")
    };
    match w.pushes.get(&key) {
        Some(None) => {
            ui.add(crate::spinner::Spinner::new().size(12.0));
            let queued = w.push_queue.iter().any(|(k, _)| k == &key);
            let text = if queued {
                crate::i18n::text("interaction-my-work-waiting")
            } else {
                crate::i18n::text("interaction-my-work-pushing")
            };
            ui.label(RichText::new(text).color(t.text_faint));
            None
        }
        Some(Some(Err(e))) => {
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-my-work-push-error",
                    &[("reason", across::display_reason(e).into())],
                ))
                .color(t.danger),
            )
            .on_hover_text(across::display_error(e));
            ui.button(label)
                .on_hover_text(crate::i18n::text("interaction-my-work-retry"))
                .clicked()
                .then(ask)
        }
        _ => {
            let tracked = if has_upstream { "yes" } else { "no" };
            let hint = match &far {
                Some(host) => crate::i18n::message(
                    "interaction-my-work-push-remote",
                    &[("host", host.label().into()), ("tracked", tracked.into())],
                ),
                None => crate::i18n::message(
                    "interaction-my-work-push-local",
                    &[("tracked", tracked.into())],
                ),
            };
            ui.button(label).on_hover_text(hint).clicked().then(ask)
        }
    }
}

/// The machines whose home folders can be searched for repositories gitcraft doesn't know yet.
fn machines_line(ui: &mut Ui, w: &MyWork, t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    ui.label(
        RichText::new(crate::i18n::text("interaction-my-work-find"))
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.label(
        RichText::new(crate::i18n::message(
            "interaction-my-work-search-description",
            &[("depth", discover::DEPTH.into())],
        ))
        .color(t.text_faint)
        .small(),
    );
    let Some(machines) = &w.machines else {
        ui.add(crate::spinner::Spinner::new().size(12.0));
        return None;
    };
    ui.horizontal_wrapped(|ui| {
        for machine in machines {
            match w.searches.get(machine) {
                Some(None) => {
                    ui.add(crate::spinner::Spinner::new().size(12.0));
                    ui.label(
                        RichText::new(format!("{}…", machine.display_label())).color(t.text_dim),
                    );
                }
                state => {
                    let label = match state {
                        Some(Some(Ok((n, true)))) => crate::i18n::message(
                            "interaction-my-work-found-cut",
                            &[
                                ("machine", machine.display_label().into()),
                                ("count", (*n).into()),
                            ],
                        ),
                        Some(Some(Ok((n, false)))) => crate::i18n::message(
                            "interaction-my-work-found",
                            &[
                                ("machine", machine.display_label().into()),
                                ("count", (*n).into()),
                            ],
                        ),
                        Some(Some(Err(_))) => crate::i18n::message(
                            "interaction-my-work-search-failed",
                            &[("machine", machine.display_label().into())],
                        ),
                        _ => machine.display_label(),
                    };
                    let mut b = ui.button(label);
                    if let Some(Some(Err(e))) = state {
                        b = b.on_hover_text(across::display_error(e));
                    }
                    if b.clicked() {
                        ask = Some(Ask::Search(machine.clone()));
                    }
                }
            }
        }
    });
    ask
}

#[cfg(test)]
#[path = "my_work_tests.rs"]
mod tests;

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_my_work_results_translate_and_only_return_the_original_open_request() {
        crate::i18n::set_language("de");
        let path = PathBuf::from("/opaque/repo 日本語");
        let mut work = MyWork::default();
        work.looked.push(path.clone());
        work.seen.insert(
            path.clone(),
            Ok(Overview {
                changed: 1234,
                ..Default::default()
            }),
        );
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, opened: &mut Option<PathBuf>| {
                if !fonts.replace(true) {
                    theme::install_fonts(ui.ctx());
                    return;
                }
                let t = Tokens::get(ui.ctx());
                panel_header(ui, &work, &t);
                if let Some(Ask::Open(path)) = results(ui, &work, &[], &t) {
                    *opened = Some(path);
                }
                assert!(
                    work.pushes.is_empty() && work.push_queue.is_empty() && work.batches.is_empty()
                );
            },
            None,
        );
        h.run();
        h.get_by_label("Wo ist meine Arbeit?");
        h.get_by_label("DIESER RECHNER");
        h.get_by_label("1.234 Dateien mit nicht committeten Änderungen");
        h.get_by_label("Öffnen").click();
        h.run();
        assert_eq!(*h.state(), Some(path));
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label("Where's my work?");
        h.get_by_label("1,234 files with uncommitted changes");
    }

    #[test]
    fn actual_push_control_returns_original_branch_and_tracking_flag_without_dispatch() {
        crate::i18n::set_language("de");
        let work = MyWork::default();
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, requested: &mut bool| {
                if !fonts.replace(true) {
                    theme::install_fonts(ui.ctx());
                    return;
                }
                let t = Tokens::get(ui.ctx());
                if let Some(Ask::Push(path, branch, tracked)) = push_button(
                    ui,
                    &work,
                    Path::new("/opaque/repo"),
                    "feature/日本語",
                    false,
                    &t,
                ) {
                    assert_eq!(path, Path::new("/opaque/repo"));
                    assert_eq!(branch, "feature/日本語");
                    assert!(!tracked);
                    *requested = true;
                }
                assert!(work.pushes.is_empty() && work.push_queue.is_empty());
            },
            false,
        );
        h.run();
        h.get_by_label("Pushen").click();
        h.run();
        assert!(*h.state());
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label("Push");
    }

    #[test]
    fn work_descriptions_keep_push_classification_raw_and_unknown_errors_opaque() {
        crate::i18n::set_language("de");
        let item = Work::Unpushed {
            branch: "feature/日本語".into(),
            upstream: Some("origin/日本語".into()),
            commits: 1234,
            tracks: crate::git::Tracks::SameName,
        };
        assert_eq!(
            item.describe(),
            "feature/日本語: 1.234 Commits nicht nach origin/日本語 gepusht"
        );
        assert_eq!(item.push(), Some(("feature/日本語", true)));
        let other = Work::Unpushed {
            branch: "feature/日本語".into(),
            upstream: Some("origin/日本語".into()),
            commits: 1234,
            tracks: crate::git::Tracks::OtherName,
        };
        assert!(other.push().is_none());
        assert!(
            other
                .why_no_push()
                .unwrap()
                .contains("feature/日本語 folgt origin/日本語")
        );
        assert_eq!(
            Work::Stashes(vec!["wip 日本語".into(), "older".into()]).describe(),
            "2 Stashes, der neueste: wip 日本語"
        );
        let raw = "feature/日本語 has no upstream set any more";
        assert_eq!(
            across::display_error(raw),
            "Für feature/日本語 ist kein Upstream mehr festgelegt"
        );
        assert_eq!(raw, "feature/日本語 has no upstream set any more");
        assert_eq!(
            across::display_reason("fatal: not a git repository"),
            "kein Repository"
        );
        assert_eq!(
            across::short_reason("fatal: not a git repository"),
            "not a repository"
        );
        for raw in [
            "External diagnostic 日本語",
            "prefix feature has no upstream set any more",
            "no answer within banana; it may still be running",
        ] {
            assert_eq!(across::display_error(raw), raw);
        }
        assert_eq!(across::discover::Machine::Here.label(), "This computer");
        assert_eq!(
            across::discover::Machine::Here.display_label(),
            "Dieser Rechner"
        );
        crate::i18n::set_language("en-US");
        assert_eq!(across::display_error(raw), raw);
    }
}
