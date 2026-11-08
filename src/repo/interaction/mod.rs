//! Find the interaction: two branches pass alone but fail together; subsets of their commits
//! are tested in a scratch worktree (delta debugging) until it can say which commits fail
//! together. The search runs in the background, shown in a window with its progress and Stop.

mod report;
#[cfg(test)]
mod tests;
mod window;

use std::time::{Duration, Instant};

use super::messages::Msg;
use super::{Action, Dialog, RepoTab};
use crate::git::{self, Interaction, InteractionSpec, RunStop, SearchProgress};

/// What the views ask of a search.
#[derive(Clone, Debug)]
pub(crate) enum InteractionAct {
    /// Open the dialog for the checked-out branch and `other`.
    Open {
        other: String,
    },
    Start(InteractionSpec),
    Stop,
    Close,
}

/// A search going on or done, until its window is closed.
pub(super) struct Search {
    id: u64,
    spec: InteractionSpec,
    last: Option<SearchProgress>,
    started: Instant,
    stop: RunStop,
    ended: Option<Result<Interaction, String>>,
}

impl Drop for Search {
    fn drop(&mut self) {
        if self.ended.is_none() {
            self.stop.stop();
        }
    }
}

#[derive(Default)]
pub(super) struct Interactions {
    search: Option<Search>,
    searches: u64,
}

impl Interactions {
    pub(super) fn going(&self) -> bool {
        self.search.as_ref().is_some_and(|s| s.ended.is_none())
    }
}

/// The menu entry that opens the dialog for the checked-out branch and `other`.
pub(crate) fn interaction_item(ui: &mut egui::Ui, acts: &mut Vec<Action>, other: &str) {
    if crate::menu_item::MenuItem::new("Find the interaction…")
        .icon("command")
        .show(ui)
        .on_hover_text(
            "When this branch and the checked-out one pass alone but fail together: tests \
             subsets of their commits until it finds those that fail together",
        )
        .clicked()
    {
        acts.push(Action::Interaction(InteractionAct::Open {
            other: other.to_owned(),
        }));
        ui.close();
    }
}

impl RepoTab {
    pub(super) fn interaction_act(&mut self, act: InteractionAct, ctx: &egui::Context) {
        match act {
            InteractionAct::Open { other } => {
                let a = self
                    .refs
                    .current
                    .clone()
                    .unwrap_or_else(|| "HEAD".to_owned());
                self.dialog = Some(Dialog::FindInteraction {
                    spec: InteractionSpec {
                        a,
                        b: other,
                        command: self.test_lane.command().unwrap_or_default().to_owned(),
                        budget: git::DEFAULT_BUDGET,
                        reruns: 1,
                    },
                    focus: true,
                });
            }
            InteractionAct::Start(spec) => self.start_search(spec, ctx),
            InteractionAct::Stop => {
                if let Some(search) = &self.interactions.search {
                    search.stop.stop();
                }
            }
            InteractionAct::Close => {
                if !self.interactions.going() {
                    self.interactions.search = None;
                }
            }
        }
    }

    /// The palette's entries: the checked-out branch with each other local branch.
    pub(super) fn interaction_palette_items(&self) -> Vec<(String, String, Action)> {
        let Some(current) = self.refs.current.as_deref() else {
            return Vec::new();
        };
        if self.interactions.going() {
            return Vec::new();
        }
        self.refs
            .local
            .iter()
            .filter(|b| b.name != current)
            .map(|b| {
                (
                    format!("Find the interaction of {current} and {}…", b.name),
                    "interaction combination breaks ddmin delta debugging".to_owned(),
                    Action::Interaction(InteractionAct::Open {
                        other: b.name.clone(),
                    }),
                )
            })
            .collect()
    }

    fn start_search(&mut self, spec: InteractionSpec, ctx: &egui::Context) {
        if self.interactions.going() {
            self.status = Some(("A search is going on already".to_owned(), true));
            return;
        }
        self.interactions.searches += 1;
        let id = self.interactions.searches;
        let stop = RunStop::default();
        self.interactions.search = Some(Search {
            id,
            spec: spec.clone(),
            last: None,
            started: Instant::now(),
            stop: stop.clone(),
            ended: None,
        });
        let path = self.path.clone();
        let tx = self.tx.clone();
        let wake = ctx.clone();
        crate::connections::spawn(move || {
            let tell = |p: SearchProgress| {
                let _ = tx.send(Msg::Then(Box::new(move |tab, _| {
                    tab.search_progress(id, p)
                })));
                wake.request_repaint();
            };
            let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                git::find_interaction(&path, &spec, &stop, &tell)
            }))
            .unwrap_or_else(|_| Err("Internal error: the search stopped".to_owned()));
            let _ = tx.send(Msg::Then(Box::new(move |tab, _| {
                tab.search_ended(id, found)
            })));
            wake.request_repaint();
        });
    }

    fn search_progress(&mut self, id: u64, p: SearchProgress) {
        if let Some(search) = self.interactions.search.as_mut().filter(|s| s.id == id) {
            search.last = Some(p);
        }
    }

    fn search_ended(&mut self, id: u64, found: Result<Interaction, String>) {
        let name = self.name.clone();
        let Some(search) = self.interactions.search.as_mut().filter(|s| s.id == id) else {
            return;
        };
        let stopped = matches!(&found, Err(e) if e == "stopped");
        if search.started.elapsed() > Duration::from_secs(4) && !stopped {
            let failed = found.is_err();
            self.finished.push((
                format!(
                    "{name} · the search of {} and {} is done",
                    search.spec.a, search.spec.b
                ),
                failed,
            ));
        }
        search.ended = Some(found);
    }
}
