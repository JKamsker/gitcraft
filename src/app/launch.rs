//! Carrying out what gitcraft was started for (`gitcraft log <file>`…, see `crate::launch`):
//! resolved off the window's thread, then the repository opened and the view asked for shown.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use super::{App, Tab};
use crate::launch::{Command, Resolved, Then};

/// A request being made sense of, and one waiting for its repository's tab.
#[derive(Default)]
pub(super) struct Launching {
    rx: Option<Receiver<Result<Resolved, String>>>,
    /// What to show once the tab of this folder is there (one elsewhere opens in the
    /// background).
    then: Option<(PathBuf, Then)>,
}

impl App {
    /// Starts carrying out `cmd`, its relative paths taken from `cwd`.
    pub(crate) fn launch(&mut self, cmd: Command, cwd: PathBuf, ctx: &egui::Context) {
        // A folder elsewhere opens as the start page's Remote does, asking what ssh asks.
        if let Command::Open(path) = &cmd {
            let path = crate::launch::resolve::absolute(&cwd, path);
            if crate::remote::is_remote(&path) {
                self.open_repo(&path, ctx);
                return;
            }
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::launch::resolve::resolve(&cmd, &cwd));
            ctx.request_repaint();
        });
        self.launching.rx = Some(rx);
    }

    /// Takes in a resolved request, and shows what it asked for once its tab is open.
    pub(super) fn poll_launch(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.launching.rx
            && let Ok(result) = rx.try_recv()
        {
            self.launching.rx = None;
            match result {
                Ok(resolved) => {
                    if crate::remote::is_remote(&resolved.root) {
                        self.open_repo(&resolved.root, ctx);
                    } else {
                        // Already the top folder, as tabs name it.
                        self.open_root(resolved.root.clone(), ctx);
                    }
                    self.launching.then = Some((resolved.root, resolved.then));
                }
                Err(e) => self.report(e),
            }
        }
        let Some((root, _)) = &self.launching.then else {
            return;
        };
        let Some(i) = self.tab_of(root) else {
            // Elsewhere: still being reached (or that failed, and said so).
            if !self.connecting.is_empty() {
                return;
            }
            self.launching.then = None;
            return;
        };
        let Some((_, then)) = self.launching.then.take() else {
            return;
        };
        self.active = i;
        if let Some(Tab::Repo(repo)) = self.tabs.get_mut(i) {
            for action in crate::launch::actions(then) {
                repo.queue(action);
            }
        }
    }

    fn tab_of(&self, root: &Path) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| matches!(t, Tab::Repo(r) if r.path == root))
    }
}
