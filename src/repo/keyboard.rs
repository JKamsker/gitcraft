//! Pane-local navigation. Requests carry the identity of the displayed file/status/diff;
//! applying one still goes through the ordinary action and operation workers.

#[path = "core_seams/keyboard_commands.rs"]
mod keyboard_commands;

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
        if self.find.open
            && self.find_in().is_some()
            && self.main_keys(ctx, super::popout::Panel::Diff)
        {
            scopes.push(Scope::Find);
        }
        if available(self, ctx) {
            if self.find_in().is_some() && self.main_keys(ctx, super::popout::Panel::Diff) {
                scopes.push(Scope::Selection);
            }
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
                Pane::Sidebar => crate::i18n::text("repo-core-2847"),
                Pane::Graph => crate::i18n::text("repo-core-2848"),
                Pane::Detail => crate::i18n::text("repo-core-2849"),
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
            crate::a11y::a11y_selected(
                response,
                || {
                    crate::i18n::message(
                        "repo-core-2850",
                        &[("path", key.path().to_string().into())],
                    )
                },
                true,
            );
        }
        let mut state = self.keyboard.borrow_mut();
        if response.has_focus() && !state.focus_requested && !state.focus_row {
            state.pane = Some(Pane::Detail);
            state.cursor = Some((scope, key.clone()));
        }
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
                            crate::i18n::text("repo-core-0844"),
                            vec![diff::unstage_args(&self.refs.head, &file.path)],
                        ),
                        Area::Unstaged | Area::Untracked => (
                            crate::i18n::text("repo-core-0836"),
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

#[cfg(test)]
#[path = "direct_tests/keyboard_menu_registry_tests.rs"]
mod menu_registry_tests;
