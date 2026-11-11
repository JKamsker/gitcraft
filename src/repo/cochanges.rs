//! Cochange suggestions own their request lifetime, ranking and dismissed evidence.
use super::{Action, RepoTab, saved_views::reader::Reader};
use crate::git::investigations::cochange::{self, Hint, History};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[cfg(test)]
#[path = "cochange_owner/lifecycle_tests.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "cochange_owner/tests.rs"]
mod tests;
#[path = "cochange_owner/view.rs"]
mod view;

#[path = "cochange_owner/requests.rs"]
mod requests;
use requests::{Request, Requests};
struct Completion {
    request: Request,
    result: Result<History, String>,
}
/// Effects contain immutable inputs; the owner alone accepts completions.
enum Effect {
    Read {
        request: Request,
        path: PathBuf,
        head: String,
    },
    None,
}

#[derive(Default)]
pub(super) struct State {
    head: String,
    repository: PathBuf,
    requests: Requests,
    reader: Reader<Completion>,
    history: Option<History>,
    error: Option<String>,
    ranked_for: Option<(BTreeSet<String>, BTreeSet<String>)>,
    hints: Vec<Hint>,
    dismissed: BTreeSet<(String, String)>,
}
impl State {
    pub(super) fn invalidate_context(&mut self) {
        self.refresh();
        self.history = None;
        self.error = None;
        self.hints.clear();
        self.dismissed.clear();
        self.ranked_for = None;
    }

    /// Retire reads on hiding/suspending without discarding completed history.
    pub(super) fn suspend(&mut self) {
        self.reader.cancel();
        self.requests.retire();
        if self.history.is_none() {
            self.head.clear();
        }
    }

    fn refresh(&mut self) {
        self.suspend();
        self.head.clear();
    }

    fn context(&mut self, path: &std::path::Path, head: &str) -> Effect {
        if self.head == head && self.repository == path {
            return Effect::None;
        }
        self.suspend();
        self.head = head.to_owned();
        self.repository = path.to_owned();
        self.history = None;
        self.error = None;
        self.hints.clear();
        self.dismissed.clear();
        self.ranked_for = None;
        if !crate::git::investigations::oid(head) {
            return Effect::None;
        }
        let Some(request) = self.requests.start() else {
            return Effect::None;
        };
        Effect::Read {
            request,
            path: path.to_owned(),
            head: head.to_owned(),
        }
    }

    fn complete(&mut self, completion: Completion) {
        if !self.requests.finish(completion.request) {
            return;
        }
        self.requests.retire();
        match completion.result {
            Ok(history) => {
                self.history = Some(history);
                self.error = None;
                self.ranked_for = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn rank(&mut self, staged: BTreeSet<String>, changed: BTreeSet<String>) {
        if staged.is_empty() {
            self.hints.clear();
            self.ranked_for = None;
            return;
        }
        let key = (staged, changed);
        if self.ranked_for.as_ref() != Some(&key)
            && let Some(history) = &self.history
        {
            self.hints = cochange::rank(history, &key.0, &key.1);
            self.ranked_for = Some(key);
        }
    }

    fn read(&mut self, effect: Effect, ctx: &egui::Context) {
        if let Effect::Read {
            request,
            path,
            head,
        } = effect
        {
            self.reader.queue(move |active| {
                Ok(Completion {
                    request,
                    result: cochange::load(&path, &head, active),
                })
            });
        }
        if let Some(result) = self.reader.poll(ctx) {
            match result {
                Ok(completion) => self.complete(completion),
                Err(error) => {
                    if let Some(request) = self.requests.pending() {
                        self.complete(Completion {
                            request,
                            result: Err(error),
                        });
                    }
                }
            }
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.suspend();
    }
}
impl RepoTab {
    pub(super) fn poll_cochanges(&mut self, ctx: &egui::Context) {
        self.notice_connections(ctx);
        self.cochanges.reader.bind(self.tx.cancel.clone());
        if !self.shown || self.sleeping() {
            self.cochanges.suspend();
            return;
        }
        // Reconcile context before accepting even an already queued answer.
        let effect = self.cochanges.context(&self.path, &self.refs.head);
        self.cochanges.read(effect, ctx);
        let staged = self.work.staged.iter().map(|f| f.path.clone()).collect();
        let changed = self
            .work
            .staged
            .iter()
            .chain(&self.work.unstaged)
            .chain(&self.work.conflicted)
            .map(|f| f.path.clone())
            .collect();
        self.cochanges.rank(staged, changed);
    }
}
