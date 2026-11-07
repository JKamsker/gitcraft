//! Carrying out what gitcraft was started for (`gitcraft log <file>`…, see `crate::launch`):
//! resolved off the window's thread, then the repository opened and the view asked for shown.
//! Several can be on their way at once: choosing History in gitcraft on five files starts
//! gitcraft five times, each handing its own over.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::panels::Toast;
use super::{App, Tab};
use crate::launch::open_link::Known;
use crate::launch::{Command, Resolved, Then};

/// Requests being made sense of, and those waiting for their repository's tab.
#[derive(Default)]
pub(super) struct Launching {
    resolving: Vec<Receiver<Result<Resolved, String>>>,
    /// What to show once the tab of each folder is there (one elsewhere opens in the
    /// background).
    waiting: Vec<(PathBuf, Then)>,
}

impl App {
    /// Starts carrying out `cmd`, its relative paths taken from `cwd`.
    pub(crate) fn launch(&mut self, cmd: Command, cwd: PathBuf, ctx: &egui::Context) {
        match cmd {
            Command::Clone => {
                self.start_clone();
                return;
            }
            Command::PickFolder => {
                self.pick_on_start = true;
                ctx.request_repaint();
                return;
            }
            _ => {}
        }
        // A folder elsewhere opens as the start page's Remote does, asking what ssh asks.
        if let Command::Open(path) = &cmd {
            let path = crate::launch::resolve::absolute(&cwd, path);
            if crate::remote::is_remote(&path) {
                self.open_repo(&path, ctx);
                return;
            }
        }
        // Where a link's clone may be: the open tabs, with the remotes they read (a tab
        // elsewhere not read yet is left out: it would be reached just to look), then the
        // recent repositories of this machine.
        let mut clones: Vec<Known> = self
            .tabs
            .iter()
            .filter_map(|t| match t {
                Tab::Repo(r) => {
                    let remotes = r.remotes_read();
                    let skip = remotes.is_none() && crate::remote::is_remote(&r.path);
                    (!skip).then(|| Known {
                        path: r.path.clone(),
                        remotes,
                    })
                }
                _ => None,
            })
            .collect();
        for path in &self.recent {
            if !crate::remote::is_remote(path) && !clones.iter().any(|c| c.path == *path) {
                clones.push(Known {
                    path: path.clone(),
                    remotes: None,
                });
            }
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::launch::resolve::resolve_among(&cmd, &cwd, &clones));
            ctx.request_repaint();
        });
        self.launching.resolving.push(rx);
    }

    /// Takes in resolved requests, and shows what each asked for once its tab is open.
    pub(super) fn poll_launch(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.launching.resolving.retain(|rx| match rx.try_recv() {
            Ok(result) => {
                done.push(result);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        for result in done {
            match result {
                Ok(Resolved {
                    then: Then::Clone(url),
                    ..
                }) => self.offer_clone(url),
                Ok(resolved) => {
                    if crate::remote::is_remote(&resolved.root) {
                        self.open_repo(&resolved.root, ctx);
                    } else {
                        // Already the top folder, as tabs name it.
                        self.open_root(resolved.root.clone(), ctx);
                    }
                    self.launching.waiting.push((resolved.root, resolved.then));
                }
                Err(e) => self.report(e),
            }
        }
        let waiting = std::mem::take(&mut self.launching.waiting);
        for (root, then) in waiting {
            match self.tab_of(&root) {
                Some(i) => self.show_in(i, then),
                // Elsewhere: still being reached (or that failed, and said so).
                None if !self.connecting.is_empty() => self.launching.waiting.push((root, then)),
                None => {}
            }
        }
    }

    /// Shows `then` in the repository tab `i`, unless an editor there holds unsaved work (a
    /// conflict being solved, an interactive rebase being set up) that it would close.
    fn show_in(&mut self, i: usize, then: Then) {
        self.active = i;
        let Some(Tab::Repo(repo)) = self.tabs.get_mut(i) else {
            return;
        };
        if then != Then::Nothing && repo.editor_open() {
            let name = repo.name.clone();
            self.toasts.push(Toast::new(
                format!(
                    "{name}: finish or close the conflict or rebase editor first, then run the \
                     command again"
                ),
                true,
            ));
            return;
        }
        for action in crate::launch::actions(then) {
            repo.queue(action);
        }
    }

    fn tab_of(&self, root: &Path) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| matches!(t, Tab::Repo(r) if r.path == root))
    }
}
