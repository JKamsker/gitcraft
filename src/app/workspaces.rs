//! The start page's workspaces: a card per named group of repositories, each repository with
//! its branch, uncommitted files, ahead / behind and last fetch, and Fetch all / Open all for
//! the group. Every look and fetch runs in the background, a few at a time (see
//! [`crate::across::runner`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Rect, RichText, Sense, Ui, pos2, vec2};

use super::App;
use crate::across::{self, runner, workspaces::Workspaces};
use crate::git::{self, Overview};
use crate::icons;
use crate::theme::{self, Tokens};

/// After this long, what a look found is looked for again when the start page shows.
const STALE_AFTER: Duration = Duration::from_secs(120);

/// What is known of one repository of a workspace.
#[derive(Default)]
struct RepoState {
    /// The last look's answer.
    seen: Option<Result<Overview, String>>,
    /// When that answer came.
    seen_at: Option<Instant>,
    /// Being looked at or fetched now.
    busy: Option<Busy>,
    /// Why its last fetch failed.
    fetch_error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Looking,
    Fetching,
}

/// Jobs running for many repositories, their results by index into `paths`.
struct Batch<R> {
    paths: Vec<PathBuf>,
    rx: Receiver<runner::Done<R>>,
}

/// A name being typed: for a new workspace, or for the one at an index.
struct Naming {
    index: Option<usize>,
    text: String,
    error: Option<String>,
    focus: bool,
}

/// The start page's workspaces, as they show: not saved (the workspaces themselves are, in
/// [`App::workspaces`]).
#[derive(Default)]
pub(super) struct WorkspacesView {
    repos: HashMap<PathBuf, RepoState>,
    looks: Vec<Batch<Overview>>,
    fetches: Vec<Batch<()>>,
    naming: Option<Naming>,
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
    /// Starts a look at each of `paths` not busy already.
    fn look(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let paths = self.mark(paths, Busy::Looking);
        if paths.is_empty() {
            return;
        }
        let ctx = ctx.clone();
        let rx = runner::run_bounded(
            paths.clone(),
            across::AT_ONCE,
            across::LOOK_FOR,
            |path: PathBuf| git::overview(&path, across::LOOK_FOR),
            move || ctx.request_repaint(),
        );
        self.looks.push(Batch { paths, rx });
    }

    fn fetch(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let paths = self.mark(paths, Busy::Fetching);
        if paths.is_empty() {
            return;
        }
        let ctx = ctx.clone();
        let rx = runner::run_bounded(
            paths.clone(),
            across::AT_ONCE,
            across::FETCH_FOR,
            |path: PathBuf| across::fetch(&path),
            move || ctx.request_repaint(),
        );
        self.fetches.push(Batch { paths, rx });
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

    /// Takes in what came back; a fetch done is followed by a look.
    fn poll(&mut self, ctx: &egui::Context) {
        for batch in &self.looks {
            for (i, result) in batch.rx.try_iter() {
                if let Some(path) = batch.paths.get(i) {
                    let state = self.repos.entry(path.clone()).or_default();
                    state.busy = None;
                    state.seen = Some(result);
                    state.seen_at = Some(Instant::now());
                }
            }
        }
        let mut fetched = Vec::new();
        for batch in &self.fetches {
            for (i, result) in batch.rx.try_iter() {
                if let Some(path) = batch.paths.get(i) {
                    let state = self.repos.entry(path.clone()).or_default();
                    state.busy = None;
                    state.fetch_error = result.err();
                    fetched.push(path.clone());
                }
            }
        }
        let done = |paths: &[PathBuf], repos: &HashMap<PathBuf, RepoState>| {
            paths
                .iter()
                .all(|p| repos.get(p).is_none_or(|s| s.busy.is_none()))
        };
        let repos = &self.repos;
        self.looks.retain(|b| !done(&b.paths, repos));
        self.fetches.retain(|b| !done(&b.paths, repos));
        self.look(fetched, ctx);
    }

    /// Looks at the repositories of `ws` never looked at yet, or not for a while.
    fn look_at_new(&mut self, ws: &Workspaces, ctx: &egui::Context) {
        let new: Vec<PathBuf> = ws
            .all_repos()
            .into_iter()
            .filter(|p| {
                self.repos.get(p).is_none_or(|s| {
                    s.busy.is_none() && s.seen_at.is_none_or(|at| at.elapsed() > STALE_AFTER)
                })
            })
            .collect();
        self.look(new, ctx);
    }
}

impl App {
    /// The workspaces' cards on the start page, and the button that makes one.
    pub(super) fn workspaces_section(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        self.workspaces_view.poll(&ctx);
        self.workspaces_view.look_at_new(&self.workspaces, &ctx);

        let mut request = None;
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("WORKSPACES")
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let new = icons::small_button(ui, "plus", "New workspace");
                    if new.clicked() && self.workspaces_view.naming.is_none() {
                        self.workspaces_view.naming = Some(Naming {
                            index: None,
                            text: String::new(),
                            error: None,
                            focus: true,
                        });
                    }
                });
            });
            if self.workspaces.is_empty() && self.workspaces_view.naming.is_none() {
                ui.label(
                    RichText::new(
                        "Group repositories you work on together; see how each stands, \
                         and fetch or open them all at once.",
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
    }

    fn workspace_request(&mut self, request: Request, ctx: &egui::Context) {
        match request {
            Request::Open(paths) => {
                for path in paths {
                    self.open_repo(&path, ctx);
                }
            }
            Request::Look(paths) => self.workspaces_view.look(paths, ctx),
            Request::Fetch(paths) => self.workspaces_view.fetch(paths, ctx),
            Request::Add(i, path) => {
                self.workspaces.add_repo(i, &path);
            }
            Request::Browse(i) => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Add a repository to the workspace")
                    .pick_folder()
                {
                    match super::repo_root(&dir) {
                        Ok(root) => {
                            self.workspaces.add_repo(i, &root);
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Request::RemoveRepo(i, path) => self.workspaces.remove_repo(i, &path),
            Request::Delete(i) => {
                self.workspaces.remove(i);
                self.workspaces_view.naming = None;
            }
        }
    }

    /// The field a workspace's name is typed in, with Save and Cancel.
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
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let label = if naming.index.is_some() {
                "Rename"
            } else {
                "Create"
            };
            if ui.button(label).clicked() || enter {
                done = Some(true);
            } else if ui.button("Cancel").clicked() || escape {
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
        let ws = self.workspaces.list.get(index)?.clone();
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
                } else if let Some(r) = self.workspace_header(ui, index, &ws.name, &ws.repos, t) {
                    request = Some(r);
                }
                if ws.repos.is_empty() {
                    ui.label(
                        RichText::new("No repositories yet: add one with +.")
                            .color(t.text_faint)
                            .small(),
                    );
                }
                ui.spacing_mut().item_spacing.y = 0.0;
                for path in &ws.repos {
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
            icons::inline(ui, "rows-2", 14.0, t.accent);
            ui.label(
                RichText::new(name)
                    .font(theme::semibold(13.0))
                    .color(t.text),
            );
            ui.label(
                RichText::new(match repos.len() {
                    1 => "1 repository".to_owned(),
                    n => format!("{n} repositories"),
                })
                .color(t.text_faint)
                .small(),
            );
            if busy {
                ui.add(crate::spinner::Spinner::new().size(12.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let more = icons::small_button(ui, "settings", "Rename or delete");
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
                        if ui.button("Delete workspace").clicked() {
                            request = Some(Request::Delete(index));
                        }
                    });
                let has = !repos.is_empty();
                let open = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(ui, "folder-open", "Open all in tabs")
                });
                if open.inner.clicked() {
                    request = Some(Request::Open(repos.to_vec()));
                }
                let fetch = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(ui, "cloud-download", "Fetch all")
                });
                if fetch.inner.clicked() {
                    request = Some(Request::Fetch(repos.to_vec()));
                }
                let look = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(ui, "refresh-cw", "Look again")
                });
                if look.inner.clicked() {
                    request = Some(Request::Look(repos.to_vec()));
                }
                let add = icons::small_button(ui, "plus", "Add a repository");
                if let Some(r) = self.add_menu(&add, index, repos) {
                    request = Some(r);
                }
            });
        });
        request
    }

    /// What can be added: the open tabs' and the recent repositories not in it, or another
    /// folder.
    fn add_menu(
        &self,
        button: &egui::Response,
        index: usize,
        repos: &[PathBuf],
    ) -> Option<Request> {
        let mut candidates: Vec<PathBuf> = self.open_repos().0;
        for p in &self.recent {
            if !candidates.contains(p) {
                candidates.push(p.clone());
            }
        }
        candidates.retain(|p| !repos.contains(p));
        let mut request = None;
        egui::Popup::menu(button)
            .style(theme::menu_style)
            .show(|ui| {
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

/// One repository of a workspace: its name and machine on the left, how it stands on the right.
fn workspace_row(
    ui: &mut Ui,
    path: &Path,
    state: Option<&RepoState>,
    t: &Tokens,
) -> Option<RowClick> {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
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
    let status = status_parts(state, t);
    let status_width = paint_parts(ui, rect, &status, hovered);
    let left = ui.painter_at(rect.with_max_x(rect.right() - status_width - 12.0));
    left.text(
        pos2(rect.left() + 30.0, y - 7.0),
        Align2::LEFT_CENTER,
        repo_name(path),
        theme::medium(12.5),
        t.text,
    );
    left.text(
        pos2(rect.left() + 30.0, y + 8.0),
        Align2::LEFT_CENTER,
        crate::remote::display(path),
        egui::FontId::proportional(10.5),
        t.text_faint,
    );
    let mut click = None;
    if hovered {
        let x_rect = Rect::from_center_size(pos2(rect.right() - 12.0, y), vec2(20.0, 20.0));
        let x = ui
            .interact(x_rect, resp.id.with("remove"), Sense::click())
            .on_hover_text("Remove from the workspace");
        if x.hovered() {
            ui.painter().rect_filled(x_rect, t.radius_sm, t.pressed);
        }
        icons::paint(ui, x_rect, "x", 12.0, t.text_faint);
        if x.clicked() {
            click = Some(RowClick::Remove);
        }
    }
    let tip = row_tooltip(path, state);
    if click.is_none() && resp.on_hover_text(tip).clicked() {
        click = Some(RowClick::Open);
    }
    click
}

/// A piece of a row's status: an icon (or none) and a text, in a colour.
struct Part {
    icon: Option<&'static str>,
    text: String,
    color: Color32,
}

fn part(icon: Option<&'static str>, text: impl Into<String>, color: Color32) -> Part {
    Part {
        icon,
        text: text.into(),
        color,
    }
}

/// What a row says of its repository, left to right.
fn status_parts(state: Option<&RepoState>, t: &Tokens) -> Vec<Part> {
    let Some(state) = state else {
        return vec![part(None, "…", t.text_faint)];
    };
    let mut parts = Vec::new();
    match state.busy {
        Some(Busy::Fetching) => parts.push(part(Some("cloud-download"), "fetching…", t.text_dim)),
        Some(Busy::Looking) if state.seen.is_none() => {
            parts.push(part(None, "looking…", t.text_faint));
        }
        _ => {}
    }
    if state.fetch_error.is_some() {
        parts.push(part(Some("circle-alert"), "fetch failed", t.danger));
    }
    match &state.seen {
        Some(Ok(o)) => parts.extend(overview_parts(o, git::now(), t)),
        Some(Err(_)) => parts.push(part(Some("circle-alert"), "couldn't look", t.danger)),
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
    let mut parts = vec![part(Some("git-branch"), branch, t.text_dim)];
    if o.changed > 0 {
        parts.push(part(None, changed_text(o.changed), t.warning));
    }
    if o.ahead > 0 {
        parts.push(part(Some("arrow-up"), o.ahead.to_string(), t.accent));
    }
    if o.behind > 0 {
        parts.push(part(Some("arrow-down"), o.behind.to_string(), t.accent));
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

/// Paints `parts` right-aligned in `rect` (leaving room for the remove button while
/// `hovered`); returns the width they took.
fn paint_parts(ui: &Ui, rect: Rect, parts: &[Part], hovered: bool) -> f32 {
    const GAP: f32 = 10.0;
    const ICON: f32 = 12.0;
    let right = rect.right() - if hovered { 30.0 } else { 8.0 };
    let mut x = right;
    let font = egui::FontId::proportional(11.5);
    for p in parts.iter().rev() {
        let galley = ui
            .painter()
            .layout_no_wrap(p.text.clone(), font.clone(), p.color);
        x -= galley.size().x;
        ui.painter().galley(
            pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            p.color,
        );
        if let Some(icon) = p.icon {
            x -= ICON + 3.0;
            icons::paint(
                ui,
                Rect::from_min_size(pos2(x, rect.center().y - ICON / 2.0), vec2(ICON, ICON)),
                icon,
                ICON,
                p.color,
            );
        }
        x -= GAP;
    }
    right - x
}

/// The row's whole story on hover: the path, and any error in full.
fn row_tooltip(path: &Path, state: Option<&RepoState>) -> String {
    let mut tip = format!("{}\nClick to open", crate::remote::display(path));
    if let Some(state) = state {
        if let Some(Err(e)) = &state.seen {
            tip.push_str(&format!("\n\nCouldn't look at it: {e}"));
        }
        if let Some(e) = &state.fetch_error {
            tip.push_str(&format!("\n\nThe last fetch failed: {e}"));
        }
        if let Some(Ok(o)) = &state.seen
            && !o.upstream.is_empty()
        {
            tip.push_str(&format!("\nTracks {}", o.upstream));
        }
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
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
        git::run(Some(dir), args)
    }

    fn scratch(name: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-workspaces-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    /// A clone of a bare repository, set up to commit.
    fn clone_of(origin: &Path, dest: &Path) -> Result<(), String> {
        let parent = dest.parent().ok_or("no parent")?;
        let name = dest.file_name().ok_or("no name")?.to_string_lossy();
        git(parent, &["clone", "-q", &origin.to_string_lossy(), &name])?;
        for args in [
            &["config", "user.name", "T"][..],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            git(dest, args)?;
        }
        Ok(())
    }

    /// Polls `view` until nothing runs any more.
    fn settle(view: &mut WorkspacesView, ctx: &egui::Context) -> Result<(), String> {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !view.looks.is_empty() || !view.fetches.is_empty() {
            if std::time::Instant::now() > until {
                return Err("the looks never finished".to_owned());
            }
            view.poll(ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(())
    }

    /// What the start page does with a workspace: looks at each repository (one gone), then
    /// fetches them all and looks again, which then shows what the remote got meanwhile.
    #[test]
    fn a_workspace_is_looked_at_and_fetched() -> Result<(), String> {
        let root = scratch("fetch")?;
        git(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"])?;
        let origin = root.join("origin.git");
        let mine = root.join("mine ü");
        let theirs = root.join("theirs");
        clone_of(&origin, &mine)?;
        clone_of(&origin, &theirs)?;
        std::fs::write(theirs.join("a"), "1").map_err(|e| e.to_string())?;
        git(&theirs, &["add", "-A"])?;
        git(&theirs, &["commit", "-qm", "one"])?;
        git(&theirs, &["push", "-q", "origin", "HEAD:main"])?;
        git(&mine, &["fetch", "-q"])?;
        git(&mine, &["checkout", "-q", "main"])?;
        std::fs::write(theirs.join("a"), "2").map_err(|e| e.to_string())?;
        git(&theirs, &["commit", "-qam", "two"])?;
        git(&theirs, &["push", "-q", "origin", "HEAD:main"])?;
        std::fs::write(mine.join("new.txt"), "x").map_err(|e| e.to_string())?;
        let gone = root.join("gone");

        let mut ws = Workspaces::default();
        let i = ws.create("W")?;
        ws.add_repo(i, &mine);
        ws.add_repo(i, &gone);
        let ctx = egui::Context::default();
        let mut view = WorkspacesView::default();
        view.look_at_new(&ws, &ctx);
        settle(&mut view, &ctx)?;
        let seen = |view: &WorkspacesView, p: &Path| view.repos.get(p).and_then(|s| s.seen.clone());
        let o = seen(&view, &mine).ok_or("not looked at")??;
        assert_eq!(o.branch.as_deref(), Some("main"));
        assert_eq!((o.ahead, o.behind, o.changed), (0, 0, 1));
        assert!(o.last_fetch.is_some());
        assert!(seen(&view, &gone).is_some_and(|r| r.is_err()));
        // Looked at already: not again until asked.
        view.look_at_new(&ws, &ctx);
        assert!(view.looks.is_empty());

        view.fetch(ws.all_repos(), &ctx);
        settle(&mut view, &ctx)?;
        let o = seen(&view, &mine).ok_or("not looked at")??;
        assert_eq!(o.behind, 1, "fetched, then looked at again");
        assert_eq!(
            view.repos.get(&mine).and_then(|s| s.fetch_error.clone()),
            None
        );
        assert!(
            view.repos
                .get(&gone)
                .is_some_and(|s| s.fetch_error.is_some())
        );
        Ok(())
    }

    /// A repository being looked at isn't looked at a second time meanwhile.
    #[test]
    fn a_busy_repository_isnt_asked_twice() {
        let mut view = WorkspacesView::default();
        let p = PathBuf::from("/nowhere/x");
        assert_eq!(
            view.mark(vec![p.clone(), p.clone()], Busy::Looking),
            vec![p.clone()]
        );
        assert!(view.mark(vec![p.clone()], Busy::Fetching).is_empty());
    }

    #[test]
    fn texts_of_a_row() {
        assert_eq!(changed_text(3), "3 changed");
        assert_eq!(fetched_text(None, 100), "never fetched");
        assert_eq!(fetched_text(Some(100 - 7200), 100), "fetched 2 hours ago");
        assert_eq!(repo_name(Path::new("/home/me/wörk repo")), "wörk repo");
        assert_eq!(repo_name(Path::new("ssh://build/~/x")), "x");
    }

    #[test]
    fn a_rows_parts_say_what_matters() {
        let t = Tokens::for_kind(crate::theme::ThemeKind::default());
        let o = Overview {
            branch: Some("main".into()),
            changed: 2,
            ahead: 1,
            behind: 0,
            has_remote: true,
            ..Overview::default()
        };
        let texts: Vec<String> = overview_parts(&o, 0, &t)
            .into_iter()
            .map(|p| p.text)
            .collect();
        assert_eq!(texts, ["main", "2 changed", "1", "never fetched"]);
        let detached = Overview {
            head: "abc1234".into(),
            ..Overview::default()
        };
        let texts: Vec<String> = overview_parts(&detached, 0, &t)
            .into_iter()
            .map(|p| p.text)
            .collect();
        assert_eq!(texts, ["detached at abc1234", "no remote"]);
    }
}
