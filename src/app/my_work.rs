//! "Where's my work?": one panel over every repository gitcraft knows (recent ones of every
//! profile, the open tabs, the workspaces, and those found on a machine), listing what exists
//! only there, with Push and Open. Each repository is looked at in one round trip, a few at a
//! time and a couple per other machine, in the background (see [`crate::across::runner`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{RichText, Stroke, Ui};

use super::App;
use crate::across::discover::{self, Machine};
use crate::across::my_work::{self, Work};
use crate::across::{self, runner};
use crate::git::Overview;
use crate::icons;
use crate::theme::{self, Tokens};

/// A push asked from the panel: the repository and the branch.
type PushKey = (PathBuf, String);

/// What a search of a machine found, or why it couldn't.
type Searched = (Machine, Result<Vec<PathBuf>, String>);

/// The panel's state while the window is open; what it found on machines is saved
/// ([`App::found_repos`]).
#[derive(Default)]
pub(super) struct MyWork {
    pub(super) open: bool,
    /// The repositories of the last look, in order.
    looked: Vec<PathBuf>,
    seen: HashMap<PathBuf, Result<Overview, String>>,
    waiting: Vec<PathBuf>,
    looking: Option<(Vec<PathBuf>, Receiver<runner::Done<Overview>>)>,
    /// Pushes: `None` while running.
    pushes: HashMap<PushKey, Option<Result<(), String>>>,
    push_rx: Vec<Receiver<(PushKey, Result<(), String>)>>,
    machines: Option<Vec<Machine>>,
    machines_rx: Option<Receiver<Vec<Machine>>>,
    /// Machines searched: `None` while searching, else how many were found or why not.
    searches: HashMap<Machine, Option<Result<usize, String>>>,
    search_rx: Vec<Receiver<Searched>>,
}

/// What a click in the panel asked for.
enum Ask {
    Open(PathBuf),
    Push(PathBuf, String, bool),
    LookAll,
    LookAt(Vec<PathBuf>),
    Search(Machine),
    Close,
}

impl MyWork {
    fn busy(&self) -> bool {
        self.looking.is_some() || !self.waiting.is_empty()
    }

    /// Queues a look at each of `repos` not waiting already; their last answers are dropped.
    fn look(&mut self, repos: Vec<PathBuf>) {
        for repo in repos {
            if !self.waiting.contains(&repo) {
                self.seen.remove(&repo);
                if !self.looked.contains(&repo) {
                    self.looked.push(repo.clone());
                }
                self.waiting.push(repo);
            }
        }
    }

    /// Takes in what came back; starts what waits once the running look ended. Returns the
    /// repositories machine searches found.
    fn poll(&mut self, ctx: &egui::Context) -> Vec<PathBuf> {
        if let Some((paths, rx)) = &self.looking {
            loop {
                match rx.try_recv() {
                    Ok((i, result)) => {
                        if let Some(p) = paths.get(i) {
                            self.seen.insert(p.clone(), result);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.looking = None;
                        break;
                    }
                }
            }
        }
        if self.looking.is_none() && !self.waiting.is_empty() {
            let paths = std::mem::take(&mut self.waiting);
            let wake = ctx.clone();
            let rx = runner::run_bounded(
                paths.clone(),
                across::LOOKS,
                across::LOOK_WAIT,
                |p: &PathBuf| across::machine_key(p),
                |p: PathBuf| across::look(&p),
                move || wake.request_repaint(),
            );
            self.looking = Some((paths, rx));
        }
        let mut pushed = Vec::new();
        for rx in &self.push_rx {
            for (key, result) in rx.try_iter() {
                if result.is_ok() {
                    pushed.push(key.0.clone());
                }
                self.pushes.insert(key, Some(result));
            }
        }
        self.look(pushed);
        if let Some(rx) = &self.machines_rx
            && let Ok(list) = rx.try_recv()
        {
            self.machines = Some(list);
            self.machines_rx = None;
        }
        let mut found = Vec::new();
        for rx in &self.search_rx {
            for (machine, result) in rx.try_iter() {
                let summary = result.map(|list| {
                    let n = list.len();
                    found.extend(list);
                    n
                });
                self.searches.insert(machine, Some(summary));
            }
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        found
    }

    fn push(&mut self, repo: PathBuf, branch: String, has_upstream: bool, ctx: &egui::Context) {
        let key = (repo.clone(), branch.clone());
        if matches!(self.pushes.get(&key), Some(None)) {
            return;
        }
        self.pushes.insert(key.clone(), None);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = my_work::push_branch(&repo, &branch, has_upstream);
            let _ = tx.send((key, result));
            ctx.request_repaint();
        });
        self.push_rx.push(rx);
    }

    fn search(&mut self, machine: Machine, ctx: &egui::Context) {
        if matches!(self.searches.get(&machine), Some(None)) {
            return;
        }
        self.searches.insert(machine.clone(), None);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
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
        std::thread::spawn(move || {
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
        for repo in known {
            if !self.my_work.looked.contains(&repo) {
                self.my_work.looked.push(repo);
            }
        }
        self.my_work.want_machines(ctx);
    }

    pub(super) fn my_work_ui(&mut self, ctx: &egui::Context) {
        if !self.my_work.open {
            return;
        }
        for repo in self.my_work.poll(ctx) {
            if !self.found_repos.contains(&repo) && !self.known_repos().contains(&repo) {
                self.found_repos.push(repo.clone());
            }
            self.my_work.look(vec![repo]);
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
                        if let Some(a) = results(ui, &self.my_work, &t) {
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
            Some(Ask::Close) => self.my_work.open = false,
            None => {}
        }
    }
}

/// The title, what the panel does, how far the look is, and Look again.
fn panel_header(ui: &mut Ui, w: &MyWork, t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Where's my work?").font(theme::semibold(16.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                ask = Some(Ask::Close);
            }
            if icons::small_button(ui, "refresh-cw", "Look again").clicked() {
                ask = Some(Ask::LookAll);
            }
        });
    });
    ui.label(
        RichText::new(
            "Commits not pushed, stashes, uncommitted changes and branches whose remote branch \
             is gone, in every repository gitcraft knows.",
        )
        .color(t.text_dim),
    );
    let total = w.looked.len();
    let done = w.looked.iter().filter(|p| w.seen.contains_key(*p)).count();
    if w.busy() {
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new().size(12.0));
            ui.label(RichText::new(format!("Looked at {done} of {total}…")).color(t.text_faint));
        });
    }
    ui.add_space(8.0);
    ask
}

/// The repositories with work only there, by machine; then those with none, those not looked
/// at and those that couldn't be.
fn results(ui: &mut Ui, w: &MyWork, t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    let mut machines: Vec<String> = Vec::new();
    for p in &w.looked {
        let m = my_work::machine_of(p);
        if !machines.contains(&m) {
            machines.push(m);
        }
    }
    let mut clean = 0;
    let mut failed: Vec<(&PathBuf, &String)> = Vec::new();
    let mut any_work = false;
    for machine in &machines {
        let repos: Vec<&PathBuf> = w
            .looked
            .iter()
            .filter(|p| &my_work::machine_of(p) == machine)
            .collect();
        let with_work: Vec<(&PathBuf, Vec<Work>)> = repos
            .iter()
            .filter_map(|p| match w.seen.get(*p) {
                Some(Ok(o)) => Some((*p, my_work::work_of(o))).filter(|(_, work)| !work.is_empty()),
                _ => None,
            })
            .collect();
        let unlooked: Vec<PathBuf> = repos
            .iter()
            .filter(|p| !w.seen.contains_key(**p) && !w.waiting.contains(p))
            .filter(|p| w.looking.as_ref().is_none_or(|(l, _)| !l.contains(p)))
            .map(|p| (*p).clone())
            .collect();
        for p in &repos {
            match w.seen.get(*p) {
                Some(Ok(o)) if my_work::work_of(o).is_empty() => clean += 1,
                Some(Err(e)) => failed.push((p, e)),
                _ => {}
            }
        }
        if with_work.is_empty() && unlooked.is_empty() {
            continue;
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(machine.to_uppercase())
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        if !unlooked.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} not looked at: gitcraft isn't connected there.",
                        repo_count(unlooked.len())
                    ))
                    .color(t.text_dim),
                );
                if ui.button("Connect and look").clicked() {
                    ask = Some(Ask::LookAt(unlooked.clone()));
                }
            });
        }
        for (repo, work) in &with_work {
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
            ui.label(
                "Nothing is only here: everything is committed and pushed, and nothing is stashed.",
            );
        });
    }
    if w.looked.is_empty() {
        ui.label(
            RichText::new(
                "gitcraft knows no repositories yet: open some, or find them on a machine below.",
            )
            .color(t.text_dim),
        );
    }
    if clean > 0 {
        ui.label(
            RichText::new(format!(
                "{} with nothing that is only there.",
                repo_count(clean)
            ))
            .color(t.text_faint),
        );
    }
    if !failed.is_empty() {
        egui::CollapsingHeader::new(
            RichText::new(format!("Couldn't look at {}", repo_count(failed.len()))).color(t.danger),
        )
        .id_salt("my-work-failed")
        .show(ui, |ui| {
            for (path, e) in failed {
                ui.label(
                    RichText::new(format!("{}: {e}", crate::remote::display(path)))
                        .color(t.text_dim)
                        .small(),
                );
            }
        });
    }
    ask
}

/// "1 repository", "3 repositories".
fn repo_count(n: usize) -> String {
    if n == 1 {
        "1 repository".to_owned()
    } else {
        format!("{n} repositories")
    }
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
                    if ui.button("Open").clicked() {
                        ask = Some(Ask::Open(repo.to_owned()));
                    }
                });
            });
            for item in work {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    icons::inline(ui, item.icon(), 13.0, t.icon);
                    ui.label(item.describe());
                    if let Work::Unpushed {
                        branch, upstream, ..
                    } = item
                        && let Some(a) = push_button(ui, w, repo, branch, upstream.is_some(), t)
                    {
                        ask = Some(a);
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
    match w.pushes.get(&key) {
        Some(None) => {
            ui.add(crate::spinner::Spinner::new().size(12.0));
            ui.label(RichText::new("pushing…").color(t.text_faint));
            None
        }
        Some(Some(Err(e))) => {
            ui.label(RichText::new(format!("push: {}", across::short_reason(e))).color(t.danger))
                .on_hover_text(e);
            ui.button("Push")
                .on_hover_text("Try again")
                .clicked()
                .then(|| Ask::Push(repo.to_owned(), branch.to_owned(), has_upstream))
        }
        _ => {
            let hint = if has_upstream {
                "Push it to its upstream (asks for no password)"
            } else {
                "Push it to origin and track it there (asks for no password)"
            };
            ui.button("Push")
                .on_hover_text(hint)
                .clicked()
                .then(|| Ask::Push(repo.to_owned(), branch.to_owned(), has_upstream))
        }
    }
}

/// The machines whose home folders can be searched for repositories gitcraft doesn't know yet.
fn machines_line(ui: &mut Ui, w: &MyWork, t: &Tokens) -> Option<Ask> {
    let mut ask = None;
    ui.label(
        RichText::new("FIND MORE REPOSITORIES")
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.label(
        RichText::new(format!(
            "Searches a machine's home folder, {} folders deep (hidden ones skipped); what is \
             found is remembered.",
            discover::DEPTH
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
                    ui.label(RichText::new(format!("{}…", machine.label())).color(t.text_dim));
                }
                state => {
                    let label = match state {
                        Some(Some(Ok(n))) => format!("{} (found {n})", machine.label()),
                        Some(Some(Err(_))) => format!("{} (failed)", machine.label()),
                        _ => machine.label(),
                    };
                    let mut b = ui.button(label);
                    if let Some(Some(Err(e))) = state {
                        b = b.on_hover_text(e);
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
