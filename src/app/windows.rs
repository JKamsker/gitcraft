//! Live repository sessions in independent, deferred native windows. Moving a tab moves its
//! Box, channels and drafts; no repository is reopened and no subprocess is launched.
mod desktop;
pub use desktop::DesktopApp;
mod routing;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use egui::{Pos2, Rect, Ui, ViewportBuilder, ViewportId};
use serde::{Deserialize, Serialize};

use super::{App, Tab, palette::Palette, tab_strip::TabDrag};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct SavedWindow {
    pub open: Vec<PathBuf>,
    pub active: usize,
    pub position: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
}

/// The parts of the shell that follow one window. All other app services have one owner.
pub(super) struct Window {
    pub tabs: Vec<Tab>,
    active: usize,
    palette: Option<Palette>,
    clone_form: Option<super::clone::CloneForm>,
    practice_open: bool,
    remote_form: Option<super::remote_open::RemoteForm>,
    teleport: Option<super::teleport::TeleportForm>,
    teleport_drag: Option<super::teleport::drag::DragTargets>,
    repo_scan: Option<super::repo_scan::RepoScan>,
    my_work: super::my_work::MyWork,
    pick_on_start: bool,
    error: Option<super::AppError>,
    error_card: Option<crate::error_card::ErrorCard>,
    manual: bool,
    focus: super::focus::State,
    drag: Option<TabDrag>,
    scroll: f32,
    scrolled_to: Option<egui::Id>,
    switcher_query: String,
    switcher_focus: bool,
    switch_to: Option<PathBuf>,
    close_worktree: bool,
    decorated: bool,
    border: Option<bool>,
    parent: ViewportId,
    position: Option<Pos2>,
    place_at_pixels: Option<Pos2>,
    size: Option<egui::Vec2>,
}

impl Window {
    fn new(tabs: Vec<Tab>, parent: ViewportId, position: Option<Pos2>) -> Self {
        Self {
            tabs,
            active: 0,
            palette: None,
            clone_form: None,
            practice_open: false,
            remote_form: None,
            teleport: None,
            teleport_drag: None,
            repo_scan: None,
            my_work: Default::default(),
            pick_on_start: false,
            error: None,
            error_card: None,
            manual: false,
            focus: super::focus::State::unfocused(),
            drag: None,
            scroll: 0.0,
            scrolled_to: None,
            switcher_query: String::new(),
            switcher_focus: false,
            switch_to: None,
            close_worktree: false,
            decorated: true,
            border: None,
            parent,
            position,
            place_at_pixels: None,
            size: None,
        }
    }

    /// Exchange the complete window view, leaving shared services in place. The coordinator
    /// mutex serializes callbacks; a session can never be painted or mutated in two windows.
    fn swap(&mut self, app: &mut App) {
        std::mem::swap(&mut self.tabs, &mut app.tabs);
        std::mem::swap(&mut self.active, &mut app.active);
        std::mem::swap(&mut self.palette, &mut app.palette);
        std::mem::swap(&mut self.clone_form, &mut app.clone_form);
        std::mem::swap(&mut self.practice_open, &mut app.practice_open);
        std::mem::swap(&mut self.remote_form, &mut app.remote_form);
        std::mem::swap(&mut self.teleport, &mut app.teleport);
        std::mem::swap(&mut self.teleport_drag, &mut app.teleport_drag);
        std::mem::swap(&mut self.repo_scan, &mut app.repo_scan);
        std::mem::swap(&mut self.my_work, &mut app.my_work);
        std::mem::swap(&mut self.pick_on_start, &mut app.pick_on_start);
        std::mem::swap(&mut self.error, &mut app.error);
        std::mem::swap(&mut self.error_card, &mut app.error_card);
        std::mem::swap(&mut self.manual, &mut app.manual_open);
        std::mem::swap(&mut self.focus, &mut app.focus);
        std::mem::swap(&mut self.drag, &mut app.tab_drag);
        std::mem::swap(&mut self.scroll, &mut app.tab_scroll);
        std::mem::swap(&mut self.scrolled_to, &mut app.tab_scrolled_to);
        std::mem::swap(&mut self.switcher_query, &mut app.switcher_query);
        std::mem::swap(&mut self.switcher_focus, &mut app.switcher_focus);
        std::mem::swap(&mut self.switch_to, &mut app.switch_to);
        std::mem::swap(&mut self.close_worktree, &mut app.close_worktree);
        std::mem::swap(&mut self.decorated, &mut app.decorated);
        std::mem::swap(&mut self.border, &mut app.window_border);
    }

    fn saved(&self) -> SavedWindow {
        SavedWindow {
            open: self.tabs.iter().filter_map(repo_path).collect(),
            active: self
                .tabs
                .iter()
                .take(self.active + 1)
                .filter(|t| matches!(t, Tab::Repo(_)))
                .count()
                .saturating_sub(1),
            position: self.position.map(|p| [p.x, p.y]),
            size: self.size.map(|s| [s.x, s.y]),
        }
    }
}

fn repo_path(tab: &Tab) -> Option<PathBuf> {
    match tab {
        Tab::Repo(r) => Some(r.path.clone()),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct Windows {
    pub parked: BTreeMap<ViewportId, Window>,
    pub enabled: bool,
    pub current: ViewportId,
    serial: u64,
    strips: BTreeMap<ViewportId, Rect>,
    root_hidden: bool,
    pub focused_elsewhere: bool,
    pub quit_requested: bool,
}

impl App {
    /// Native close/Return moves every live session back, preserving operations and drafts.
    fn return_window(&mut self, ctx: egui::Context) {
        let id = self.windows.current;
        if id == ViewportId::ROOT {
            return;
        }
        for tab in &mut self.tabs {
            if let Tab::Repo(r) = tab {
                r.cancel_keyboard_ownership(&ctx);
            }
        }
        if let Some(root) = self.windows.parked.get_mut(&ViewportId::ROOT) {
            if root.tabs.len() == 1 && matches!(root.tabs.first(), Some(Tab::Home)) {
                root.tabs.clear();
            }
            root.tabs.append(&mut self.tabs);
            root.active = root.tabs.len().saturating_sub(1);
            root.drag = None;
        }
        self.windows.strips.remove(&id);
        for window in self.windows.parked.values_mut() {
            if window.parent == id {
                window.parent = ViewportId::ROOT;
            }
        }
        self.windows.root_hidden = false;
        show(&ctx, ViewportId::ROOT);
    }

    pub(super) fn root_shown(&mut self) {
        self.windows.root_hidden = false;
    }

    pub(super) fn close_root_with_children(&mut self, ctx: &egui::Context) {
        self.root_close_request(ctx, ctx.input(|i| i.viewport().close_requested()));
    }

    fn root_close_request(&mut self, ctx: &egui::Context, requested: bool) -> bool {
        if !self.windows.quit_requested
            && self.windows.current == ViewportId::ROOT
            && (self.settings.keep_running_in_tray
                || !self.windows.parked.is_empty()
                || self.window_has_dialog())
            && requested
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.window_has_dialog() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.windows.root_hidden = true;
            }
            return true;
        }
        false
    }

    pub(super) fn window_strip(&mut self, ctx: &egui::Context, strip: Rect) {
        if let Some(origin) = ctx.input(|i| i.viewport().inner_rect.map(|r| r.min)) {
            self.windows.strips.insert(
                self.windows.current,
                strip.translate(origin.to_vec2()) * ctx.pixels_per_point(),
            );
        }
    }

    /// Source viewport's captured mouse release uses desktop coordinates to select another
    /// strip. A drop outside all strips creates a window; in-strip drops keep normal reorder.
    pub(super) fn drop_window_target(
        &self,
        from: usize,
        strip: Rect,
        ctx: &egui::Context,
    ) -> Option<(Option<ViewportId>, Pos2)> {
        let pointer = ctx.pointer_interact_pos()?;
        if !matches!(self.tabs.get(from), Some(Tab::Repo(_))) {
            return None;
        }
        if strip.expand(20.0).contains(pointer) || !self.windows.enabled || ctx.embed_viewports() {
            return None;
        }
        let origin = ctx.input(|i| i.viewport().inner_rect.map(|r| r.min))?;
        let desktop = (pointer + origin.to_vec2()) * ctx.pixels_per_point();
        let target = drop_target(self.windows.current, desktop, &self.windows.strips);
        Some((target, desktop))
    }
}

fn show(ctx: &egui::Context, id: ViewportId) {
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
    ctx.request_repaint_of(id);
}

fn bounded_size(size: egui::Vec2, monitor: Option<egui::Vec2>) -> egui::Vec2 {
    let limit = monitor
        .filter(|s| s.is_finite() && s.x > 128.0 && s.y > 128.0)
        .map(|s| s - egui::vec2(64.0, 64.0))
        .unwrap_or(egui::vec2(1920.0, 1080.0));
    let minimum = egui::vec2(360.0, 240.0).min(limit);
    if size.is_finite() {
        size.clamp(minimum, limit)
    } else {
        egui::vec2(1200.0, 800.0).clamp(minimum, limit)
    }
}

fn drop_target(
    source: ViewportId,
    point: Pos2,
    strips: &BTreeMap<ViewportId, Rect>,
) -> Option<ViewportId> {
    strips
        .iter()
        .find(|(id, rect)| **id != source && rect.contains(point))
        .map(|(&id, _)| id)
}

#[cfg(test)]
#[path = "windows/tests.rs"]
mod tests;
