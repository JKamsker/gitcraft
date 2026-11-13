//! Persisted application state and process lifecycle entry points.

use super::*;

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.tick_model(ctx, Some(frame));
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        self.window_ui(ui, Some(frame));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Index of the active tab among the saved (repo) tabs.
        let (mut open, active) = self.open_repos();
        let detached = self.saved_windows();
        open.retain(|p| !detached.iter().any(|w| w.open.contains(p)));
        let saved = Saved {
            open,
            active,
            recent: self.recent.clone(),
            theme: self.theme.clone(),
            clone_parent: self.clone_parent.clone(),
            settings: self.settings.clone(),
            skipped_version: self.skipped_version.clone(),
            pinned_version: self.pinned_version.clone(),
            tab_looks: self.tab_looks.clone(),
            profile_states: self.profile_states.clone(),
            palette_recent: self.palette_recent.clone(),
            last_opened: self.last_opened.clone(),
            workspaces: self.workspaces.clone(),
            found_repos: self.found_repos.clone(),
            library: self.library.saved().clone(),
            inbox_seen: self.inbox_seen.clone(),
            windows: detached,
        };
        eframe::set_value(storage, eframe::APP_KEY, &saved);
    }

    fn on_exit(&mut self) {
        self.stop_teleport();
        let owners = self.windows.parked.keys().copied().collect::<Vec<_>>();
        for owner in owners {
            self.in_window(owner, Self::stop_teleport);
        }
        self.close_drafts();
        crate::instance::closing();
    }
}

/// Where the tab that was active at `active` is once the tabs not `kept` (their folder is gone)
/// are left out: a gone one passes to the tab before it.
pub(super) fn active_among_kept(kept: &[bool], active: usize) -> usize {
    kept.iter()
        .take(active.saturating_add(1))
        .filter(|&&k| k)
        .count()
        .saturating_sub(1)
}

/// What the widgets of the repository tab of `path` are told apart from another tab's by.
pub(super) fn repo_scope(path: &Path) -> egui::Id {
    egui::Id::new(("repo-tab", path))
}

/// The top of the repository `path` is in, in the form tabs are told apart by.
pub(crate) fn repo_root(path: &Path) -> Result<PathBuf, String> {
    crate::launch::resolve::repo_root(path)
}

pub use draft_saving::keep_no_drafts;

/// When this process was started as `gitcraft checkpoint` by a coding agent's hook: saves the
/// turn and returns the exit code to end with, before any window is made. `None` otherwise.
pub fn checkpoint_cli() -> Option<i32> {
    crate::agent::checkpoint::cli()
}

/// When this process was started by ssh as its askpass program: asks the running app ssh's
/// question and returns the exit code to end with, before any window is made. `None` for an
/// ordinary start.
pub fn askpass_helper() -> Option<i32> {
    crate::remote::askpass::run_helper()
}

/// How many commands for a repository on another machine were asked from the window's thread
/// (and refused: it never waits for a network). For tests and `examples/ui_shot.rs`.
pub fn refused_on_ui_thread() -> usize {
    crate::remote::refused_on_ui_thread()
}

/// Work the first frame will need, started before the window is made so that it overlaps with
/// that: on macOS, asking the login shell for its PATH, which every git call waits for (its
/// profile scripts can take seconds).
pub fn warm_up() {
    std::thread::spawn(|| {
        let _ = crate::shell_path::path();
    });
}

/// The file the window's state is saved in, when it isn't eframe's own default place: in the
/// portable folder when gitcraft runs portable.
pub fn persistence_path() -> Option<PathBuf> {
    crate::portable::persistence_path()
}

pub use crate::instance::Start;

/// Settles, before any window is made, whether this start opens the window or hands what it was
/// started for (a folder, or a command line such as `log <file>`) to the gitcraft already running
/// (see `crate::instance`).
pub fn single_instance(
    launch: Option<&crate::launch::Command>,
    args: &[std::ffi::OsString],
) -> Start {
    use crate::launch::Command;
    match launch {
        None => crate::instance::start(None),
        Some(Command::Open(path)) => crate::instance::start(Some(path)),
        // `log <file>`…: the command line as typed, read again by the running window.
        Some(_) => crate::instance::start_launch(args),
    }
}

/// Selected-language human presentation; does not execute startup or native actions.
pub fn display_start_message(message: &str) -> String {
    panels::display_app_message(message)
}

/// Tells the user why this start ends without a window: in a dialog, and on stderr for a start
/// from a terminal.
/// What the command line asked to be told (`--help`, a mistake in it): on the terminal it was
/// typed in, or in a dialog when there is none to see it (a Windows release has no console; a
/// menu or a file manager starts gitcraft without one).
pub fn tell(message: &str, error: bool) {
    use std::io::IsTerminal;
    let display = display_start_message(message);
    let message = display.as_str();
    let terminal = !cfg!(all(windows, not(debug_assertions))) && std::io::stderr().is_terminal();
    match (terminal, error) {
        (true, true) => eprintln!("gitcraft: {message}"),
        (true, false) => println!("{message}"),
        (false, true) => report_displayed_start(message),
        (false, false) => {
            let _ = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Info)
                .set_title("gitcraft")
                .set_description(message)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
        }
    }
}

pub fn report_failed_start(message: &str) {
    let display = display_start_message(message);
    report_displayed_start(&display);
}

fn report_displayed_start(message: &str) {
    eprintln!("gitcraft: {message}");
    // rfd's dialogs are zenity's on Linux; without it (KDE, minimal installs) a start from the
    // menu would end with nothing shown: a desktop notification says it instead.
    if cfg!(all(unix, not(target_os = "macos"))) && !on_path("zenity") {
        let _ = notify_rust::Notification::new()
            .appname("gitcraft")
            .summary("gitcraft")
            .body(&panels::notification_body(message))
            .show();
        return;
    }
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("gitcraft")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// Whether a program called `name` is on the PATH.
pub(super) fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// Once the window closed: lets the next start be the running instance, then starts the
/// version an update put in place, if it did.
pub fn after_exit() {
    crate::tasks::shutdown();
    crate::parallel_bisect::shutdown();
    crate::processes::shutdown_leases();
    crate::processes::shutdown();
    crate::remote::shutdown();
    crate::processes::shutdown_cleanup();
    gitcraft_os::retire_process_helpers();
    crate::instance::shut_down();
    update::relaunch();
}

/// The window's native frame, configured before creation from the saved title-bar setting.
pub fn window_viewport() -> egui::ViewportBuilder {
    let app_ron = eframe::storage_dir("gitcraft").map(|dir| dir.join("app.ron"));
    title_bar::viewport(title_bar::decorated_at_start(app_ron.as_deref()))
}

/// The window and taskbar icon (the `.exe` gets the same picture from `build.rs`).
pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/app-icon/gitcraft-256.png"))
        .unwrap_or_default()
}

/// `canonicalize` on Windows returns `\\?\C:\…`; show and store the familiar form.
pub(crate) fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

/// Installs the persistent local panic log before the application window is created.
pub fn install_panic_log() {
    crate::diagnostics::crash::install();
}
