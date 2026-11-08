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
    /// A fix the card this one replaced had made, still undoable from here: (what it did,
    /// its undo).
    earlier: Option<(String, Fix)>,
    /// What came of undoing [`Self::earlier`].
    earlier_undone: Option<Result<String, String>>,
    /// The background work undoing [`Self::earlier`], if any.
    earlier_rx: Option<Receiver<Result<String, String>>>,
    /// Try again was used since what it is about last failed.
    retried: bool,
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
    /// Undone, or why not with the undo kept to try again.
    Undone(Result<String, (String, Fix)>),
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
            earlier: None,
            earlier_undone: None,
            earlier_rx: None,
            retried: false,
        };
        card.look_again(ctx);
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

    /// A fix was made that can still be taken back here: the card stays, for its undo.
    pub fn fixed(&self) -> bool {
        matches!(self.stage, Stage::Done { undo: Some(_), .. }) || self.earlier.is_some()
    }

    /// Takes the place of `old`, keeping the undo of a fix it made (the config change stays
    /// until undone, so its undo must not go with the card).
    pub fn replacing(mut self, old: Option<ErrorCard>) -> Self {
        if let Some(old) = old {
            self.earlier = match old.stage {
                Stage::Done {
                    what,
                    undo: Some(undo),
                } => Some((what, undo)),
                _ => old.earlier,
            };
        }
        self
    }

    /// What an earlier card's fix did, while it can be undone here.
    pub fn earlier(&self) -> Option<&str> {
        self.earlier.as_ref().map(|(what, _)| what.as_str())
    }

    /// What came of undoing the earlier card's fix.
    pub fn earlier_undone(&self) -> Option<&Result<String, String>> {
        self.earlier_undone.as_ref()
    }

    /// Undoes the earlier card's fix, in the background.
    pub fn undo_earlier(&mut self, ctx: &egui::Context) {
        let Some((_, fix)) = self.earlier.take() else {
            return;
        };
        let (tx, rx) = channel();
        let (ctx, dir) = (ctx.clone(), self.dir.clone());
        std::thread::spawn(move || {
            let _ = tx.send(fix.undo(&dir));
            ctx.request_repaint();
        });
        self.earlier_rx = Some(rx);
    }

    fn poll_earlier(&mut self) {
        let Some(rx) = &self.earlier_rx else { return };
        match rx.try_recv() {
            Ok(done) => self.earlier_undone = Some(done),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.earlier_undone = Some(Err(
                    crate::i18n::text("interaction-error-undo-stopped").to_owned()
                ))
            }
        }
        self.earlier_rx = None;
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
        self.poll_earlier();
        let Some(rx) = &self.rx else { return };
        let reply = match rx.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => return,
            // It panicked (a bug): say so rather than wait for good.
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                let e = crate::i18n::text("interaction-error-check-stopped").to_owned();
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
            Reply::Undone(Err((e, fix))) => {
                self.stage = Stage::Done {
                    what: crate::i18n::message(
                        "interaction-error-undo-failed",
                        &[("error", e.as_str().into())],
                    ),
                    undo: Some(fix),
                }
            }
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
    pub fn back(&mut self, ctx: &egui::Context) {
        match self.stage {
            Stage::Confirming => self.stage = Stage::Idle,
            // What was found may be what made it fail (a lock written again since): found
            // out afresh.
            Stage::Failed(_) => {
                self.stage = Stage::Idle;
                self.look_again(ctx);
            }
            _ => {}
        }
    }

    /// Finds out about the problem again, in the background.
    fn look_again(&mut self, ctx: &egui::Context) {
        self.found = None;
        let (problem, dir, place) = (self.problem.clone(), self.dir.clone(), self.place);
        self.start(ctx, move || {
            Reply::Found(fix::gather(&problem, &dir, place))
        });
    }

    /// Whether `error` is this card's problem again (what failed was run again and failed the
    /// same way): the card stays as it is rather than being made again.
    pub fn is_about(&self, error: &str) -> bool {
        errors::explain(error).is_some_and(|p| p == self.problem)
    }

    /// What it is about failed the same way again: Try again is offered again.
    pub fn failed_again(&mut self) {
        self.retried = false;
    }

    /// Try again was asked for: it isn't offered again until what it ran fails again (it
    /// might run an operation twice that worked).
    pub fn retried(&mut self) {
        self.retried = true;
    }

    /// Whether Try again can be offered, as far as the card knows.
    pub fn can_retry(&self) -> bool {
        !self.retried
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
        self.start(ctx, move || {
            Reply::Undone(fix.undo(&dir).map_err(|e| (e, fix)))
        });
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
