//! The start page's workspaces: a card per named group of repositories, each repository with
//! its branch, uncommitted files, ahead / behind and last fetch, and Fetch all / Open all for
//! the group. Every look and fetch runs in the background, a few at a time and only a couple
//! per other machine (see [`crate::across::runner`]); one queue each, whatever asked.

mod row;
mod view;
pub(in crate::app) use row::*;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Rect, RichText, Sense, Ui, pos2, vec2};

use super::App;
use crate::across::{self, runner};
use crate::git::{self, Overview};
use crate::icons;
use crate::theme::{self, Tokens};

/// After this long, what a look found is looked for again when the start page shows.
const STALE_AFTER: Duration = Duration::from_secs(120);

/// Open all asks first past this many repositories.
const OPEN_WITHOUT_ASKING: usize = 8;

/// What is known of one repository of a workspace.
#[derive(Default)]
struct RepoState {
    /// The last look's answer, and when it came.
    seen: Option<Result<Overview, String>>,
    seen_at: Option<Instant>,
    /// Being looked at or fetched now (or waiting its turn).
    busy: Option<Busy>,
    /// Why its last fetch failed, and when (Unix seconds): cleared by a later fetch, here or
    /// anywhere else, as a look finds.
    fetch_error: Option<(String, i64)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Looking,
    Fetching,
}

/// The jobs of one kind: those waiting, and the batch running (one at a time, so its limits
/// hold for everything of that kind).
struct Queue<R> {
    waiting: Vec<PathBuf>,
    running: Option<(Vec<PathBuf>, Receiver<runner::Done<R>>)>,
}

impl<R> Default for Queue<R> {
    fn default() -> Self {
        Self {
            waiting: Vec::new(),
            running: None,
        }
    }
}

/// A finished job: its repository and result.
type Finished<R> = (PathBuf, Result<R, String>);

impl<R: Send + 'static> Queue<R> {
    fn is_idle(&self) -> bool {
        self.waiting.is_empty() && self.running.is_none()
    }

    /// The results that came, and starts the waiting ones once the batch running ended.
    fn poll(
        &mut self,
        start: impl FnOnce(Vec<PathBuf>) -> Receiver<runner::Done<R>>,
    ) -> Vec<Finished<R>> {
        let mut done = Vec::new();
        if let Some((paths, rx)) = &self.running {
            loop {
                match rx.try_recv() {
                    Ok((i, result)) => done.extend(paths.get(i).map(|p| (p.clone(), result))),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.running = None;
                        break;
                    }
                }
            }
        }
        if self.running.is_none() && !self.waiting.is_empty() {
            let paths = std::mem::take(&mut self.waiting);
            let rx = start(paths.clone());
            self.running = Some((paths, rx));
        }
        done
    }
}

/// A name being typed: for a new workspace, or for the one at an index.
struct Naming {
    index: Option<usize>,
    text: String,
    error: Option<String>,
    focus: bool,
}

/// A step asked to be confirmed, on the workspace at an index.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Delete(usize),
    OpenAll(usize),
}

/// The start page's workspaces, as they show: not saved (the workspaces themselves are, in
/// [`App::workspaces`]).
#[derive(Default)]
pub(super) struct WorkspacesView {
    repos: HashMap<PathBuf, RepoState>,
    looks: Queue<Overview>,
    fetches: Queue<()>,
    naming: Option<Naming>,
    confirm: Option<Confirm>,
    /// A word for the card at an index (a repository that was in it already…).
    notice: Option<(usize, String)>,
}

/// What a click on the cards asked for, done once they are drawn.
enum Request {
    Open(Vec<PathBuf>),
    Look(Vec<PathBuf>),
    Fetch(Vec<PathBuf>),
    Add(usize, PathBuf),
    Browse(usize),
    RemoveRepo(usize, PathBuf),
    Delete(usize),
}

impl WorkspacesView {
    /// Queues a look at each of `paths` not busy already.
    fn look(&mut self, paths: Vec<PathBuf>) {
        let paths = self.mark(paths, Busy::Looking);
        self.looks.waiting.extend(paths);
    }

    fn fetch(&mut self, paths: Vec<PathBuf>) {
        let paths = self.mark(paths, Busy::Fetching);
        self.fetches.waiting.extend(paths);
    }

    /// Of `paths`, those not busy, marked `busy`.
    fn mark(&mut self, paths: Vec<PathBuf>, busy: Busy) -> Vec<PathBuf> {
        paths
            .into_iter()
            .filter(|p| {
                let state = self.repos.entry(p.clone()).or_default();
                if state.busy.is_some() {
                    return false;
                }
                state.busy = Some(busy);
                true
            })
            .collect()
    }

    fn busy(&self) -> bool {
        !self.looks.is_idle() || !self.fetches.is_idle()
    }

    /// Takes in what came back, of repositories still in a workspace (`known`); a fetch done is
    /// followed by a look. Forgets the repositories no workspace has any more.
    fn poll(&mut self, known: &HashSet<PathBuf>, ctx: &egui::Context) {
        self.repos.retain(|p, _| known.contains(p));
        let wake = |ctx: &egui::Context| {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        };
        let looked = self.looks.poll(|paths| {
            runner::run_bounded(
                paths,
                across::LOOKS,
                across::LOOK_WAIT,
                |p: &PathBuf| across::machine_key(p),
                |p: PathBuf| across::look(&p),
                wake(ctx),
            )
        });
        for (path, result) in looked {
            if let Some(state) = self.repos.get_mut(&path) {
                state.busy = None;
                if let (Ok(o), Some((_, failed_at))) = (&result, &state.fetch_error)
                    && o.last_fetch.is_some_and(|at| at >= *failed_at)
                {
                    state.fetch_error = None;
                }
                state.seen = Some(result);
                state.seen_at = Some(Instant::now());
            }
        }
        let fetched = self.fetches.poll(|paths| {
            runner::run_bounded(
                paths,
                across::FETCHES,
                across::FETCH_WAIT,
                |p: &PathBuf| across::machine_key(p),
                |p: PathBuf| across::fetch(&p),
                wake(ctx),
            )
        });
        let mut again = Vec::new();
        for (path, result) in fetched {
            if let Some(state) = self.repos.get_mut(&path) {
                state.busy = None;
                state.fetch_error = result.err().map(|e| (e, git::now()));
                again.push(path);
            }
        }
        self.look(again);
        if self.busy() {
            // The next batch starts on a later frame: come back for it.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    /// Looks at the repositories of `all` never looked at yet, or not for a while. One on
    /// another machine only while a connection to it is open: connecting may ask for a
    /// password, which only an explicit Look again, Fetch all or Open should.
    fn look_unasked(&mut self, all: &[PathBuf]) {
        let due: Vec<PathBuf> = all
            .iter()
            .filter(|p| {
                self.repos.get(*p).is_none_or(|s| {
                    s.busy.is_none() && s.seen_at.is_none_or(|at| at.elapsed() > STALE_AFTER)
                })
            })
            .filter(|p| {
                crate::remote::parse(p).is_none_or(|t| crate::remote::is_connected(&t.host))
            })
            .cloned()
            .collect();
        self.look(due);
    }
}

impl App {
    /// The workspaces' cards on the start page, and the button that makes one.
    pub(super) fn workspaces_section(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let all = self.workspaces.all_repos();
        let known: HashSet<PathBuf> = all.iter().cloned().collect();
        self.workspaces_view.poll(&known, &ctx);
        self.workspaces_view.look_unasked(&all);

        let mut request = None;
        let mut open_my_work = false;
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-workspace-heading"))
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::small_button(
                        ui,
                        "plus",
                        crate::i18n::text("interaction-workspace-new"),
                    )
                    .clicked()
                    {
                        self.workspaces_view.naming = Some(Naming {
                            index: None,
                            text: String::new(),
                            error: None,
                            focus: true,
                        });
                    }
                    let find = ui
                        .link(
                            RichText::new(crate::i18n::text("interaction-workspace-my-work"))
                                .small(),
                        )
                        .on_hover_text(crate::i18n::text("interaction-workspace-my-work-tip"));
                    if find.clicked() {
                        open_my_work = true;
                    }
                });
            });
            if self.workspaces.is_empty() && self.workspaces_view.naming.is_none() {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-workspace-intro"))
                        .color(t.text_faint)
                        .small(),
                );
            }
            if self
                .workspaces_view
                .naming
                .as_ref()
                .is_some_and(|n| n.index.is_none())
            {
                self.naming_field(ui, &t);
            }
            for i in 0..self.workspaces.list.len() {
                ui.add_space(8.0);
                if let Some(r) = self.workspace_card(ui, i, &t) {
                    request = Some(r);
                }
            }
        });
        if let Some(r) = request {
            self.workspace_request(r, &ctx);
        }
        if open_my_work {
            self.open_my_work(&ctx);
        }
    }

    fn workspace_request(&mut self, request: Request, ctx: &egui::Context) {
        match request {
            Request::Open(paths) => self.open_many(&paths, ctx),
            Request::Look(paths) => self.workspaces_view.look(paths),
            Request::Fetch(paths) => self.workspaces_view.fetch(paths),
            Request::Add(i, path) => self.add_to_workspace(i, &path),
            Request::Browse(i) => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title(crate::i18n::text("interaction-workspace-browse-title"))
                    .pick_folder()
                {
                    // One on another machine is taken as it is: finding its top folder would
                    // wait for that machine. Its first look says if it isn't a repository.
                    let root = if crate::remote::is_remote(&dir) {
                        Ok(dir)
                    } else {
                        super::repo_root(&dir)
                    };
                    match root {
                        Ok(root) => self.add_to_workspace(i, &root),
                        Err(e) => self.workspaces_view.notice = Some((i, e)),
                    }
                }
            }
            Request::RemoveRepo(i, path) => self.workspaces.remove_repo(i, &path),
            Request::Delete(i) => {
                self.workspaces.remove(i);
                self.workspaces_view.naming = None;
                self.workspaces_view.notice = None;
            }
        }
        ctx.request_repaint();
    }

    fn add_to_workspace(&mut self, i: usize, path: &Path) {
        if self.workspaces.add_repo(i, path) {
            self.workspaces_view.notice = None;
            let spelled = self.workspaces.spelling_of(path);
            self.workspaces_view.look(vec![spelled]);
        } else {
            self.workspaces_view.notice = Some((
                i,
                crate::i18n::message(
                    "interaction-workspace-already",
                    &[("repository", repo_name(path).into())],
                ),
            ));
        }
    }

    /// Opens each of `paths` in a tab; those that can't be opened are named once, together.
    fn open_many(&mut self, paths: &[PathBuf], ctx: &egui::Context) {
        let mut failed = Vec::new();
        for path in paths {
            if crate::remote::is_remote(path) {
                self.open_repo(path, ctx);
                continue;
            }
            match super::repo_root(path) {
                Ok(root) => self.open_root(root, ctx),
                Err(e) => failed.push(format!(
                    "{} ({})",
                    repo_name(path),
                    across::display_reason(&e)
                )),
            }
        }
        if !failed.is_empty() {
            let msg = crate::i18n::message(
                "interaction-workspace-failed-open",
                &[
                    ("failed", failed.len().into()),
                    ("total", paths.len().into()),
                    ("details", failed.join(", ").into()),
                ],
            );
            self.toasts
                .push(super::panels::Toast::new(msg.clone(), true));
            self.error = Some(msg.into());
        }
    }
}
#[cfg(test)]
#[path = "workspaces_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "workspaces/localization_tests.rs"]
mod localization_tests;
