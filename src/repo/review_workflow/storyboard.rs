//! Author-arranged, revision-pinned review cards; immutable originals remain available.
use super::{sharing, store};
use crate::{
    git,
    pull_request::{PrDetails, PrFiles},
    repo::{Action, RepoTab, file_view::FileTab},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Serialize, Deserialize, Default)]
struct Board {
    cards: Vec<Card>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Card {
    phase: String,
    text: String,
    base: String,
    head: String,
    anchor: Anchor,
}
#[derive(Clone, Serialize, Deserialize)]
enum Anchor {
    Code {
        commit: String,
        path: String,
        line: u64,
        blob: String,
    },
    Test {
        commit: String,
        tree: String,
        command: String,
        verdict: String,
    },
}
#[derive(Clone)]
struct Shown {
    card: Card,
    target: Option<(String, String, u64)>,
    stale: Option<String>,
}
#[derive(Clone, Default)]
pub(crate) struct CardForm {
    phase: usize,
    text: String,
    path: String,
    commit: String,
    line: String,
    test: bool,
}
#[derive(Default)]
pub(crate) struct Storyboard {
    request: u64,
    rows: Option<Result<Vec<Shown>, String>>,
    form: CardForm,
    remote: String,
    imported: bool,
    cached: Option<Vec<Card>>,
}
impl Storyboard {
    pub(in crate::repo) fn invalidate(&mut self) {
        self.request += 1;
        self.rows = None;
    }
    fn accept(&mut self, request: u64, result: Result<Vec<Shown>, String>) {
        if request != self.request {
            return;
        }
        if let Ok(rows) = &result {
            self.cached = Some(rows.iter().map(|r| r.card.clone()).collect());
        }
        self.rows = Some(result);
    }
}
#[derive(Clone)]
pub(crate) enum StoryAct {
    Load,
    Fetch {
        remote: String,
        head: String,
    },
    Publish {
        remote: String,
        head: String,
    },
    OpenCode {
        commit: String,
        path: String,
        line: u64,
    },
    Add {
        form: CardForm,
        head: String,
    },
    Move {
        index: usize,
        up: bool,
        head: String,
    },
    Remove {
        index: usize,
        head: String,
    },
}
fn current_files(request: u64, files_request: u64, head: &str, files_head: &str) -> bool {
    request == files_request && head == files_head
}
impl RepoTab {
    pub(in crate::repo) fn refresh_storyboard(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.pr_view else {
            return;
        };
        let (Some(Ok(details)), Some(Ok(files))) = (&view.details, &view.files) else {
            return;
        };
        if !current_files(
            self.pr_requests,
            view.files_request,
            &details.head_oid,
            &files.to,
        ) {
            return;
        }
        let Some(cards) = view.storyboard.cached.clone() else {
            return;
        };
        view.storyboard.invalidate();
        let request = view.storyboard.request;
        let pr_request = self.pr_requests;
        let (repo, files) = (self.path.clone(), files.clone());
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                cards
                    .into_iter()
                    .map(|c| show_card(&repo, c, &files))
                    .collect()
            },
            move |tab, result, _| {
                if tab.pr_requests != pr_request {
                    return;
                }
                if let Some(view) = tab
                    .pr_view
                    .as_mut()
                    .filter(|v| v.storyboard.request == request)
                {
                    view.storyboard.accept(request, result);
                }
            },
        );
    }
    pub(in crate::repo) fn story_act(&mut self, act: StoryAct, ctx: &egui::Context) {
        if let StoryAct::OpenCode { commit, path, line } = act {
            self.review_document.open = false;
            self.dossier.close();
            self.symbols.close();
            self.file_comparison = None;
            // A line card describes the full pinned file, including unchanged/out-of-hunk
            // lines. Its logical line stays on this view until accepted file data is painted.
            self.open_file_view(path, Some(commit), FileTab::File, ctx);
            if let Some(view) = &mut self.file_view {
                view.goto_line
                    .set(usize::try_from(line).ok().filter(|n| *n > 0));
                view.marked_line = usize::try_from(line).ok().filter(|n| *n > 0);
            }
            return;
        }
        if let StoryAct::Fetch { remote, head } | StoryAct::Publish { remote, head } = &act {
            let publish = matches!(act, StoryAct::Publish { .. });
            let (Some(v), Some(observed)) = (
                &mut self.pr_view,
                self.remotes.iter().find(|r| &r.name == remote).cloned(),
            ) else {
                self.status = Some(crate::repo::Notice::plain((
                    "Choose a configured storyboard sharing remote".into(),
                    true,
                )));
                return;
            };
            let (Some(Ok(d)), Some(Ok(f))) = (&v.details, &v.files) else {
                return;
            };
            if !current_files(self.pr_requests, v.files_request, &d.head_oid, &f.to)
                || &d.head_oid != head
            {
                self.status = Some(crate::repo::Notice::plain((
                    "Pull request changed; refresh the storyboard".into(),
                    true,
                )));
                return;
            }
            if publish
                && !d
                    .extras
                    .viewer
                    .as_deref()
                    .is_some_and(|viewer| viewer.eq_ignore_ascii_case(&d.author))
            {
                self.status = Some(crate::repo::Notice::plain((
                    "Only the pull request author publishes its storyboard".into(),
                    true,
                )));
                return;
            }
            let (d, f, repo, creds) = (
                d.clone(),
                f.clone(),
                self.path.clone(),
                self.credentials.clone(),
            );
            v.storyboard.request += 1;
            let request = v.storyboard.request;
            v.storyboard.rows = None;
            let number = d.number;
            let pr_request = self.pr_requests;
            self.then_or(
                ctx,
                true,
                Err,
                move || {
                    let target = sharing::Target::capture(&repo, &observed)?;
                    target.guard(&repo)?;
                    if publish {
                        let board = store::load::<Board>(&repo, "storyboard", &key(&d))?
                            .unwrap_or_default();
                        publish_board(&repo, &key(&d), &board, &target, creds.as_ref())?;
                        Ok((read(&repo, &d, &f)?, false))
                    } else {
                        let rows = fetch_board(&repo, &key(&d), &target, creds.as_ref())?
                            .cards
                            .into_iter()
                            .map(|c| show_card(&repo, c, &f))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok((rows, true))
                    }
                },
                move |tab, result, _| {
                    if tab.pr_requests != pr_request {
                        return;
                    }
                    if let Some(v) = tab
                        .pr_view
                        .as_mut()
                        .filter(|v| v.number == number && v.storyboard.request == request)
                    {
                        match result {
                            Ok((rows, imported)) => {
                                v.storyboard.imported = imported;
                                v.storyboard.accept(request, Ok(rows));
                            }
                            Err(e) => v.storyboard.rows = Some(Err(e)),
                        }
                    }
                },
            );
            return;
        }
        let Some(v) = &mut self.pr_view else { return };
        let (Some(Ok(d)), Some(Ok(f))) = (&v.details, &v.files) else {
            return;
        };
        if !current_files(self.pr_requests, v.files_request, &d.head_oid, &f.to) {
            self.status = Some(crate::repo::Notice::plain((
                "Pull request files are refreshing; wait before arranging its storyboard".into(),
                true,
            )));
            return;
        }
        let (d, f, path, request) = (
            d.clone(),
            f.clone(),
            self.path.clone(),
            v.storyboard.request + 1,
        );
        v.storyboard.request = request;
        let mutation = !matches!(act, StoryAct::Load);
        if mutation && v.storyboard.imported {
            self.status = Some(crate::repo::Notice::plain((
                "Load your local storyboard before editing".into(),
                true,
            )));
            return;
        }
        v.storyboard.imported = false;
        if mutation
            && !d
                .extras
                .viewer
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(&d.author))
        {
            self.status = Some(crate::repo::Notice::plain((
                "Only the pull request author arranges this storyboard".into(),
                true,
            )));
            return;
        }
        if mutation {
            v.storyboard.rows = None;
        }
        let pr_request = self.pr_requests;
        self.then_or(
            ctx,
            mutation,
            Err,
            move || {
                if mutation {
                    let mut board =
                        store::load::<Board>(&path, "storyboard", &key(&d))?.unwrap_or_default();
                    match act {
                        StoryAct::Add { form, head } if head == f.to => {
                            board.cards.push(card(&path, &d, &f, form)?)
                        }
                        StoryAct::Remove { index, head }
                            if head == f.to && index < board.cards.len() =>
                        {
                            board.cards.remove(index);
                        }
                        StoryAct::Move { index, up, head }
                            if head == f.to && index < board.cards.len() =>
                        {
                            let other = if up {
                                index.checked_sub(1)
                            } else {
                                index.checked_add(1)
                            };
                            if let Some(other) = other.filter(|n| *n < board.cards.len()) {
                                board.cards.swap(index, other);
                            }
                        }
                        _ => return Err("The storyboard selection is stale; reload it".into()),
                    }
                    preserve(&path, &key(&d), &board)?;
                }
                read(&path, &d, &f)
            },
            move |tab, result, _| {
                if tab.pr_requests != pr_request {
                    return;
                }
                if let Some(v) = tab
                    .pr_view
                    .as_mut()
                    .filter(|v| v.storyboard.request == request)
                {
                    v.storyboard.accept(request, result);
                }
            },
        );
    }
}
#[cfg(test)]
#[path = "storyboard/fluent_review_r1_tests.rs"]
mod fluent_review_r1_tests;
#[cfg(test)]
#[path = "storyboard/fluent_review_tests.rs"]
mod fluent_review_tests;
#[cfg(test)]
#[path = "storyboard/tests.rs"]
mod tests;

mod anchors;
use anchors::{card, file_blob, key, oid, read, show_card};

mod publication;
#[cfg(test)]
use publication::{cached_ref, shared_ref};
use publication::{fetch_board, preserve, publish_board};

mod presentation;
pub(in crate::repo) use presentation::show;
#[cfg(test)]
use presentation::{display_phase, display_verdict};
