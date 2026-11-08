//! Live repository sessions in independent, deferred native windows. Moving a tab moves its
//! Box, channels and drafts; no repository is reopened and no subprocess is launched.
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
    remote_form: Option<super::remote_open::RemoteForm>,
    teleport: Option<super::teleport::TeleportForm>,
    teleport_drag: Option<super::teleport::drag::DragTargets>,
    repo_scan: Option<super::repo_scan::RepoScan>,
    my_work: super::my_work::MyWork,
    pick_on_start: bool,
    error: Option<String>,
    error_card: Option<crate::error_card::ErrorCard>,
    manual: bool,
    was_focused: bool,
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
            remote_form: None,
            teleport: None,
            teleport_drag: None,
            repo_scan: None,
            my_work: Default::default(),
            pick_on_start: false,
            error: None,
            error_card: None,
            manual: false,
            was_focused: false,
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
        std::mem::swap(&mut self.remote_form, &mut app.remote_form);
        std::mem::swap(&mut self.teleport, &mut app.teleport);
        std::mem::swap(&mut self.teleport_drag, &mut app.teleport_drag);
        std::mem::swap(&mut self.repo_scan, &mut app.repo_scan);
        std::mem::swap(&mut self.my_work, &mut app.my_work);
        std::mem::swap(&mut self.pick_on_start, &mut app.pick_on_start);
        std::mem::swap(&mut self.error, &mut app.error);
        std::mem::swap(&mut self.error_card, &mut app.error_card);
        std::mem::swap(&mut self.manual, &mut app.manual_open);
        std::mem::swap(&mut self.was_focused, &mut app.was_focused);
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

/// eframe's root owns this coordinator. Deferred callbacks keep Weak references so closing
/// the application releases sessions, terminals, watchers and callback captures normally.
#[derive(Clone, Default)]
struct Children {
    full: Vec<(ViewportId, ViewportBuilder)>,
    panels: Vec<(ViewportId, ViewportBuilder)>,
}
type WindowCache = Arc<Mutex<BTreeMap<ViewportId, Children>>>;

pub struct DesktopApp {
    app: Arc<Mutex<App>>,
    cache: WindowCache,
    deferred_close: bool,
}

impl DesktopApp {
    pub fn new(mut app: App) -> Self {
        app.windows.enabled = true;
        Self {
            app: Arc::new(Mutex::new(app)),
            cache: Default::default(),
            deferred_close: false,
        }
    }

    fn children(owner: &Arc<Mutex<App>>, cache: &WindowCache, ctx: &egui::Context) {
        if ctx.embed_viewports() {
            return;
        }
        let (specs, paused) = if let Ok(app) = owner.try_lock() {
            let full = app
                .windows
                .parked
                .iter()
                .filter(|(_, w)| w.parent == ctx.viewport_id())
                .map(|(&id, w)| {
                    let title = w
                        .tabs
                        .get(w.active)
                        .and_then(repo_path)
                        .map(|p| format!("{} — gitcraft", p.display()))
                        .unwrap_or_else(|| "gitcraft".into());
                    let size = bounded_size(
                        w.size.unwrap_or(egui::vec2(1200.0, 800.0)),
                        ctx.input(|i| i.viewport().monitor_size),
                    );
                    let mut b = super::window_viewport()
                        .with_title(title)
                        .with_app_id("gitcraft")
                        .with_icon(super::window_icon())
                        .with_inner_size(size)
                        .with_min_inner_size(size.min(egui::vec2(800.0, 500.0)));
                    if let Some(position) = w.position {
                        b = b.with_position(position);
                    }
                    (id, b)
                })
                .collect::<Vec<_>>();
            let tabs = if app.windows.current == ctx.viewport_id() {
                Some(&app.tabs)
            } else {
                app.windows.parked.get(&ctx.viewport_id()).map(|w| &w.tabs)
            };
            let panels = tabs
                .into_iter()
                .flatten()
                .filter_map(|tab| match tab {
                    Tab::Repo(repo) => Some(repo.popout_window_specs()),
                    _ => None,
                })
                .flatten()
                .collect();
            if let Ok(mut cached) = cache.lock() {
                cached.retain(|id, _| {
                    *id == ViewportId::ROOT
                        || *id == app.windows.current
                        || app.windows.parked.contains_key(id)
                });
            }
            (Children { full, panels }, false)
        } else {
            // A native picker can run a nested AppKit event loop. Reentrant callbacks
            // must neither wait for their own coordinator nor prune existing windows.
            (
                cache
                    .lock()
                    .ok()
                    .and_then(|c| c.get(&ctx.viewport_id()).cloned())
                    .unwrap_or_default(),
                true,
            )
        };
        if let Ok(mut cached) = cache.lock() {
            cached.insert(ctx.viewport_id(), specs.clone());
        }
        for (id, builder) in specs.full {
            let weak = Arc::downgrade(owner);
            let cache = cache.clone();
            ctx.show_viewport_deferred(id, builder, move |ui, _| {
                Self::child(&weak, &cache, id, ui)
            });
        }
        if paused {
            // A nested native picker cannot borrow repository sessions. Redeclare the
            // existing immediate panels as well, so egui retains their native windows.
            for (id, builder) in specs.panels {
                ctx.show_viewport_immediate(id, builder, |_, _| {});
            }
        }
    }

    fn child(weak: &Weak<Mutex<App>>, cache: &WindowCache, id: ViewportId, ui: &mut Ui) {
        let Some(owner) = weak.upgrade() else { return };
        let ctx = ui.ctx().clone();
        {
            let Ok(mut app) = owner.try_lock() else {
                Self::children(&owner, cache, &ctx);
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            };
            let Some(mut window) = app.windows.parked.remove(&id) else {
                return;
            };
            if let Some(pixels) = window.place_at_pixels.take() {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                    pixels / ctx.pixels_per_point(),
                ));
            }
            window.swap(&mut app);
            // Keep the root's sessions visible to persistence, handoff and progress reducers
            // during a child callback. Current sessions live only in app.tabs.
            app.windows.parked.insert(ViewportId::ROOT, window);
            app.windows.current = id;
            let profile = app.settings.active_profile;
            if ctx.input(|i| i.viewport().close_requested()) && !app.window_has_dialog() {
                app.return_window(ctx.clone());
            } else {
                if ctx.input(|i| i.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
                app.window_ui(ui, None);
            }
            let root = app.windows.parked.remove(&ViewportId::ROOT);
            if let Some(mut root) = root {
                root.swap(&mut app);
                if app.settings.active_profile != profile {
                    app.tabs = std::mem::take(&mut root.tabs);
                    app.active = root.active;
                    app.palette = None;
                    app.manual_open = false;
                    show(&ctx, ViewportId::ROOT);
                } else if !root.tabs.is_empty() {
                    if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
                        root.position = Some(rect.min);
                        root.size = Some(rect.size());
                    }
                    app.windows.parked.insert(id, root);
                }
            }
            app.windows.current = ViewportId::ROOT;
        }
        // Never hold the coordinator across show_viewport_deferred: embedded backends may
        // call it synchronously. Native callbacks remain independent of root paint passes.
        Self::children(&owner, cache, &ctx);
        // Workers may have retained the original viewport's repaint target. This bounded
        // hidden/minimized-safe tick drains their completion channels without polling Git.
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn logic_tick(&mut self, ctx: &egui::Context, frame: Option<&eframe::Frame>) {
        if let Ok(mut app) = self.app.try_lock() {
            if std::mem::take(&mut self.deferred_close) && !app.root_close_request(ctx, true) {
                // The request was delayed only while a native picker borrowed App.
                // Explicit Quit and an ordinary last-window close still complete.
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            app.tick_model(ctx, frame);
        } else {
            Self::defer_close(ctx, &mut self.deferred_close);
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn defer_close(ctx: &egui::Context, pending: &mut bool) {
        if ctx.viewport_id() == ViewportId::ROOT && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            *pending = true;
        }
    }

    fn ui_tick(&mut self, ui: &mut Ui, frame: Option<&mut eframe::Frame>) {
        {
            let Ok(mut app) = self.app.try_lock() else {
                Self::defer_close(ui.ctx(), &mut self.deferred_close);
                Self::children(&self.app, &self.cache, ui.ctx());
                ui.ctx().request_repaint_after(Duration::from_millis(50));
                return;
            };
            if ui.ctx().embed_viewports() {
                for (_, mut window) in std::mem::take(&mut app.windows.parked) {
                    app.tabs.append(&mut window.tabs);
                }
            }
            app.window_ui(ui, frame);
        }
        Self::children(&self.app, &self.cache, ui.ctx());
    }
}

impl eframe::App for DesktopApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.logic_tick(ctx, Some(frame));
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        self.ui_tick(ui, Some(frame));
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(mut app) = self.app.lock() {
            eframe::App::save(&mut *app, storage);
        }
    }
    fn on_exit(&mut self) {
        if let Ok(mut app) = self.app.lock() {
            eframe::App::on_exit(&mut *app);
        }
    }
}

impl App {
    /// Run a model completion against its initiating window, regardless of which
    /// viewport polled it. A closed owner falls back to the root, never a repainting peer.
    pub(super) fn in_window<R>(
        &mut self,
        requested: ViewportId,
        work: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.windows.current;
        let owner = if requested == previous || self.windows.parked.contains_key(&requested) {
            requested
        } else {
            ViewportId::ROOT
        };
        if owner == previous {
            return work(self);
        }
        let Some(mut view) = self.windows.parked.remove(&owner) else {
            return work(self);
        };
        view.swap(self);
        self.windows.parked.insert(previous, view);
        self.windows.current = owner;
        let result = work(self);
        if let Some(mut view) = self.windows.parked.remove(&previous) {
            view.swap(self);
            self.windows.parked.insert(owner, view);
        }
        self.windows.current = previous;
        result
    }

    pub(super) fn window_has_dialog(&self) -> bool {
        self.clone_form.is_some()
            || self.remote_form.is_some()
            || self.teleport.is_some()
            || self.repo_scan.is_some()
            || self.my_work.open
            || self.askpass_open()
    }

    pub(super) fn credentials_for(&mut self, path: &Path, credentials: crate::git::Credentials) {
        if let Some(repo) = self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
            .find_map(|tab| match tab {
                Tab::Repo(repo) if repo.path == path => Some(repo),
                _ => None,
            })
        {
            repo.set_credentials(credentials);
        }
    }

    /// Poll window-owned continuation state even if none of its UI is painted.
    pub(super) fn poll_window_models(&mut self, ctx: &egui::Context) {
        let owners = std::iter::once(self.windows.current)
            .chain(self.windows.parked.keys().copied())
            .collect::<Vec<_>>();
        for owner in owners {
            self.in_window(owner, |app| {
                app.poll_teleport(ctx);
                app.poll_repo_scan();
                app.poll_my_work(ctx);
            });
        }
    }

    pub(super) fn restore_windows(&mut self, windows: Vec<SavedWindow>, ctx: &egui::Context) {
        let mut known = self
            .all_repo_paths()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        for saved in windows {
            let mut kept = Vec::new();
            let mut tabs = Vec::new();
            for path in saved.open {
                let keep = super::tabs::reopenable(&path) && known.insert(path.clone());
                kept.push(keep);
                if keep {
                    tabs.push(Tab::Repo(Box::new(crate::repo::RepoTab::new_deferred(
                        path, ctx,
                    ))));
                }
            }
            if tabs.is_empty() {
                continue;
            }
            let id = self.next_window_id();
            let mut window = Window::new(
                tabs,
                ViewportId::ROOT,
                // The saved monitor may have been disconnected. Let the compositor
                // place restored windows on an available monitor, rather than reuse
                // an unverifiable absolute origin from a different scale/layout.
                None,
            );
            window.active =
                super::active_among_kept(&kept, saved.active).min(window.tabs.len() - 1);
            window.size = saved
                .size
                .map(egui::Vec2::from)
                .filter(|s| s.is_finite() && s.x >= 800.0 && s.y >= 500.0);
            self.windows.parked.insert(id, window);
        }
    }

    fn next_window_id(&mut self) -> ViewportId {
        self.windows.serial += 1;
        ViewportId::from_hash_of(("repository-window", self.windows.serial))
    }

    pub(super) fn saved_windows(&self) -> Vec<SavedWindow> {
        self.windows
            .parked
            .values()
            .map(Window::saved)
            .filter(|w| !w.open.is_empty())
            .collect()
    }

    pub(super) fn all_repo_paths(&self) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .chain(self.windows.parked.values().flat_map(|w| &w.tabs))
            .filter_map(repo_path)
            .collect()
    }

    pub(super) fn focus_elsewhere(&mut self, path: &Path, ctx: &egui::Context) -> bool {
        for (&id, w) in &mut self.windows.parked {
            if let Some(i) = w
                .tabs
                .iter()
                .position(|t| repo_path(t).as_deref() == Some(path))
            {
                w.active = i;
                self.windows.focused_elsewhere = true;
                show(ctx, id);
                return true;
            }
        }
        false
    }

    pub(super) fn window_targets(&self) -> Vec<(ViewportId, String)> {
        let mut targets = self
            .windows
            .parked
            .iter()
            .map(|(&id, w)| {
                let label = if id == ViewportId::ROOT {
                    crate::i18n::text("interaction-tabs-main-window").into()
                } else {
                    w.tabs
                        .get(w.active)
                        .and_then(repo_path)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| crate::i18n::text("interaction-tabs-new-tab").into())
                };
                (id, label)
            })
            .collect::<Vec<_>>();
        if self.windows.current != ViewportId::ROOT
            && !targets.iter().any(|(id, _)| *id == ViewportId::ROOT)
        {
            targets.push((
                ViewportId::ROOT,
                crate::i18n::text("interaction-tabs-main-window").into(),
            ));
        }
        targets
    }

    /// Moves exactly the captured session into an existing window or a newly allocated one.
    pub(super) fn move_to_window(
        &mut self,
        index: usize,
        target: Option<ViewportId>,
        position: Option<Pos2>,
        ctx: &egui::Context,
    ) {
        if !self.windows.enabled
            || ctx.embed_viewports()
            || !matches!(self.tabs.get(index), Some(Tab::Repo(_)))
        {
            return;
        }
        if target == Some(self.windows.current) {
            return;
        }
        if let Some(id) = target
            && !self.windows.parked.contains_key(&id)
        {
            return;
        }
        let mut tab = self.tabs.remove(index);
        self.tab_drag = None;
        self.teleport_drag = None;
        self.palette = None;
        // Invalidate any prepared keyboard mutation across ownership transfer, even if the
        // same path/hunk still appears in the new window.
        if let Tab::Repo(r) = &mut tab {
            r.cancel_keyboard_ownership(ctx);
        }
        let id = match target {
            Some(id) => {
                if let Some(window) = self.windows.parked.get_mut(&id) {
                    if window.tabs.len() == 1 && matches!(window.tabs.first(), Some(Tab::Home)) {
                        window.tabs.clear();
                    }
                    window.tabs.push(tab);
                    window.active = window.tabs.len() - 1;
                }
                id
            }
            None => {
                let id = self.next_window_id();
                let mut window = Window::new(vec![tab], self.windows.current, None);
                window.place_at_pixels = position;
                self.windows.parked.insert(id, window);
                id
            }
        };
        if self.tabs.is_empty() {
            self.tabs.push(Tab::Home);
        }
        self.active = self
            .active
            .saturating_sub(usize::from(self.active >= index))
            .min(self.tabs.len() - 1);
        show(ctx, id);
        ctx.request_repaint();
    }

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
mod tests {
    use super::*;
    fn harness() -> egui_kittest::Harness<'static, DesktopApp> {
        egui_kittest::Harness::builder()
            .with_size([1200.0, 800.0])
            .build_eframe(|cc| {
                let mut app =
                    App::launched_using(cc, None, || Ok("git version 2.43.0 (recorded)".into()));
                app.settings.check_updates = false;
                app.settings.notifications = false;
                app.settings.set(&cc.egui_ctx);
                DesktopApp::new(app)
            })
    }

    // build_eframe runs logic/ui during harness construction. These direct adapter tests
    // need only recorded App state: never enter tick_model or start background watchers.
    fn recorded_harness() -> egui_kittest::Harness<'static, Option<DesktopApp>> {
        egui_kittest::Harness::builder()
            .with_size([1200.0, 800.0])
            .build_ui_state(
                |ui, state: &mut Option<DesktopApp>| {
                    if state.is_none() {
                        let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                        let mut app = App::launched_using(&cc, None, || {
                            Ok("git version 2.43.0 (recorded)".into())
                        });
                        app.settings.check_updates = false;
                        app.settings.notifications = false;
                        app.settings.set(ui.ctx());
                        *state = Some(DesktopApp::new(app));
                    }
                },
                None,
            )
    }

    fn deferred(path: &str, ctx: &egui::Context) -> Tab {
        Tab::Repo(Box::new(crate::repo::RepoTab::new_deferred(
            path.into(),
            ctx,
        )))
    }

    fn identity(tab: &Tab) -> Option<usize> {
        match tab {
            Tab::Repo(r) => Some(r.as_ref() as *const crate::repo::RepoTab as usize),
            _ => None,
        }
    }

    #[test]
    fn transfers_keep_one_live_box_and_preserve_selected_other_tabs() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![
            deferred("/virtual/alpha", &ctx),
            deferred("/virtual/beta", &ctx),
            deferred("/virtual/gamma", &ctx),
        ];
        app.active = 2;
        let alpha = identity(&app.tabs[0]);
        let gamma = identity(&app.tabs[2]);
        if let Tab::Repo(r) = &mut app.tabs[0] {
            r.finished
                .push(("completion queued before detaching".into(), false));
        }
        app.move_to_window(0, None, Some(egui::pos2(-1000.0, 60.0)), &ctx);
        let first = *app.windows.parked.keys().next().ok_or("no window")?;
        assert_eq!(identity(&app.tabs[app.active]), gamma);
        let w = app.windows.parked.get(&first).ok_or("missing window")?;
        assert_eq!(identity(&w.tabs[0]), alpha);
        assert!(
            matches!(&w.tabs[0], Tab::Repo(r) if r.finished[0].0 == "completion queued before detaching")
        );
        app.move_to_window(0, Some(first), None, &ctx);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.windows.parked[&first].tabs.len(), 2);
        assert_eq!(app.all_repo_paths().len(), 3);
        // A stale closed-window target must not remove the source tab.
        app.move_to_window(0, Some(ViewportId::from_hash_of("gone")), None, &ctx);
        assert_eq!(identity(&app.tabs[0]), gamma);
        let mut w = app.windows.parked.remove(&first).ok_or("missing window")?;
        w.swap(&mut app);
        app.windows.parked.insert(ViewportId::ROOT, w);
        app.windows.current = first;
        app.return_window(ctx.clone());
        assert!(app.tabs.is_empty());
        let root = &app.windows.parked[&ViewportId::ROOT];
        assert_eq!(root.tabs.len(), 3);
        assert_eq!(identity(&root.tabs[1]), alpha);
        assert_eq!(
            root.tabs
                .iter()
                .filter_map(identity)
                .filter(|id| Some(*id) == alpha)
                .count(),
            1
        );
        Ok(())
    }

    #[test]
    fn saved_windows_do_not_duplicate_main_tabs_and_old_state_defaults_to_no_windows()
    -> Result<(), String> {
        use eframe::App as _;
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![
            deferred("/virtual/alpha", &ctx),
            deferred("/virtual/beta", &ctx),
        ];
        app.move_to_window(1, None, Some(egui::pos2(-600.0, 10.0)), &ctx);
        #[derive(Default)]
        struct Storage(std::collections::HashMap<String, String>);
        impl eframe::Storage for Storage {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.into(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }
        let mut storage = Storage::default();
        app.save(&mut storage);
        let saved: super::super::Saved =
            eframe::get_value(&storage, eframe::APP_KEY).ok_or("no saved state")?;
        assert_eq!(saved.open, [PathBuf::from("/virtual/alpha")]);
        assert_eq!(saved.windows.len(), 1);
        assert_eq!(saved.windows[0].open, [PathBuf::from("/virtual/beta")]);
        assert_eq!(saved.windows[0].position, None);
        let old: super::super::Saved =
            ron::from_str("(open:[],active:0,recent:[])").map_err(|e| e.to_string())?;
        assert!(old.windows.is_empty());
        Ok(())
    }

    #[test]
    fn deferred_child_accepts_keys_and_closes_without_another_root_paint() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let owner = h.state().app.clone();
        let id = ViewportId::from_hash_of("independent-child");
        {
            let mut app = owner.lock().map_err(|e| e.to_string())?;
            app.tabs = vec![deferred("/virtual/root-session", &ctx)];
            app.windows
                .parked
                .insert(id, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
        }
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            DesktopApp::children(&owner, &h.state().cache, ui.ctx())
        });
        let cb = output
            .viewport_output
            .get(&id)
            .and_then(|v| v.viewport_ui_cb.clone())
            .ok_or("no deferred callback")?;
        assert!(output.viewport_output[&id].class == egui::ViewportClass::Deferred);
        output.drop_without_applying_deltas();
        let mut raw = egui::RawInput {
            viewport_id: id,
            ..Default::default()
        };
        raw.viewports.entry(ViewportId::ROOT).or_default().minimized = Some(true);
        raw.viewports.entry(id).or_default().focused = Some(true);
        raw.events.push(egui::Event::Key {
            key: egui::Key::T,
            physical_key: Some(egui::Key::T),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
        let output = ctx.run_ui(raw, |ui| cb(ui));
        assert!(!output.shapes.is_empty());
        output.drop_without_applying_deltas();
        {
            let app = owner.lock().map_err(|e| e.to_string())?;
            assert_eq!(app.tabs.len(), 1, "root must not execute child's New Tab");
            assert_eq!(
                app.windows.parked[&id].tabs.len(),
                2,
                "child must execute New Tab while root sleeps"
            );
            assert!(
                matches!(&app.tabs[0], Tab::Repo(r) if r.path == Path::new("/virtual/root-session"))
            );
        }
        let mut raw = egui::RawInput {
            viewport_id: id,
            ..Default::default()
        };
        raw.viewports
            .entry(id)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run_ui(raw, |ui| cb(ui));
        output.drop_without_applying_deltas();
        let app = owner.lock().map_err(|e| e.to_string())?;
        assert!(!app.windows.parked.contains_key(&id));
        assert_eq!(
            app.all_repo_paths(),
            [PathBuf::from("/virtual/root-session")]
        );
        Ok(())
    }

    #[test]
    fn hidden_logic_drains_each_windows_completions_once_without_a_ui_pass() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![deferred("/virtual/root", &ctx)];
        let id = ViewportId::from_hash_of("hidden-child");
        app.windows.parked.insert(
            id,
            Window::new(
                vec![deferred("/virtual/child", &ctx)],
                ViewportId::ROOT,
                None,
            ),
        );
        if let Tab::Repo(r) = &mut app.tabs[0] {
            r.finished.push(("root completion".into(), false));
        }
        if let Tab::Repo(r) = &mut app.windows.parked.get_mut(&id).ok_or("no child")?.tabs[0] {
            r.finished.push(("child completion".into(), true));
        }
        let before = app.toasts.len();
        let passes = ctx.cumulative_pass_nr();
        let focus = ctx.memory(|m| m.focused());
        let mut raw = egui::RawInput::default();
        raw.viewports.entry(ViewportId::ROOT).or_default().minimized = Some(true);
        let _ = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
        assert_eq!(app.toasts.len(), before + 2);
        let _ = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
        assert_eq!(app.toasts.len(), before + 2);
        assert_eq!(ctx.cumulative_pass_nr(), passes);
        assert_eq!(ctx.memory(|m| m.focused()), focus);
        assert_eq!(app.all_repo_paths().len(), 2);
        Ok(())
    }

    #[test]
    fn captured_release_after_source_exit_detaches_the_live_tab() -> Result<(), String> {
        use egui::{Event, Modifiers, PointerButton};
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![
            deferred("/virtual/dragged", &ctx),
            deferred("/virtual/stays", &ctx),
        ];
        let owned = identity(&app.tabs[0]);
        let mut time = 1.0;
        let mut frame = |app: &mut App, events| {
            time += 0.1;
            let mut raw = egui::RawInput {
                time: Some(time),
                events,
                ..Default::default()
            };
            raw.viewports
                .entry(ViewportId::ROOT)
                .or_default()
                .inner_rect = Some(Rect::from_min_size(
                egui::pos2(100.0, 200.0),
                egui::vec2(1200.0, 800.0),
            ));
            ctx.run_ui(raw, |ui| {
                ui.set_min_size(egui::vec2(1200.0, 800.0));
                app.tab_strip(ui);
            })
            .drop_without_applying_deltas();
        };
        frame(&mut app, vec![]);
        let press = egui::pos2(40.0, 20.0);
        frame(
            &mut app,
            vec![
                Event::PointerMoved(press),
                Event::PointerButton {
                    pos: press,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        let outside = egui::pos2(160.0, 140.0);
        frame(&mut app, vec![Event::PointerMoved(outside)]);
        assert!(app.tab_drag.is_some());
        frame(&mut app, vec![Event::PointerGone]);
        for _ in 0..3 {
            frame(&mut app, vec![]);
            assert!(
                app.tab_drag.is_some(),
                "leaving the source must retain the captured drag"
            );
            assert_eq!(identity(&app.tabs[0]), owned);
        }
        // Recorded backend output: CursorMoved at the native captured mouse-up location
        // MUST precede MouseInput Released, even though no ordinary motion arrived after exit.
        let released = egui::pos2(-240.0, 140.0);
        frame(
            &mut app,
            vec![
                Event::PointerMoved(released),
                Event::PointerButton {
                    pos: released,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(app.tab_drag.is_none());
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.windows.parked.len(), 1);
        let window = app
            .windows
            .parked
            .values()
            .next()
            .ok_or("drag did not detach")?;
        assert_eq!(identity(&window.tabs[0]), owned);
        assert_eq!(window.place_at_pixels, Some(egui::pos2(-140.0, 340.0)));
        Ok(())
    }

    #[test]
    fn captured_release_after_source_exit_targets_an_existing_window() -> Result<(), String> {
        use egui::{Event, Modifiers, PointerButton};
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![
            deferred("/virtual/dragged", &ctx),
            deferred("/virtual/stays", &ctx),
        ];
        let peer = ViewportId::from_hash_of("existing-drop-target");
        app.windows.parked.insert(
            peer,
            Window::new(
                vec![deferred("/virtual/peer", &ctx)],
                ViewportId::ROOT,
                None,
            ),
        );
        app.windows.strips.insert(
            peer,
            Rect::from_min_size(egui::pos2(-200.0, 320.0), egui::vec2(100.0, 40.0)),
        );
        let owned = identity(&app.tabs[0]);
        let mut time = 1.0;
        let mut frame = |app: &mut App, events| {
            time += 0.1;
            let mut raw = egui::RawInput {
                time: Some(time),
                events,
                ..Default::default()
            };
            raw.viewports
                .entry(ViewportId::ROOT)
                .or_default()
                .inner_rect = Some(Rect::from_min_size(
                egui::pos2(100.0, 200.0),
                egui::vec2(1200.0, 800.0),
            ));
            ctx.run_ui(raw, |ui| {
                ui.set_min_size(egui::vec2(1200.0, 800.0));
                app.tab_strip(ui);
            })
            .drop_without_applying_deltas();
        };
        frame(&mut app, vec![]);
        let press = egui::pos2(40.0, 20.0);
        frame(
            &mut app,
            vec![
                Event::PointerMoved(press),
                Event::PointerButton {
                    pos: press,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        let outside = egui::pos2(160.0, 140.0);
        frame(&mut app, vec![Event::PointerMoved(outside)]);
        assert!(app.tab_drag.is_some());
        frame(&mut app, vec![Event::PointerGone]);
        for _ in 0..3 {
            frame(&mut app, vec![]);
            assert!(
                app.tab_drag.is_some(),
                "leaving the source must retain the captured drag"
            );
            assert_eq!(identity(&app.tabs[0]), owned);
        }
        // Recorded backend output: CursorMoved at the native captured mouse-up location
        // MUST precede MouseInput Released, even though no ordinary motion arrived after exit.
        let released = egui::pos2(-240.0, 140.0);
        frame(
            &mut app,
            vec![
                Event::PointerMoved(released),
                Event::PointerButton {
                    pos: released,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(app.tab_drag.is_none());
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.windows.parked.len(), 1);
        let window = app
            .windows
            .parked
            .values()
            .next()
            .ok_or("drag did not detach")?;
        assert!(app.windows.parked.contains_key(&peer));
        assert_eq!(window.tabs.len(), 2);
        assert_eq!(identity(&window.tabs[window.active]), owned);
        assert_eq!(app.all_repo_paths().len(), 3);
        Ok(())
    }

    #[test]
    fn reentrant_native_callbacks_keep_window_registrations_without_locking_again()
    -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let owner = h.state().app.clone();
        let cache = h.state().cache.clone();
        let id = ViewportId::from_hash_of("reentrant-child");
        let panel = crate::repo::popout::viewport_id(
            Path::new("/virtual/panel-owner"),
            crate::repo::popout::Panel::Diff,
        );
        {
            let mut app = owner.lock().map_err(|e| e.to_string())?;
            app.tabs = vec![deferred("/virtual/panel-owner", &ctx)];
            if let Tab::Repo(repo) = &mut app.tabs[0] {
                repo.set_test_popout(crate::repo::popout::Panel::Diff);
            }
        }
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorded = seen.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            recorded.borrow_mut().push(viewport.ids.this);
            let id = viewport.ids.this;
            let mut raw = egui::RawInput {
                viewport_id: id,
                ..Default::default()
            };
            raw.viewports.entry(id).or_default();
            ctx.run_ui(raw, |ui| (viewport.viewport_ui_cb)(ui))
                .drop_without_applying_deltas();
        });
        owner
            .lock()
            .map_err(|e| e.to_string())?
            .windows
            .parked
            .insert(id, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx()
                .show_viewport_immediate(panel, ViewportBuilder::default(), |_, _| {});
            DesktopApp::children(&owner, &cache, ui.ctx())
        });
        let cb = out
            .viewport_output
            .get(&id)
            .and_then(|v| v.viewport_ui_cb.clone())
            .ok_or("no callback")?;
        out.drop_without_applying_deltas();
        let app = owner.lock().map_err(|e| e.to_string())?;
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            DesktopApp::children(&owner, &cache, ui.ctx())
        });
        assert!(
            out.viewport_output.contains_key(&id),
            "a nested native event loop must not prune the live child"
        );
        assert!(
            out.viewport_output.contains_key(&panel),
            "the immediate panel must survive the same reentrant pass"
        );
        assert_eq!(seen.borrow().iter().filter(|id| **id == panel).count(), 2);
        out.drop_without_applying_deltas();
        let mut raw = egui::RawInput {
            viewport_id: id,
            ..Default::default()
        };
        raw.viewports.entry(id).or_default();
        let out = ctx.run_ui(raw, |ui| cb(ui));
        out.drop_without_applying_deltas();
        assert_eq!(app.windows.parked[&id].tabs.len(), 1);
        Ok(())
    }

    #[test]
    fn restore_deduplicates_sessions_and_preserves_selection_after_missing_paths()
    -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![deferred("ssh://fixture/root", &ctx)];
        app.restore_windows(
            vec![
                SavedWindow {
                    open: vec![
                        "/gitcraft/nonexistent-window-path".into(),
                        "ssh://fixture/first".into(),
                        "ssh://fixture/second".into(),
                    ],
                    active: 1,
                    position: Some([1_000_000.0, -1_000_000.0]),
                    size: Some([1.0, 0.0]),
                },
                SavedWindow {
                    open: vec![
                        "ssh://fixture/first".into(),
                        "ssh://fixture/first".into(),
                        "ssh://fixture/root".into(),
                    ],
                    ..Default::default()
                },
            ],
            &ctx,
        );
        assert_eq!(app.windows.parked.len(), 1);
        assert_eq!(app.all_repo_paths().len(), 3);
        let window = app
            .windows
            .parked
            .values()
            .next()
            .ok_or("no restored window")?;
        assert_eq!(
            repo_path(&window.tabs[window.active]),
            Some(PathBuf::from("ssh://fixture/first"))
        );
        assert_eq!(window.position, None);
        assert_eq!(window.size, None);
        Ok(())
    }

    #[test]
    fn desktop_drop_only_targets_another_windows_strip() {
        let a = ViewportId::ROOT;
        let b = ViewportId::from_hash_of("second");
        let strips = BTreeMap::from([
            (
                a,
                Rect::from_min_size(egui::pos2(-800.0, 10.0), egui::vec2(800.0, 40.0)),
            ),
            (
                b,
                Rect::from_min_size(egui::pos2(40.0, 100.0), egui::vec2(900.0, 40.0)),
            ),
        ]);
        assert_eq!(drop_target(a, egui::pos2(100.0, 120.0), &strips), Some(b));
        assert_eq!(drop_target(a, egui::pos2(100.0, 300.0), &strips), None);
        assert_eq!(drop_target(a, egui::pos2(-100.0, 20.0), &strips), None);
    }

    #[test]
    fn resident_missing_git_still_accepts_quit_handoff_and_keeps_git_actions_guarded()
    -> Result<(), String> {
        let h = recorded_harness();
        let ctx = h.ctx.clone();
        let mut app = h
            .state()
            .as_ref()
            .ok_or("no recorded App")?
            .app
            .lock()
            .map_err(|e| e.to_string())?;
        app.git_version = Err("Git is unavailable (recorded)".into());
        app.settings.keep_running_in_tray = true;
        let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            app.handle_handoff(
                crate::instance::Request::Launch {
                    cwd: Default::default(),
                    args: vec!["--clone".into()],
                },
                ctx,
            );
        });
        assert!(app.clone_form.is_none());
        assert!(!app.windows.quit_requested);
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            assert!(app.root_close_request(ctx, true));
        });
        assert!(app.windows.root_hidden);
        assert!(
            output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Visible(false))
        );
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            app.handle_handoff(
                crate::instance::Request::Launch {
                    cwd: Default::default(),
                    args: vec!["--quit".into()],
                },
                ctx,
            );
            assert!(!app.root_close_request(ctx, true));
        });
        assert!(app.windows.quit_requested);
        assert!(
            output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::Close)
        );
        assert!(
            !output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(
            !output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Visible(true))
        );
        Ok(())
    }

    #[test]
    fn resident_close_hides_only_when_opted_in_and_quit_bypasses_hide() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        assert!(!app.settings.keep_running_in_tray);
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            assert!(!app.root_close_request(ctx, true));
        });
        assert!(
            !output
                .viewport_commands
                .get(&ViewportId::ROOT)
                .is_some_and(|commands| commands.contains(&egui::ViewportCommand::CancelClose))
        );
        app.settings.keep_running_in_tray = true;
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            assert!(app.root_close_request(ctx, true));
        });
        assert!(app.windows.root_hidden);
        assert!(
            output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Visible(false))
        );
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            app.handle_handoff(crate::instance::Request::Focus, ctx)
        });
        assert!(!app.windows.root_hidden);
        assert!(
            output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Visible(true))
        );
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            app.launch(crate::launch::Command::Quit, Default::default(), ctx);
            assert!(!app.root_close_request(ctx, true));
        });
        assert!(app.windows.quit_requested);
        assert!(
            output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::Close)
        );
        assert!(
            !output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::CancelClose)
        );
        Ok(())
    }

    #[test]
    fn hidden_root_close_is_cancelled_in_logic_but_explicit_quit_is_not() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.windows.parked.insert(
            ViewportId::from_hash_of("child"),
            Window::new(vec![Tab::Home], ViewportId::ROOT, None),
        );
        let mut raw = egui::RawInput::default();
        let info = raw.viewports.entry(ViewportId::ROOT).or_default();
        info.minimized = Some(true);
        info.events.push(egui::ViewportEvent::Close);
        let passes = ctx.cumulative_pass_nr();
        let output = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
        assert!(
            output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(
            output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::Visible(false))
        );
        assert_eq!(ctx.cumulative_pass_nr(), passes);
        app.windows.quit_requested = true;
        let output = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
        assert!(
            !output
                .viewport_commands
                .get(&ViewportId::ROOT)
                .is_some_and(|commands| commands.contains(&egui::ViewportCommand::CancelClose))
        );
        Ok(())
    }

    #[test]
    fn clone_form_is_owned_by_its_window_when_a_peer_renders_first() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        let child = ViewportId::from_hash_of("form-owner");
        app.windows
            .parked
            .insert(child, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
        app.in_window(child, |app| {
            app.start_clone();
            app.set_clone_url("https://example.invalid/owner/repo");
        });
        assert!(!app.window_has_dialog());
        let mut saw_peer_form = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.clone_dialog(ui.ctx());
            saw_peer_form = app.clone_form.is_some();
            let count = app.tabs.len();
            let blocked = app.window_has_dialog();
            app.window_command(crate::keymap::Command::NewTab, blocked, ui.ctx());
            assert_eq!(app.tabs.len(), count + 1);
        })
        .drop_without_applying_deltas();
        assert!(!saw_peer_form);
        app.in_window(child, |app| {
            assert!(app.window_has_dialog());
            assert_eq!(
                app.clone_url().as_deref(),
                Some("https://example.invalid/owner/repo")
            );
        });
        assert!(!app.window_has_dialog());
        assert!(matches!(app.tabs.last(), Some(Tab::Home)));
        drop(app);
        use egui_kittest::kittest::Queryable as _;
        let owner = h.state().app.clone();
        let mut initialized = false;
        let mut child_ui = egui_kittest::Harness::new_ui_state(
            |ui, owner: &mut Arc<Mutex<App>>| {
                if !std::mem::replace(&mut initialized, true) {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    return;
                }
                if let Ok(mut app) = owner.lock() {
                    app.in_window(child, |app| app.clone_dialog(ui.ctx()));
                }
            },
            owner.clone(),
        );
        child_ui.run_steps(2);
        child_ui.get_by_label("Clone repository");
        child_ui.get_by_label("Cancel").click();
        child_ui.run();
        let app = owner.lock().map_err(|e| e.to_string())?;
        assert!(app.windows.parked[&child].clone_form.is_none());
        assert!(!app.window_has_dialog());
        Ok(())
    }

    #[test]
    fn recorded_completions_route_to_the_initiator_and_credentials_follow_parked_targets()
    -> Result<(), String> {
        #[derive(Clone, Copy)]
        enum Completion {
            Clone,
            Connection,
            Link,
        }
        for source in [Completion::Clone, Completion::Connection, Completion::Link] {
            for poll_from_child in [false, true] {
                let h = recorded_harness();
                let ctx = h.ctx.clone();
                let mut app = h
                    .state()
                    .as_ref()
                    .ok_or("no recorded App")?
                    .app
                    .lock()
                    .map_err(|e| e.to_string())?;
                let child = ViewportId::from_hash_of("completion-owner");
                let root = PathBuf::from("/virtual/completion-target");
                app.tabs = vec![Tab::Home];
                app.windows.parked.insert(
                    child,
                    Window::new(
                        vec![deferred(root.to_str().ok_or("path")?, &ctx), Tab::Home],
                        ViewportId::ROOT,
                        None,
                    ),
                );
                app.windows.parked.get_mut(&child).ok_or("child")?.active = 1;
                match source {
                    Completion::Clone => app.recorded_clone_completion(child, root.clone()),
                    Completion::Connection => {
                        app.recorded_connection_completion(child, root.clone())
                    }
                    Completion::Link => app.recorded_launch_completion(child, root.clone()),
                }
                let poller = if poll_from_child {
                    child
                } else {
                    ViewportId::ROOT
                };
                app.in_window(poller, |app| match source {
                    Completion::Clone => app.poll_clone(&ctx),
                    Completion::Connection => app.poll_connecting(&ctx),
                    Completion::Link => app.poll_launch(&ctx),
                });
                assert_eq!(app.windows.parked[&child].active, 0);
                assert!(
                    matches!(&app.windows.parked[&child].tabs[0], Tab::Repo(repo) if repo.has_test_credentials("recorded-user", "recorded-secret") != matches!(source, Completion::Link))
                );
                assert!(matches!(app.tabs.first(), Some(Tab::Home)));
                // A completion may find its target in a different live window.
                app.recorded_connection_completion(ViewportId::ROOT, root.clone());
                app.poll_connecting(&ctx);
                assert!(
                    matches!(&app.windows.parked[&child].tabs[0], Tab::Repo(repo) if repo.has_test_credentials("recorded-user", "recorded-secret"))
                );
            }
        }
        Ok(())
    }

    #[test]
    fn closed_owner_falls_back_to_root_not_the_polling_peer() -> Result<(), String> {
        let h = harness();
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        let peer = ViewportId::from_hash_of("polling-peer");
        app.windows
            .parked
            .insert(peer, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
        app.in_window(peer, |app| {
            app.in_window(ViewportId::from_hash_of("closed-owner"), |app| {
                assert_eq!(app.windows.current, ViewportId::ROOT);
                app.error = Some("recorded result".into());
            });
            assert_eq!(app.windows.current, peer);
            assert!(app.error.is_none());
        });
        assert_eq!(app.error.as_deref(), Some("recorded result"));
        Ok(())
    }

    #[test]
    fn hidden_logic_advances_the_patch_queue_without_a_paint_pass() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![Tab::Home];
        app.patches
            .push("recorded.patch".into(), "/no-live-session".into());
        let before = app.toasts.len();
        let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.tick_model(ctx, None));
        assert!(app.patches.is_empty());
        assert_eq!(app.toasts.len(), before + 1);
        let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.tick_model(ctx, None));
        assert_eq!(app.toasts.len(), before + 1);
        Ok(())
    }

    #[test]
    fn restored_bounds_delegate_placement_and_fit_the_available_monitor() {
        assert_eq!(
            bounded_size(egui::vec2(5000.0, 4000.0), Some(egui::vec2(1280.0, 720.0))),
            egui::vec2(1216.0, 656.0)
        );
        assert_eq!(
            bounded_size(egui::vec2(f32::NAN, 0.0), Some(egui::vec2(640.0, 480.0))),
            egui::vec2(576.0, 416.0)
        );
    }

    #[test]
    fn mixed_dpi_strips_and_source_pointer_use_physical_desktop_pixels() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![deferred("/virtual/drag-source", &ctx)];
        let peer = ViewportId::from_hash_of("two-times-scale");
        app.windows
            .parked
            .insert(peer, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
        let mut raw = egui::RawInput {
            viewport_id: peer,
            ..Default::default()
        };
        let info = raw.viewports.entry(peer).or_default();
        info.native_pixels_per_point = Some(2.0);
        info.inner_rect = Some(Rect::from_min_size(
            egui::pos2(800.0, 50.0),
            egui::vec2(400.0, 300.0),
        ));
        app.windows.current = peer;
        ctx.run_ui(raw, |ui| {
            app.window_strip(
                ui.ctx(),
                Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 20.0)),
            )
        })
        .drop_without_applying_deltas();
        assert_eq!(
            app.windows.strips[&peer],
            Rect::from_min_size(egui::pos2(1600.0, 100.0), egui::vec2(800.0, 40.0))
        );
        app.windows.current = ViewportId::ROOT;
        let mut raw = egui::RawInput {
            events: vec![egui::Event::PointerMoved(egui::pos2(1700.0, 120.0))],
            ..Default::default()
        };
        let info = raw.viewports.entry(ViewportId::ROOT).or_default();
        info.native_pixels_per_point = Some(1.0);
        info.inner_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0)));
        ctx.run_ui(raw, |ui| {
            assert_eq!(
                app.drop_window_target(
                    0,
                    Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 40.0)),
                    ui.ctx()
                ),
                Some((Some(peer), egui::pos2(1700.0, 120.0)))
            );
        })
        .drop_without_applying_deltas();
        Ok(())
    }

    #[test]
    fn new_window_applies_physical_drop_position_using_its_actual_scale() -> Result<(), String> {
        let h = harness();
        let ctx = h.ctx.clone();
        ctx.set_embed_viewports(false);
        let owner = h.state().app.clone();
        let id = ViewportId::from_hash_of("positioned-child");
        let physical = egui::pos2(1700.0, 120.0);
        let mut window = Window::new(vec![Tab::Home], ViewportId::ROOT, None);
        window.place_at_pixels = Some(physical);
        owner
            .lock()
            .map_err(|e| e.to_string())?
            .windows
            .parked
            .insert(id, window);
        let mut raw = egui::RawInput {
            viewport_id: id,
            ..Default::default()
        };
        raw.viewports.entry(id).or_default().native_pixels_per_point = Some(2.0);
        let output = ctx.run_ui(raw, |ui| {
            DesktopApp::child(&Arc::downgrade(&owner), &h.state().cache, id, ui)
        });
        assert!(
            output.viewport_output[&id]
                .commands
                .contains(&egui::ViewportCommand::OuterPosition(physical / 2.0))
        );
        output.drop_without_applying_deltas();
        assert!(
            owner.lock().map_err(|e| e.to_string())?.windows.parked[&id]
                .place_at_pixels
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn contended_logic_defers_close_then_replays_keep_children_or_explicit_quit()
    -> Result<(), String> {
        for quit in [false, true] {
            let mut h = harness();
            let ctx = h.ctx.clone();
            let owner = h.state().app.clone();
            let child = ViewportId::from_hash_of("kept-during-contention");
            let mut held = owner.lock().map_err(|e| e.to_string())?;
            held.windows
                .parked
                .insert(child, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
            held.windows.quit_requested = quit;
            let mut raw = egui::RawInput::default();
            let info = raw.viewports.entry(ViewportId::ROOT).or_default();
            info.minimized = Some(true);
            info.events.push(egui::ViewportEvent::Close);
            let output = ctx.run_logic(&raw, |ctx| h.state_mut().logic_tick(ctx, None));
            assert!(
                output.viewport_commands[&ViewportId::ROOT]
                    .contains(&egui::ViewportCommand::CancelClose)
            );
            assert!(h.state().deferred_close);
            drop(held);
            let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
                h.state_mut().logic_tick(ctx, None)
            });
            let commands = &output.viewport_commands[&ViewportId::ROOT];
            if quit {
                assert!(commands.contains(&egui::ViewportCommand::Close));
                assert!(!commands.contains(&egui::ViewportCommand::CancelClose));
            } else {
                assert!(commands.contains(&egui::ViewportCommand::CancelClose));
                assert!(commands.contains(&egui::ViewportCommand::Visible(false)));
            }
            assert!(!h.state().deferred_close);
            assert!(
                owner
                    .lock()
                    .map_err(|e| e.to_string())?
                    .windows
                    .parked
                    .contains_key(&child)
            );
        }
        Ok(())
    }

    #[test]
    fn contended_root_ui_uses_the_same_deferred_close_policy() -> Result<(), String> {
        let mut h = harness();
        let ctx = h.ctx.clone();
        let owner = h.state().app.clone();
        let child = ViewportId::from_hash_of("kept-during-ui-contention");
        let mut held = owner.lock().map_err(|e| e.to_string())?;
        held.windows
            .parked
            .insert(child, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
        let mut raw = egui::RawInput::default();
        raw.viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run_ui(raw, |ui| h.state_mut().ui_tick(ui, None));
        assert!(
            output.viewport_output[&ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(h.state().deferred_close);
        output.drop_without_applying_deltas();
        drop(held);
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            h.state_mut().logic_tick(ctx, None)
        });
        assert!(
            output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(
            owner
                .lock()
                .map_err(|e| e.to_string())?
                .windows
                .parked
                .contains_key(&child)
        );
        assert!(!h.state().deferred_close);
        Ok(())
    }
}
