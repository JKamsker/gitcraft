//! Deferred viewport coordinator, callback cache, and eframe lifecycle.

use super::*;

/// eframe's root owns this coordinator. Deferred callbacks keep Weak references so closing
/// the application releases sessions, terminals, watchers and callback captures normally.
#[derive(Clone, Default)]
pub(super) struct Children {
    pub(super) full: Vec<(ViewportId, ViewportBuilder)>,
    pub(super) panels: Vec<(ViewportId, ViewportBuilder)>,
}
pub(super) type WindowCache = Arc<Mutex<BTreeMap<ViewportId, Children>>>;

pub struct DesktopApp {
    pub(super) app: Arc<Mutex<App>>,
    pub(super) cache: WindowCache,
    pub(super) deferred_close: bool,
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

    pub(super) fn children(owner: &Arc<Mutex<App>>, cache: &WindowCache, ctx: &egui::Context) {
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
                    let mut b = super::super::window_viewport()
                        .with_title(title)
                        .with_app_id("gitcraft")
                        .with_icon(super::super::window_icon())
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

    pub(super) fn child(weak: &Weak<Mutex<App>>, cache: &WindowCache, id: ViewportId, ui: &mut Ui) {
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

    pub(super) fn logic_tick(&mut self, ctx: &egui::Context, frame: Option<&eframe::Frame>) {
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

    pub(super) fn defer_close(ctx: &egui::Context, pending: &mut bool) {
        if ctx.viewport_id() == ViewportId::ROOT && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            *pending = true;
        }
    }

    pub(super) fn ui_tick(&mut self, ui: &mut Ui, frame: Option<&mut eframe::Frame>) {
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
