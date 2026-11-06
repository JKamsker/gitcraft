//! The card that explains a git failure gitcraft knows (see [`crate::git::errors`]) and offers
//! the fix for it: shown in a repository's tab under the toolbar, and on the start page when a
//! repository couldn't be opened. What it reads and changes runs in the background, on the
//! repository's machine (here, in WSL or over SSH); the window never waits for it.

mod fix;
#[cfg(test)]
mod tests;
pub mod text;
mod view;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

pub use fix::{Fix, Found};
use text::Place;

use crate::git::errors::{self, GitProblem};

/// Most of git's own words the card keeps, for "What git said".
const KEPT_OUTPUT: usize = 4000;

/// A known failure, explained.
pub struct ErrorCard {
    problem: GitProblem,
    /// Where git ran: the repository, or the folder that was being opened.
    dir: PathBuf,
    place: Place,
    /// What failed, as its status line said ("Push", "Open").
    label: String,
    /// git's output, cut to [`KEPT_OUTPUT`].
    said: String,
    /// What the background found: `None` while it looks.
    found: Option<Result<Found, String>>,
    stage: Stage,
    /// The background work going on, if any.
    rx: Option<Receiver<Reply>>,
}

/// Where the fix is at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Showing the problem (and the fix's button).
    Idle,
    /// Asking whether to make the change it says.
    Confirming,
    /// Making it, or undoing it.
    Working,
    /// Made: what was done, and how to take it back.
    Done { what: String, undo: Option<Fix> },
    /// Taken back.
    Undone(String),
    /// It failed: why.
    Failed(String),
}

enum Reply {
    Found(Result<Found, String>),
    Applied(Result<(String, Option<Fix>), String>),
    Undone(Result<String, String>),
}

/// What the card asks of its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardEvent {
    /// Run what failed again.
    Retry,
    /// Close the card.
    Close,
}

impl ErrorCard {
    /// A card for `error` (what git said when `label` failed, running in `dir`), if it is a
    /// failure gitcraft knows; it starts finding out more in the background.
    pub fn for_error(error: &str, dir: &Path, label: &str, ctx: &egui::Context) -> Option<Self> {
        let problem = errors::explain(error)?;
        let place = Place {
            windows: cfg!(windows),
            remote: crate::remote::is_remote(dir),
        };
        let mut card = Self {
            problem,
            dir: dir.to_owned(),
            place,
            label: label.to_owned(),
            said: cut(error, KEPT_OUTPUT),
            found: None,
            stage: Stage::Idle,
            rx: None,
        };
        let (problem, dir) = (card.problem.clone(), card.dir.clone());
        card.start(ctx, move || {
            Reply::Found(fix::gather(&problem, &dir, place))
        });
        Some(card)
    }

    #[cfg(test)]
    pub fn problem(&self) -> &GitProblem {
        &self.problem
    }

    #[cfg(test)]
    pub(crate) fn stage(&self) -> &Stage {
        &self.stage
    }

    /// The repository (or folder) git ran in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// What failed ("Push").
    pub fn label(&self) -> &str {
        &self.label
    }

    /// A fix was made and not taken back: the card stays, for its undo.
    pub fn fixed(&self) -> bool {
        matches!(self.stage, Stage::Done { .. })
    }

    /// Runs `job` in the background; its reply is taken in by [`Self::poll`].
    fn start(&mut self, ctx: &egui::Context, job: impl FnOnce() -> Reply + Send + 'static) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
        self.rx = Some(rx);
    }

    /// Takes in what the background work came back with.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        let reply = match rx.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => return,
            // It panicked (a bug): say so rather than wait for good.
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                let e = "Internal error: the check stopped before it was done".to_owned();
                if self.found.is_none() {
                    self.found = Some(Err(e));
                } else {
                    self.stage = Stage::Failed(e);
                }
                return;
            }
        };
        self.rx = None;
        match reply {
            Reply::Found(found) => self.found = Some(found),
            Reply::Applied(Ok((what, undo))) => self.stage = Stage::Done { what, undo },
            Reply::Applied(Err(e)) => self.stage = Stage::Failed(e),
            Reply::Undone(Ok(what)) => self.stage = Stage::Undone(what),
            Reply::Undone(Err(e)) => self.stage = Stage::Failed(e),
        }
    }

    /// The fix offered, once found.
    pub fn offered(&self) -> Option<&Fix> {
        self.found.as_ref()?.as_ref().ok()?.fix.as_ref()
    }

    /// Asks to confirm the fix offered.
    pub fn ask(&mut self) {
        if self.offered().is_some() && self.stage == Stage::Idle {
            self.stage = Stage::Confirming;
        }
    }

    /// Back to showing the problem (the confirmation cancelled, a failure read).
    pub fn back(&mut self) {
        if matches!(self.stage, Stage::Confirming | Stage::Failed(_)) {
            self.stage = Stage::Idle;
        }
    }

    /// Makes the fix confirmed, in the background.
    pub fn confirm(&mut self, ctx: &egui::Context) {
        let Some(fix) = self.offered().cloned() else {
            return;
        };
        if self.stage != Stage::Confirming {
            return;
        }
        self.stage = Stage::Working;
        let dir = self.dir.clone();
        self.start(ctx, move || Reply::Applied(fix.apply(&dir)));
    }

    /// Takes the fix made back, in the background.
    pub fn undo(&mut self, ctx: &egui::Context) {
        let Stage::Done {
            undo: Some(fix), ..
        } = &self.stage
        else {
            return;
        };
        let (fix, dir) = (fix.clone(), self.dir.clone());
        self.stage = Stage::Working;
        self.start(ctx, move || Reply::Undone(fix.undo(&dir)));
    }
}

/// `s`, at most `max` bytes of it (on a character boundary), `…` when cut.
fn cut(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}
