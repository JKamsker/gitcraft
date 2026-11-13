//! Application construction and restoration from persisted state.

use super::*;

impl App {
    /// `open_arg`: a repository passed on the command line, opened (or focused) on start.
    pub fn new(cc: &eframe::CreationContext<'_>, open_arg: Option<PathBuf>) -> Self {
        Self::launched(cc, open_arg.map(crate::launch::Command::Open))
    }

    /// The window, carrying out `cmd` from the command line (`gitcraft log <file>`…): its
    /// relative paths are taken from the current folder.
    pub fn launched(cc: &eframe::CreationContext<'_>, cmd: Option<crate::launch::Command>) -> Self {
        Self::launched_using(cc, cmd, git::version)
    }

    pub(super) fn launched_using(
        cc: &eframe::CreationContext<'_>,
        cmd: Option<crate::launch::Command>,
        version: impl FnOnce() -> Result<String, String>,
    ) -> Self {
        // The window's thread: it never waits for another machine (see `remote::ui_guard`).
        crate::remote::mark_ui_thread();
        // ssh's questions (passwords, new host keys) come to this window.
        crate::remote::askpass::set_repaint(cc.egui_ctx.clone());
        // So do later starts' requests (see `crate::instance`).
        crate::instance::set_repaint(cc.egui_ctx.clone());
        let native_ctx = cc.egui_ctx.clone();
        gitcraft_os::set_native_url_wake(move || native_ctx.request_repaint());
        let dock_ctx = cc.egui_ctx.clone();
        gitcraft_os::set_native_dock_wake(move || dock_ctx.request_repaint());
        let saved: Saved = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();
        let ctx = &cc.egui_ctx;
        egui_extras::install_image_loaders(ctx);
        crate::svg::install(ctx);
        crate::image_fit::install(ctx);
        crate::image_cache::install(ctx);
        theme::install_fonts(ctx);
        let mut settings = saved.settings;
        theme::apply(ctx, &saved.theme, &settings.theme_tweaks);
        settings.ensure_profile();
        settings.set(ctx);
        // Before anything could reach the credential store.
        crate::portable::set_credential_store(settings.portable_credential_store);
        crate::desktop_entry::refresh();

        let mut app = Self {
            windows: Default::default(),
            resident: Default::default(),
            git_version: version(),
            tabs: Vec::new(),
            switcher_query: String::new(),
            switcher_focus: false,
            switch_to: None,
            close_worktree: false,
            active: 0,
            recent: saved.recent,
            theme: saved.theme.clone(),
            clone_parent: saved.clone_parent,
            settings,
            profile_states: saved.profile_states,
            profile_git_seen: String::new(),
            manual_open: false,
            adapter: diagnostics::adapter(cc),
            diagnostics: None,
            palette: None,
            palette_recent: saved.palette_recent,
            toasts: Vec::new(),
            closed: Vec::new(),
            tab_drag: None,
            keeper: Default::default(),
            session: crate::drafts::Session::start(),
            recent_filter: Default::default(),
            recent_status: Default::default(),
            last_opened: saved.last_opened,
            patches: Default::default(),
            repo_scan: None,
            library: library::LibraryState::new(saved.library),
            library_open: false,
            launching: Default::default(),
            home_link: String::new(),
            tab_scroll: 0.0,
            tab_scrolled_to: None,
            decorated: title_bar::decorated_now(),
            window_border: None,
            error: None,
            error_card: None,
            focus: focus::State::default(),
            clone_form: None,
            practice_open: false,
            setup: Default::default(),
            practice_jobs: crate::tasks::Scope::default(),
            practicing: None,
            cloning: None,
            cloning_form: None,
            pick_on_start: false,
            clone_failed: false,
            taskbar: Default::default(),
            menu_for: None,
            remote_form: None,
            connecting: Vec::new(),
            teleport: None,
            teleport_drag: None,
            askpass: Default::default(),
            update: None,
            update_discovery: Default::default(),
            update_later: false,
            update_error: None,
            update_failure: None,
            skipped_version: saved.skipped_version,
            pinned_version: update::settle_pin(&saved.pinned_version, update::CURRENT),
            previous_version: update::previous(),
            tab_looks: saved.tab_looks,
            workspaces: saved.workspaces,
            workspaces_view: Default::default(),
            found_repos: saved.found_repos,
            my_work: Default::default(),
            inbox: Default::default(),
            inbox_seen: saved.inbox_seen,
        };
        // Before the window first shows, so its title bar doesn't flash.
        title_bar::apply(ctx, &app.settings, &mut app.decorated);
        update::clean_up();
        if let crate::portable::Mode::Unusable(why) = crate::portable::mode() {
            app.toasts.push(Toast::new(why.clone(), true));
        }
        let mut kept = Vec::new();
        if app.git_version.is_ok() {
            for path in saved.open {
                let is_dir = tabs::reopenable(&path);
                kept.push(is_dir);
                if is_dir {
                    // Read once shown, or once the one shown is read: one at a time first.
                    app.tabs
                        .push(Tab::Repo(Box::new(RepoTab::new_deferred(path, ctx))));
                }
            }
        }
        if app.tabs.is_empty() {
            app.tabs.push(Tab::Home);
        }
        app.restore_windows(saved.windows, ctx);
        app.active = active_among_kept(&kept, saved.active).min(app.tabs.len() - 1);
        if let Some(cmd) = cmd
            && app.git_version.is_ok()
        {
            let cwd = std::env::current_dir().unwrap_or_default();
            app.launch(cmd, cwd, ctx);
        }
        app
    }
}
