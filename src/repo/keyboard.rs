//! Pane-local navigation. Requests carry the identity of the displayed file/status/diff;
//! applying one still goes through the ordinary action and operation workers.

use std::path::PathBuf;

use super::{Action, Dialog, DiffKey, Refresh, RepoTab, branch_tree, diff, ops};
use crate::git::Area;
use crate::keymap::{self, Command, Scope};
use crate::settings::Settings;
use egui::{Id, Key, Rect, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Pane {
    Sidebar,
    Graph,
    Detail,
}

#[derive(Default)]
pub(super) struct State {
    path: PathBuf,
    pub pane: Option<Pane>,
    cursor: Option<(Id, DiffKey)>,
    enter: bool,
    focus_requested: bool,
    focus_row: bool,
    parked_focus: Option<Id>,
    step: i32,
}

#[derive(Clone)]
pub(crate) struct Request {
    path: PathBuf,
    read: u64,
    epoch: u64,
    target: Target,
}

#[derive(Clone)]
enum Target {
    File {
        scope: Id,
        key: DiffKey,
        open: bool,
    },
    Hunk {
        key: DiffKey,
        request: u64,
        pick: diff::Pick,
        lines: std::collections::BTreeSet<usize>,
        confirmed: bool,
    },
}

fn same_file(a: &DiffKey, b: &DiffKey) -> bool {
    match (a, b) {
        (DiffKey::Work(a), DiffKey::Work(b)) => a.path == b.path && a.area == b.area,
        _ => a == b,
    }
}

fn available(tab: &RepoTab, ctx: &egui::Context) -> bool {
    tab.dialog.is_none()
        && tab.package.is_none()
        && tab.release.is_none()
        && tab.hotfix.finish.is_none()
        && tab.agents.dialog.is_none()
        && tab.bakeoff.picker.is_none()
        && !tab.stacks.modal_open()
        && !tab.issue_picker_open()
        && !tab.keys_blocked
        && !crate::a11y::keyboard_taken(ctx)
        && !egui::Popup::is_any_open(ctx)
        && ctx.input(|i| i.focused && !i.pointer.any_pressed())
}

/// Contextual registry commands consume the same event once; text/list editing primitives
/// stay owned by their widgets.
fn take_key(ctx: &egui::Context, key: Key, _repeat: bool) -> bool {
    let command = match key {
        Key::J => Command::FileNext,
        Key::K => Command::FilePrevious,
        Key::Space => Command::FileToggleStage,
        Key::Enter => Command::FileOpen,
        Key::S => Command::HunkStage,
        Key::U => Command::HunkUnstage,
        Key::D => Command::HunkDiscard,
        _ => return false,
    };
    keymap::take(ctx, command)
}

impl RepoTab {
    pub(crate) fn cancel_keyboard_ownership(&mut self, ctx: &egui::Context) {
        self.work_epoch = self.work_epoch.wrapping_add(1);
        *self.keyboard.borrow_mut() = State::default();
        keymap::cancel(ctx, Scope::Window);
    }

    fn keyboard_scope(&self) {
        let mut state = self.keyboard.borrow_mut();
        if state.path != self.path {
            *state = State {
                path: self.path.clone(),
                ..Default::default()
            };
        }
    }

    pub(crate) fn key_scopes(&self, ctx: &egui::Context) -> Vec<Scope> {
        let mut scopes = vec![Scope::Window, Scope::Repository];
        if self.diff.is_some()
            && self.conflict.is_none()
            && self.rebase.is_none()
            && self.split.is_none()
            && self.absorb.is_none()
            && self.main_keys(ctx, super::popout::Panel::Diff)
        {
            scopes.push(Scope::Changes);
        }
        if available(self, ctx) {
            if self.rebase.is_some() {
                scopes.push(Scope::Rebase);
                return scopes;
            }
            if self.keyboard.borrow().pane != Some(Pane::Detail)
                && (matches!(
                    self.current_view(),
                    Some(super::navigation::View::Graph { .. })
                ) || self.diff_beside_graph(ctx))
            {
                scopes.push(Scope::Graph);
            }
            match self.keyboard.borrow().pane {
                Some(Pane::Detail) => scopes.push(Scope::Files),
                None | Some(Pane::Graph) if self.diff.is_some() => scopes.push(Scope::Diff),
                _ => {}
            }
        }
        scopes
    }

    pub(super) fn keyboard_panes(&mut self, ctx: &egui::Context) {
        self.keyboard_scope();
        if self.keys_blocked && !crate::a11y::keyboard_taken(ctx) {
            if let Some(focused) = ctx.memory(|m| m.focused()) {
                self.keyboard.borrow_mut().parked_focus = Some(focused);
                ctx.memory_mut(|m| m.surrender_focus(focused));
            }
        } else if !self.keys_blocked
            && let Some(focused) = self.keyboard.borrow_mut().parked_focus.take()
            && ctx.memory(|m| m.focused().is_none())
        {
            ctx.memory_mut(|m| m.request_focus(focused));
        }
        keymap::active(ctx, self.key_scopes(ctx));
        if !available(self, ctx) {
            return;
        }
        for (command, pane) in [
            (Command::FocusSidebar, Pane::Sidebar),
            (Command::FocusGraph, Pane::Graph),
            (Command::FocusDetail, Pane::Detail),
        ] {
            if keymap::take(ctx, command) {
                self.focus_pane(pane, ctx);
            }
        }
    }

    pub(super) fn focus_pane(&self, pane: Pane, ctx: &egui::Context) {
        let mut state = self.keyboard.borrow_mut();
        state.pane = Some(pane);
        state.enter = pane == Pane::Detail;
        state.focus_requested = true;
        let mut settings = Settings::get(ctx);
        if pane == Pane::Sidebar {
            settings.show_branches = true;
        }
        if pane == Pane::Detail {
            settings.show_detail = true;
        }
        settings.set(ctx);
        ctx.request_repaint();
    }

    pub(super) fn keyboard_pane(&self, ui: &Ui, pane: Pane, rect: Rect) {
        self.keyboard_scope();
        let response = ui.interact(
            rect,
            Id::new(("repository-pane", self.repo_key(), pane)),
            egui::Sense::focusable_noninteractive(),
        );
        if ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(rect) {
            self.keyboard.borrow_mut().pane = Some(pane);
        }
        let mut state = self.keyboard.borrow_mut();
        if state.pane == Some(pane) && state.focus_requested {
            response.request_focus();
            state.focus_requested = false;
        }
        if response.has_focus()
            && !state.focus_requested
            && !self.keys_blocked
            && self.dialog.is_none()
        {
            state.pane = Some(pane);
        }
        drop(state);
        crate::a11y::pane(
            &response,
            match pane {
                Pane::Sidebar => "Repository sidebar",
                Pane::Graph => "Repository graph or diff",
                Pane::Detail => "Commit details and changed files",
            },
        );
    }

    pub(super) fn keyboard_graph_focus(&self, response: &egui::Response) {
        let mut state = self.keyboard.borrow_mut();
        if response.has_focus() && !state.focus_requested && !state.focus_row {
            state.pane = Some(Pane::Graph);
        }
    }

    pub(super) fn keyboard_graph_active(&self) -> bool {
        matches!(self.keyboard.borrow().pane, None | Some(Pane::Graph))
    }

    pub(super) fn keyboard_file_selected(&self, key: &DiffKey) -> bool {
        self.keyboard
            .borrow()
            .cursor
            .as_ref()
            .is_some_and(|(_, selected)| same_file(selected, key))
    }

    pub(super) fn keyboard_clear_cursor(&self) {
        self.keyboard.borrow_mut().cursor = None;
    }

    pub(super) fn keyboard_select_file(&self, scope: Id, key: DiffKey) {
        let mut state = self.keyboard.borrow_mut();
        state.cursor = Some((scope, key));
        state.enter = false;
    }

    pub(super) fn keyboard_file_row(&self, scope: Id, response: &egui::Response, key: &DiffKey) {
        if self.keyboard.borrow().pane == Some(Pane::Detail) && self.keyboard_file_selected(key) {
            let mut state = self.keyboard.borrow_mut();
            if state.focus_row {
                response.request_focus();
                state.focus_row = false;
                state.focus_requested = false;
            }
            drop(state);
            crate::a11y::a11y_selected(response, || format!("File {}", key.path()), true);
        }
        let mut state = self.keyboard.borrow_mut();
        if response.has_focus() && !state.focus_requested && !state.focus_row {
            state.pane = Some(Pane::Detail);
            state.cursor = Some((scope, key.clone()));
        }
    }

    pub(super) fn keyboard_work_files(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let groups = [
            &self.work.conflicted,
            &self.work.unstaged,
            &self.work.staged,
        ];
        let navigating = self.keyboard.borrow().enter
            || (keymap::queued(ui.ctx(), Command::FileNext)
                || keymap::queued(ui.ctx(), Command::FilePrevious)
                || self.keyboard.borrow().step != 0);
        let files: Vec<_> = if navigating && Settings::get(ui.ctx()).files_as_tree {
            groups
                .into_iter()
                .flat_map(|files| {
                    let mut nodes =
                        branch_tree::build(files.iter().map(|file| (file.path.as_str(), file)));
                    branch_tree::compact(&mut nodes);
                    let mut out = Vec::new();
                    let mut levels = vec![nodes.iter()];
                    while let Some(level) = levels.last_mut() {
                        match level.next() {
                            Some(branch_tree::Node::Leaf { item, .. }) => out.push(*item),
                            Some(branch_tree::Node::Folder(folder)) => {
                                levels.push(folder.children.iter())
                            }
                            None => {
                                levels.pop();
                            }
                        }
                    }
                    out
                })
                .collect()
        } else {
            groups.into_iter().flatten().collect()
        };
        self.keyboard_files(
            ui,
            (Id::new("working-files"), None),
            &files,
            |file| file.path.as_str(),
            |file| DiffKey::Work((*file).clone()),
            acts,
        );
    }

    /// Only key presses build a navigation order. Ordinary frames reconcile one selected
    /// identity; collapsed folders are revealed when navigation moves into them.
    pub(super) fn keyboard_files<T>(
        &self,
        ui: &mut Ui,
        (scope, folder): (Id, Option<Id>),
        files: &[T],
        path: impl Fn(&T) -> &str,
        key: impl Fn(&T) -> DiffKey,
        acts: &mut Vec<Action>,
    ) {
        self.keyboard_scope();
        let active = self.keyboard.borrow().pane == Some(Pane::Detail) && available(self, ui.ctx());
        let requested = if active {
            std::mem::take(&mut self.keyboard.borrow_mut().step)
        } else {
            0
        };
        let delta = if active && take_key(ui.ctx(), Key::J, true) {
            1
        } else if active && take_key(ui.ctx(), Key::K, true) {
            -1
        } else {
            requested
        };
        let mut state = self.keyboard.borrow_mut();
        let previous = state
            .cursor
            .as_ref()
            .filter(|(old_scope, _)| *old_scope == scope)
            .map(|(_, key)| key.clone());
        let current = previous
            .as_ref()
            .and_then(|old| {
                files
                    .iter()
                    .find(|file| path(file) == old.path() && same_file(&key(file), old))
            })
            .map(&key)
            .or_else(|| {
                // Staging may move a path to the other section; a rename may change its path.
                let DiffKey::Work(old) = previous.as_ref()? else {
                    return None;
                };
                files.iter().find_map(|file| match key(file) {
                    DiffKey::Work(new)
                        if new.path == old.path || new.old_path.as_deref() == Some(&old.path) =>
                    {
                        Some(DiffKey::Work(new))
                    }
                    _ => None,
                })
            });
        let enter = active && state.enter;
        if active {
            state.enter = false;
        }
        let selected = if delta != 0 || enter {
            let mut ordered: Vec<&T> = if folder.is_some() && Settings::get(ui.ctx()).files_as_tree
            {
                let mut nodes = branch_tree::build(files.iter().map(|file| (path(file), file)));
                branch_tree::compact(&mut nodes);
                let mut out = Vec::new();
                let mut levels = vec![nodes.iter()];
                while let Some(level) = levels.last_mut() {
                    match level.next() {
                        Some(branch_tree::Node::Leaf { item, .. }) => out.push(*item),
                        Some(branch_tree::Node::Folder(folder)) => {
                            levels.push(folder.children.iter())
                        }
                        None => {
                            levels.pop();
                        }
                    }
                }
                out
            } else {
                files.iter().collect()
            };
            let at = current.as_ref().and_then(|old| {
                ordered
                    .iter()
                    .position(|file| path(file) == old.path() && same_file(&key(file), old))
            });
            let next = match at {
                Some(i) if delta > 0 => (i + 1).min(ordered.len().saturating_sub(1)),
                Some(i) if delta < 0 => i.saturating_sub(1),
                Some(i) => i,
                None if delta < 0 => ordered.len().saturating_sub(1),
                None => 0,
            };
            ordered.get_mut(next).map(|file| key(file))
        } else {
            current
        };
        state.cursor = selected.clone().map(|key| (scope, key));
        if delta != 0 || enter {
            state.focus_row = true;
        }
        drop(state);
        let Some(selected) = selected else {
            return;
        };
        if delta != 0 || enter {
            let folder = folder.unwrap_or_else(|| {
                let tree = match &selected {
                    DiffKey::Work(f) if f.area == Area::Staged => "staged",
                    DiffKey::Work(f) if f.area == Area::Conflicted => "conflicted",
                    _ => "unstaged",
                };
                Id::new(("file-folder", self.repo_key(), tree))
            });
            let path = selected.path();
            ui.data_mut(|d| {
                for (i, _) in path.match_indices('/') {
                    d.remove::<bool>(folder.with(&path[..i]));
                }
                d.insert_temp(folder.with("keyboard-reveal"), path.to_owned());
            });
        }
        if !active
            || self.busy_op.is_some()
            || self.loading
            || self.work_pending != 0
            || self.load_error.is_some()
        {
            return;
        }
        let open = take_key(ui.ctx(), Key::Enter, false);
        if open || take_key(ui.ctx(), Key::Space, false) {
            acts.push(Action::Keyboard(Request {
                path: self.path.clone(),
                read: self.work_read,
                epoch: self.work_epoch,
                target: Target::File {
                    scope,
                    key: selected,
                    open,
                },
            }));
        }
    }

    pub(super) fn keyboard_hunk(&self, ui: &Ui, acts: &mut Vec<Action>) {
        if self.keyboard.borrow().pane != Some(Pane::Graph)
            || !available(self, ui.ctx())
            || self.loading
            || self.busy_op.is_some()
            || self.work_pending != 0
            || self.load_error.is_some()
        {
            return;
        }
        let Some(view) = &self.diff else {
            return;
        };
        let (DiffKey::Work(file), Some(Ok(data)), Some(hunk)) =
            (&view.key, &view.data, view.keyboard_hunk)
        else {
            return;
        };
        if view.data_request != view.request
            || !data.partial
            || view.structural.enabled
            || self.diff_ignore_ws
            || !matches!(file.area, Area::Staged | Area::Unstaged)
        {
            return;
        }
        let pick = if take_key(ui.ctx(), Key::S, false) && file.area == Area::Unstaged {
            Some(diff::Pick::Stage)
        } else if take_key(ui.ctx(), Key::U, false) && file.area == Area::Staged {
            Some(diff::Pick::Unstage)
        } else if take_key(ui.ctx(), Key::D, false) && file.area == Area::Unstaged {
            Some(diff::Pick::Discard)
        } else {
            None
        };
        if let Some(pick) = pick {
            let lines = diff::hunk_changes(&data.lines, hunk);
            if !lines.is_empty() {
                acts.push(Action::Keyboard(Request {
                    path: self.path.clone(),
                    read: self.work_read,
                    epoch: self.work_epoch,
                    target: Target::Hunk {
                        key: view.key.clone(),
                        request: view.request,
                        pick,
                        lines,
                        confirmed: false,
                    },
                }));
            }
        }
    }

    pub(super) fn keyboard_step(&self, step: i32) {
        self.keyboard.borrow_mut().step = step;
    }

    /// Capture a dynamic file/hunk target for palette and registry actions. Application still
    /// revalidates the captured read, epoch, file and diff request at execution/confirmation.
    pub(super) fn keyboard_command(&self, command: Command) -> Result<Action, String> {
        let missing = || "Focus the corresponding file list or working-tree diff first".to_owned();
        if matches!(command, Command::FileNext | Command::FilePrevious) {
            if self.keyboard.borrow().pane != Some(Pane::Detail) {
                return Err(missing());
            }
            return Ok(Action::KeyboardStep(if command == Command::FileNext {
                1
            } else {
                -1
            }));
        }
        if self.loading || self.work_pending != 0 || self.load_error.is_some() {
            return Err("Wait for a successful repository read before using this selection".into());
        }
        let target = if matches!(command, Command::FileOpen | Command::FileToggleStage) {
            if self.keyboard.borrow().pane != Some(Pane::Detail) {
                return Err(missing());
            }
            let (scope, key) = self.keyboard.borrow().cursor.clone().ok_or_else(missing)?;
            if command == Command::FileToggleStage
                && !matches!(&key, DiffKey::Work(f) if f.area != Area::Conflicted)
            {
                return Err(
                    "Only non-conflicted working-tree files can be staged or unstaged".into(),
                );
            }
            Target::File {
                scope,
                key,
                open: command == Command::FileOpen,
            }
        } else {
            let view = self.diff.as_ref().ok_or_else(missing)?;
            let (DiffKey::Work(file), Some(Ok(data)), Some(hunk)) =
                (&view.key, &view.data, view.keyboard_hunk)
            else {
                return Err(missing());
            };
            if view.request != view.data_request
                || !data.partial
                || self.diff_ignore_ws
                || view.structural.enabled
                || self.diff_as_file
                || self.diff_rendered
                || self.diff_rich_kind().is_some()
                || self.conflict.is_some()
                || self.rebase.is_some()
            {
                return Err("Use the current ordinary patch with whitespace changes visible to change a hunk".into());
            }
            let pick = match (command, file.area) {
                (Command::HunkStage, Area::Unstaged) => diff::Pick::Stage,
                (Command::HunkUnstage, Area::Staged) => diff::Pick::Unstage,
                (Command::HunkDiscard, Area::Unstaged) => diff::Pick::Discard,
                _ => {
                    return Err(
                        "Stage/discard needs an unstaged hunk; unstage needs a staged hunk".into(),
                    );
                }
            };
            let lines = diff::hunk_changes(&data.lines, hunk);
            if lines.is_empty() {
                return Err("This hunk has no editable changed lines".into());
            }
            Target::Hunk {
                key: view.key.clone(),
                request: view.request,
                pick,
                lines,
                confirmed: false,
            }
        };
        Ok(Action::Keyboard(Request {
            path: self.path.clone(),
            read: self.work_read,
            epoch: self.work_epoch,
            target,
        }))
    }

    pub(super) fn apply_keyboard(&mut self, request: Request, ctx: &egui::Context) {
        if self.path != request.path
            || self.work_read != request.read
            || self.work_epoch != request.epoch
            || self.busy_op.is_some()
            || self.loading
            || self.work_pending != 0
            || self.load_error.is_some()
            || self.dialog.is_some()
        {
            return;
        }
        match &request.target {
            Target::File { scope, key, open } => {
                if !self
                    .keyboard
                    .borrow()
                    .cursor
                    .as_ref()
                    .is_some_and(|(s, cursor)| s == scope && same_file(cursor, key))
                {
                    return;
                }
                if let DiffKey::Work(file) = key {
                    if !self
                        .work
                        .conflicted
                        .iter()
                        .chain(&self.work.unstaged)
                        .chain(&self.work.staged)
                        .any(|f| f == file)
                    {
                        return;
                    }
                    if !self.selected.is_some_and(|i| self.is_wip(i))
                        || self.compare.is_some()
                        || self.pr_view.is_some()
                    {
                        return;
                    }
                }
                if *open {
                    self.focus_pane(Pane::Graph, ctx);
                    if !self.diff.as_ref().is_some_and(|view| &view.key == key) {
                        self.apply(Action::OpenDiff(key.clone()), ctx);
                    }
                } else if let DiffKey::Work(file) = key {
                    let (label, steps) = match file.area {
                        Area::Staged => (
                            "Unstage",
                            vec![diff::unstage_args(&self.refs.head, &file.path)],
                        ),
                        Area::Unstaged | Area::Untracked => (
                            "Stage",
                            ops(&[&["--literal-pathspecs", "add", "--", &file.path]]),
                        ),
                        Area::Conflicted => return,
                    };
                    self.apply(
                        Action::Ops {
                            label: label.to_owned(),
                            ops: steps,
                            refresh: Refresh::Status,
                        },
                        ctx,
                    );
                }
            }
            Target::Hunk {
                key,
                request: read,
                pick,
                lines,
                confirmed,
            } => {
                let Some(view) = self
                    .diff
                    .as_ref()
                    .filter(|v| &v.key == key && v.request == *read)
                else {
                    return;
                };
                if view.data_request != view.request
                    || !matches!(&view.data, Some(Ok(d)) if d.partial)
                    || self.diff_as_file
                    || view.structural.enabled
                    || self.diff_ignore_ws
                    || self.conflict.is_some()
                    || self.rebase.is_some()
                    || (self.diff_rendered && diff::rendered::is_markdown(key.path()))
                {
                    return;
                }
                let DiffKey::Work(file) = key else {
                    return;
                };
                if !self
                    .work
                    .unstaged
                    .iter()
                    .chain(&self.work.staged)
                    .any(|current| current == file)
                    || !matches!(
                        (file.area, pick),
                        (Area::Unstaged, diff::Pick::Stage | diff::Pick::Discard)
                            | (Area::Staged, diff::Pick::Unstage)
                    )
                {
                    return;
                }
                if *pick == diff::Pick::Discard && !confirmed {
                    let mut confirmed = request.clone();
                    if let Target::Hunk {
                        confirmed: flag, ..
                    } = &mut confirmed.target
                    {
                        *flag = true;
                    }
                    self.dialog = Some(Dialog::KeyboardDiscard { request: confirmed });
                } else {
                    self.apply(
                        Action::PickLines {
                            pick: *pick,
                            lines: lines.clone(),
                        },
                        ctx,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
