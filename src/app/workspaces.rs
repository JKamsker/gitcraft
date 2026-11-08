//! The start page's workspaces: a card per named group of repositories, each repository with
//! its branch, uncommitted files, ahead / behind and last fetch, and Fetch all / Open all for
//! the group. Every look and fetch runs in the background, a few at a time and only a couple
//! per other machine (see [`crate::across::runner`]); one queue each, whatever asked.

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
                    RichText::new("WORKSPACES")
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::small_button(ui, "plus", "New workspace").clicked() {
                        self.workspaces_view.naming = Some(Naming {
                            index: None,
                            text: String::new(),
                            error: None,
                            focus: true,
                        });
                    }
                    let find = ui
                        .link(RichText::new("Where's my work?").small())
                        .on_hover_text(
                            "Commits not pushed, stashes and changes in every repository \
                             gitcraft knows",
                        );
                    if find.clicked() {
                        open_my_work = true;
                    }
                });
            });
            if self.workspaces.is_empty() && self.workspaces_view.naming.is_none() {
                ui.label(
                    RichText::new(
                        "Group repositories you work on together to see how each stands, and \
                         fetch or open them all at once. + on the right makes one.",
                    )
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
                    .set_title("Add a repository to the workspace")
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
                format!("{} is in this workspace already.", repo_name(path)),
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
                    across::short_reason(&e)
                )),
            }
        }
        if !failed.is_empty() {
            let msg = format!(
                "{} of {} didn't open: {}",
                failed.len(),
                paths.len(),
                failed.join(", ")
            );
            self.toasts
                .push(super::panels::Toast::new(msg.clone(), true));
            self.error = Some(msg);
        }
    }

    /// The field a workspace's name is typed in, with Create (or Rename) and Cancel.
    fn naming_field(&mut self, ui: &mut Ui, t: &Tokens) {
        let Some(naming) = &mut self.workspaces_view.naming else {
            return;
        };
        let mut done = None;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut naming.text)
                    .hint_text("Workspace name")
                    .desired_width(260.0)
                    .char_limit(across::workspaces::MAX_NAME),
            );
            if std::mem::take(&mut naming.focus) {
                field.request_focus();
            }
            let (enter, escape) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            // Only the field's own keys: Escape closing a menu elsewhere keeps the name.
            let own = field.has_focus() || field.lost_focus();
            let label = if naming.index.is_some() {
                "Rename"
            } else {
                "Create"
            };
            if ui.button(label).clicked() || (field.lost_focus() && enter) {
                done = Some(true);
            } else if ui.button("Cancel").clicked() || (own && escape) {
                done = Some(false);
            }
        });
        if let Some(err) = &naming.error {
            ui.colored_label(t.danger, err);
        }
        match done {
            Some(true) => {
                let result = match naming.index {
                    Some(i) => self.workspaces.rename(i, &naming.text),
                    None => self.workspaces.create(&naming.text).map(|_| ()),
                };
                match result {
                    Ok(()) => self.workspaces_view.naming = None,
                    Err(e) => {
                        naming.error = Some(e);
                        naming.focus = true;
                    }
                }
            }
            Some(false) => self.workspaces_view.naming = None,
            None => {}
        }
    }

    /// One workspace: its header with the group's buttons, then a row per repository.
    fn workspace_card(&mut self, ui: &mut Ui, index: usize, t: &Tokens) -> Option<Request> {
        let (name, repos) = {
            let ws = self.workspaces.list.get(index)?;
            (ws.name.clone(), ws.repos.clone())
        };
        let mut request = None;
        egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let renaming = self
                    .workspaces_view
                    .naming
                    .as_ref()
                    .is_some_and(|n| n.index == Some(index));
                if renaming {
                    self.naming_field(ui, t);
                } else if let Some(r) = self.workspace_header(ui, index, &name, &repos, t) {
                    request = Some(r);
                }
                if let Some(r) = self.confirm_line(ui, index, &name, &repos, t) {
                    request = Some(r);
                }
                if let Some((i, text)) = &self.workspaces_view.notice
                    && *i == index
                {
                    ui.colored_label(t.warning, text);
                }
                if repos.is_empty() {
                    ui.label(
                        RichText::new("No repositories yet: + on this card adds one.")
                            .color(t.text_faint)
                            .small(),
                    );
                }
                ui.spacing_mut().item_spacing.y = 0.0;
                for path in &repos {
                    let state = self.workspaces_view.repos.get(path);
                    match workspace_row(ui, path, state, t) {
                        Some(RowClick::Open) => request = Some(Request::Open(vec![path.clone()])),
                        Some(RowClick::Remove) => {
                            request = Some(Request::RemoveRepo(index, path.clone()));
                        }
                        None => {}
                    }
                }
            });
        request
    }

    /// The question a Delete or a big Open all asks first, under the card's header.
    fn confirm_line(
        &mut self,
        ui: &mut Ui,
        index: usize,
        name: &str,
        repos: &[PathBuf],
        t: &Tokens,
    ) -> Option<Request> {
        let asked = self.workspaces_view.confirm?;
        let (text, button, request) = match asked {
            Confirm::Delete(i) if i == index => (
                format!(
                    "Delete the workspace “{name}”? Its repositories stay where they are; only \
                     the group goes."
                ),
                "Delete",
                Request::Delete(index),
            ),
            Confirm::OpenAll(i) if i == index => (
                format!("Open all {} repositories in tabs?", repos.len()),
                "Open all",
                Request::Open(repos.to_vec()),
            ),
            _ => return None,
        };
        let mut answer = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(text).color(t.text_dim));
            if ui.button(button).clicked() {
                answer = Some(true);
            }
            if ui.button("Cancel").clicked() {
                answer = Some(false);
            }
        });
        let yes = answer?;
        self.workspaces_view.confirm = None;
        yes.then_some(request)
    }

    fn workspace_header(
        &mut self,
        ui: &mut Ui,
        index: usize,
        name: &str,
        repos: &[PathBuf],
        t: &Tokens,
    ) -> Option<Request> {
        let mut request = None;
        let busy = repos.iter().any(|p| {
            self.workspaces_view
                .repos
                .get(p)
                .is_some_and(|s| s.busy.is_some())
        });
        ui.horizontal(|ui| {
            // The buttons first, from the right: a long name takes what is left and is cut.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let more = icons::small_button(ui, "settings", "Rename or delete the workspace");
                egui::Popup::menu(&more)
                    .style(theme::menu_style)
                    .show(|ui| {
                        if ui.button("Rename…").clicked() {
                            self.workspaces_view.naming = Some(Naming {
                                index: Some(index),
                                text: name.to_owned(),
                                error: None,
                                focus: true,
                            });
                        }
                        if ui.button("Delete workspace…").clicked() {
                            self.workspaces_view.confirm = Some(Confirm::Delete(index));
                        }
                    });
                let has = !repos.is_empty();
                let open = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(ui, "folder-open", "Open all in tabs")
                });
                if open.inner.clicked() {
                    if repos.len() > OPEN_WITHOUT_ASKING {
                        self.workspaces_view.confirm = Some(Confirm::OpenAll(index));
                    } else {
                        request = Some(Request::Open(repos.to_vec()));
                    }
                }
                let fetch = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(ui, "cloud-download", "Fetch all (asks for no password)")
                });
                if fetch.inner.clicked() {
                    request = Some(Request::Fetch(repos.to_vec()));
                }
                let look = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(ui, "refresh-cw", "Look again (connects to other machines)")
                });
                if look.inner.clicked() {
                    request = Some(Request::Look(repos.to_vec()));
                }
                let add = icons::small_button(ui, "plus", "Add a repository");
                if let Some(r) = self.add_menu(&add, index, repos) {
                    request = Some(r);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    icons::inline(ui, "rows-2", 14.0, t.accent);
                    let count = match repos.len() {
                        1 => "1 repository".to_owned(),
                        n => format!("{n} repositories"),
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(name)
                                .font(theme::semibold(13.0))
                                .color(t.text),
                        )
                        .truncate(),
                    )
                    .on_hover_text(format!("{name} · {count}"));
                    ui.label(RichText::new(count).color(t.text_faint).small());
                    if busy {
                        ui.add(crate::spinner::Spinner::new().size(12.0));
                    }
                });
            });
        });
        request
    }

    /// What can be added: the open tabs' and the recent repositories not in it, or another
    /// folder. Gathered only while the menu is open.
    fn add_menu(
        &self,
        button: &egui::Response,
        index: usize,
        repos: &[PathBuf],
    ) -> Option<Request> {
        let mut request = None;
        egui::Popup::menu(button)
            .style(theme::menu_style)
            .show(|ui| {
                let mut candidates: Vec<PathBuf> = self.open_repos().0;
                for p in &self.recent {
                    if !candidates.contains(p) {
                        candidates.push(p.clone());
                    }
                }
                candidates.retain(|p| !repos.contains(p));
                for path in &candidates {
                    let label = format!("{}  ·  {}", repo_name(path), crate::remote::display(path));
                    if ui.button(label).clicked() {
                        request = Some(Request::Add(index, path.clone()));
                    }
                }
                if !candidates.is_empty() {
                    ui.separator();
                }
                if ui.button("Another folder…").clicked() {
                    request = Some(Request::Browse(index));
                }
            });
        request
    }
}

enum RowClick {
    Open,
    Remove,
}

/// Room kept at the row's right end for the remove button, shown or not.
const REMOVE_ROOM: f32 = 30.0;

/// One repository of a workspace: its name and machine on the left, how it stands on the right.
fn workspace_row(
    ui: &mut Ui,
    path: &Path,
    state: Option<&RepoState>,
    t: &Tokens,
) -> Option<RowClick> {
    let (_, rect) = ui.allocate_space(vec2(ui.available_width(), 36.0));
    let resp = ui.interact(
        rect,
        ui.id().with(("workspace-repository", path)),
        Sense::click(),
    );
    crate::a11y::a11y(&resp, egui::WidgetType::Button, || row_tooltip(path, state));
    let hovered = resp.hovered() || ui.rect_contains_pointer(rect);
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    let y = rect.center().y;
    let icon = if crate::remote::is_remote(path) {
        "server"
    } else {
        "folder-git-2"
    };
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 14.0, y), vec2(16.0, 16.0)),
        icon,
        14.0,
        t.icon,
    );
    // The status gets at most 60 % of the row; the name keeps the rest.
    let text_left = rect.left() + 30.0;
    let status_rect = Rect::from_min_max(
        pos2(text_left + (rect.right() - text_left) * 0.4, rect.top()),
        pos2(rect.right() - REMOVE_ROOM, rect.bottom()),
    );
    let connected = crate::remote::parse(path).is_none_or(|t| crate::remote::is_connected(&t.host));
    let status = status_parts(state, connected, t);
    let status_width = paint_parts(ui, status_rect, status);
    let left = ui.painter_at(rect.with_max_x(status_rect.right() - status_width - 12.0));
    left.text(
        pos2(text_left, y - 7.0),
        Align2::LEFT_CENTER,
        repo_name(path),
        theme::medium(12.5),
        t.text,
    );
    left.text(
        pos2(text_left, y + 8.0),
        Align2::LEFT_CENTER,
        crate::remote::display(path),
        egui::FontId::proportional(10.5),
        t.text_faint,
    );
    let mut click = None;
    if hovered
        || crate::a11y::keyboard_on(&resp)
        || ui.memory(|m| m.has_focus(resp.id.with("remove")))
    {
        let x_rect = Rect::from_center_size(pos2(rect.right() - 12.0, y), vec2(20.0, 20.0));
        let x = ui
            .interact(x_rect, resp.id.with("remove"), Sense::click())
            .on_hover_text("Remove from the workspace");
        crate::a11y::a11y(&x, egui::WidgetType::Button, || {
            format!("Remove {} from the workspace", repo_name(path))
        });
        if x.hovered() {
            ui.painter().rect_filled(x_rect, t.radius_sm, t.pressed);
        }
        icons::paint(ui, x_rect, "x", 12.0, t.text_faint);
        if x.clicked() {
            click = Some(RowClick::Remove);
        }
    }
    if click.is_none() && resp.on_hover_text(row_tooltip(path, state)).clicked() {
        click = Some(RowClick::Open);
    }
    click
}

/// A piece of a row's status: an icon (or none) and a text, in a colour.
struct Part {
    icon: Option<&'static str>,
    text: String,
    color: Color32,
    /// Can be cut in the middle when the row is short of room (a branch's name).
    shrinks: bool,
}

fn part(icon: Option<&'static str>, text: impl Into<String>, color: Color32) -> Part {
    Part {
        icon,
        text: text.into(),
        color,
        shrinks: false,
    }
}

/// What a row says of its repository, left to right.
fn status_parts(state: Option<&RepoState>, connected: bool, t: &Tokens) -> Vec<Part> {
    let Some(state) = state.filter(|s| s.seen.is_some() || s.busy.is_some()) else {
        return vec![if connected {
            part(None, "…", t.text_faint)
        } else {
            part(None, "not connected: look again to connect", t.text_faint)
        }];
    };
    let mut parts = Vec::new();
    match state.busy {
        Some(Busy::Fetching) => parts.push(part(Some("cloud-download"), "fetching…", t.text_dim)),
        Some(Busy::Looking) if state.seen.is_none() => {
            parts.push(part(None, "looking…", t.text_faint));
        }
        _ => {}
    }
    if let Some((e, _)) = &state.fetch_error {
        let words = format!("fetch: {}", across::short_reason(e));
        parts.push(part(Some("circle-alert"), words, t.danger));
    }
    match &state.seen {
        Some(Ok(o)) => parts.extend(overview_parts(o, git::now(), t)),
        Some(Err(e)) => parts.push(part(
            Some("circle-alert"),
            across::short_reason(e),
            t.danger,
        )),
        None => {}
    }
    parts
}

/// The branch, the uncommitted files, ahead / behind and the last fetch of `o`.
fn overview_parts(o: &Overview, now: i64, t: &Tokens) -> Vec<Part> {
    let branch = match &o.branch {
        Some(b) => b.clone(),
        None if o.head.is_empty() => "no commits".to_owned(),
        None => format!("detached at {}", o.head),
    };
    let mut parts = vec![Part {
        shrinks: true,
        ..part(Some("git-branch"), branch, t.text_dim)
    }];
    if o.changed > 0 {
        parts.push(part(None, changed_text(o.changed), t.warning));
    }
    if o.ahead > 0 {
        parts.push(part(Some("arrow-up"), o.ahead.to_string(), t.text_dim));
    }
    if o.behind > 0 {
        parts.push(part(Some("arrow-down"), o.behind.to_string(), t.text_dim));
    }
    if o.has_remote {
        parts.push(part(None, fetched_text(o.last_fetch, now), t.text_faint));
    } else {
        parts.push(part(None, "no remote", t.text_faint));
    }
    parts
}

/// "1 changed", "12 changed".
pub(super) fn changed_text(n: usize) -> String {
    format!("{n} changed")
}

/// "fetched 3 hours ago", or "never fetched".
pub(super) fn fetched_text(at: Option<i64>, now: i64) -> String {
    match at {
        Some(at) => format!("fetched {}", git::relative_time(at, now)),
        None => "never fetched".to_owned(),
    }
}

/// `text` cut to `keep` characters with "…" in the middle (both ends say the most).
fn cut_middle(text: &str, keep: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= keep {
        return text.to_owned();
    }
    let keep = keep.max(2) - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let mut s: String = chars[..head].iter().collect();
    s.push('…');
    s.extend(&chars[chars.len() - tail..]);
    s
}

const PART_GAP: f32 = 10.0;
const PART_ICON: f32 = 12.0;

/// How wide `parts` paint.
fn parts_width(ui: &Ui, parts: &[Part], font: &egui::FontId) -> f32 {
    parts
        .iter()
        .map(|p| {
            let text = ui
                .painter()
                .layout_no_wrap(p.text.clone(), font.clone(), p.color)
                .size()
                .x;
            text + if p.icon.is_some() {
                PART_ICON + 3.0
            } else {
                0.0
            } + PART_GAP
        })
        .sum()
}

/// Paints `parts` right-aligned in `rect`, cutting the branch's name when they don't fit;
/// returns the width they took.
fn paint_parts(ui: &Ui, rect: Rect, mut parts: Vec<Part>) -> f32 {
    let font = egui::FontId::proportional(11.5);
    let full: Vec<String> = parts.iter().map(|p| p.text.clone()).collect();
    let mut keep = usize::MAX;
    while parts_width(ui, &parts, &font) > rect.width() {
        let Some((i, p)) = parts
            .iter_mut()
            .enumerate()
            .find(|(_, p)| p.shrinks && p.text.chars().count() > 6)
        else {
            break;
        };
        keep = keep.min(p.text.chars().count()).saturating_sub(4);
        p.text = cut_middle(&full[i], keep);
    }
    let painter = ui.painter_at(rect);
    let mut x = rect.right();
    for p in parts.iter().rev() {
        let galley = painter.layout_no_wrap(p.text.clone(), font.clone(), p.color);
        x -= galley.size().x;
        painter.galley(
            pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            p.color,
        );
        if let Some(icon) = p.icon {
            x -= PART_ICON + 3.0;
            let at = Rect::from_min_size(
                pos2(x, rect.center().y - PART_ICON / 2.0),
                vec2(PART_ICON, PART_ICON),
            );
            if rect.contains_rect(at) {
                icons::paint(ui, at, icon, PART_ICON, p.color);
            }
        }
        x -= PART_GAP;
    }
    (rect.right() - x).min(rect.width())
}

/// The row's whole story on hover: the path, and why a look or fetch failed, in full.
fn row_tooltip(path: &Path, state: Option<&RepoState>) -> String {
    let mut tip = crate::remote::display(path);
    let mut failed = false;
    if let Some(state) = state {
        if let Some(Err(e)) = &state.seen {
            tip.push_str(&format!("\n\nCouldn't look at it: {e}"));
            failed = true;
        }
        if let Some((e, _)) = &state.fetch_error {
            tip.push_str(&format!("\n\nThe last fetch failed: {e}"));
        }
        if let Some(Ok(o)) = &state.seen
            && !o.upstream.is_empty()
        {
            tip.push_str(&format!("\nTracks {}", o.upstream));
        }
    }
    if !failed {
        tip.push_str("\nClick to open");
    }
    tip
}

/// The folder's name, or the whole path for a root.
pub(super) fn repo_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| crate::remote::display(path))
}

#[cfg(test)]
#[path = "workspaces_tests.rs"]
mod tests;
