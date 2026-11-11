//! A single scroll document with the same patch builder and operation runner as file diffs.
use crate::{
    git::{self, Area, DiffOptions},
    repo::{Action, DiffKey, RepoTab, diff},
    theme::Tokens,
};
use egui::{RichText, Ui};

/// Reload behavior is explicit: historical objects stay pinned; live sources enumerate anew.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReviewSource {
    Working,
    Commit(String),
    Comparison { from: String, to: Option<String> },
    PullRequest { number: u64, url: String },
}

#[derive(Clone)]
pub(crate) enum ReviewAct {
    Open(ReviewSource),
    Close,
    Reload,
    Pick {
        request: u64,
        file: usize,
        hunk: usize,
        pick: diff::Pick,
    },
}
#[derive(Default)]
pub(crate) struct ReviewState {
    pub open: bool,
    request: u64,
    source: Option<ReviewSource>,
    cancel: crate::tasks::Scope,
    identity: String,
    keys: Vec<DiffKey>,
    data: Option<Result<Vec<diff::DiffData>, String>>,
}

impl RepoTab {
    pub(in crate::repo) fn review_all_button(
        &self,
        ui: &mut Ui,
        source: ReviewSource,
        acts: &mut Vec<Action>,
    ) {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-review-all-changes-243201",
            ))
            .clicked()
        {
            acts.push(Action::Review(ReviewAct::Open(source)));
        }
    }
    pub(in crate::repo) fn review_act(&mut self, act: ReviewAct, ctx: &egui::Context) {
        match act {
            ReviewAct::Close => {
                self.review_document.open = false;
                self.review_document.invalidate();
            }
            ReviewAct::Reload => self.load_review_document(ctx),
            ReviewAct::Open(source) => {
                if self.conflict.is_some() || self.rebase.is_some() {
                    return;
                }
                self.file_comparison = None;
                self.symbols.open = false;
                self.dossier.open = false;
                self.review_document.source = Some(source);
                self.review_document.open = true;
                self.load_review_document(ctx);
            }
            ReviewAct::Pick {
                request,
                file,
                hunk,
                pick,
            } => {
                if self.busy_op.is_some() || self.review_document.request != request {
                    return;
                }
                let Some(key) = self.review_document.keys.get(file).cloned() else {
                    return;
                };
                let Some(Ok(data)) = &self.review_document.data else {
                    return;
                };
                let Some(data) = data.get(file) else {
                    return;
                };
                if !picks(&key, data.partial, data.revertable).contains(&pick) {
                    return;
                }
                let Some(patch) = diff::partial_patch(
                    &data.header,
                    &data.lines,
                    &diff::hunk_changes(&data.lines, hunk),
                    pick,
                ) else {
                    return;
                };
                let source = data.source;
                let path = self.path.clone();
                let dark = Tokens::get(ctx).dark();
                let opts = DiffOptions {
                    no_textconv: true,
                    ..Default::default()
                };
                self.then_or(ctx, true, Err, move || {
                    let now = diff::load(&path,&key,opts,dark)?;
                    if now.source != source { return Err("Changes moved since this page was loaded; reload before applying a hunk".to_owned()); }
                    git::save_patch_for(&path,&patch)
                }, move |tab, saved, ctx| {
                    tab.finish_review_hunk(request, pick, saved, ctx);
                });
            }
        }
    }
    fn finish_review_hunk(
        &mut self,
        request: u64,
        pick: diff::Pick,
        saved: Result<String, String>,
        ctx: &egui::Context,
    ) {
        // A saved patch belongs to this document request, including its feedback.
        if !self.review_document.owns(request) {
            return;
        }
        if let Err(error) = &saved {
            self.review_document.data = Some(Err(format!("Hunk refused: {error}")));
            return;
        }
        self.apply_picked(pick, "Review hunk", saved, ctx);
        let message = if self.busy_op.is_some() {
            "Hunk submitted. Reload after the operation finishes.".to_owned()
        } else {
            format!(
                "Hunk refused: {}",
                self.status
                    .as_deref()
                    .map(|s| s.0.as_str())
                    .unwrap_or("operation could not start")
            )
        };
        self.review_document.data = Some(Err(message));
    }
    /// Called before a PR details refresh: no stale membership or head can remain actionable.
    pub(in crate::repo) fn invalidate_pr_document(&mut self) {
        if matches!(
            self.review_document.source,
            Some(ReviewSource::PullRequest { .. })
        ) {
            self.review_document.invalidate();
            self.review_document.data = Some(Err(
                "Pull request is refreshing; reload after its current changes arrive".into(),
            ));
        }
    }
    pub(in crate::repo) fn refresh_pr_document(&mut self, ctx: &egui::Context) {
        if self.review_document.open
            && matches!(
                self.review_document.source,
                Some(ReviewSource::PullRequest { .. })
            )
        {
            self.load_review_document(ctx);
        }
    }
    fn load_review_document(&mut self, ctx: &egui::Context) {
        self.review_document.invalidate();
        let request = self.review_document.request;
        let Some(source) = self.review_document.source.clone() else {
            return;
        };
        // Capture only an accepted, consistent PR snapshot. Other live sources enumerate on the worker.
        let pr = match &source {
            ReviewSource::PullRequest { number, url } => {
                let snapshot = self.pr_view.as_ref().and_then(|view| {
                    let details = view.details.as_ref()?.as_ref().ok()?;
                    let files = view.files.as_ref()?.as_ref().ok()?;
                    (view.files_request == self.pr_requests
                        && view.number == *number
                        && details.url == *url
                        && details.head_oid == files.to)
                        .then(|| (files.from.clone(), files.to.clone(), files.files.clone()))
                });
                if snapshot.is_none() {
                    self.review_document.data = Some(Err("Current pull request details and changes must finish loading before review".into()));
                    return;
                }
                snapshot
            }
            _ => None,
        };
        let path = self.path.clone();
        let dark = Tokens::get(ctx).dark();
        let cancel = self.review_document.cancel.0.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                let (keys, identity) = match source {
                    ReviewSource::Working => {
                        let work = git::status(&path)?;
                        (
                            working_keys(work),
                            "Working tree and index · current membership".into(),
                        )
                    }
                    ReviewSource::Commit(hash) => {
                        let detail = git::commit_detail(&path, &hash)?;
                        (
                            detail
                                .files
                                .into_iter()
                                .map(|f| DiffKey::Commit {
                                    hash: detail.hash.clone(),
                                    path: f.path,
                                })
                                .collect(),
                            format!("Commit {}", detail.hash),
                        )
                    }
                    ReviewSource::Comparison { from, to } => {
                        let files = git::diff_files(&path, &from, to.as_deref())?;
                        (
                            range_keys(&from, to.as_deref(), files),
                            format!(
                                "Comparison {from} → {}",
                                to.as_deref().unwrap_or("working tree")
                            ),
                        )
                    }
                    ReviewSource::PullRequest { number, .. } => {
                        let (from, to, files) = pr.ok_or("PR snapshot missing")?;
                        (
                            range_keys(&from, Some(&to), files),
                            format!("Pull request #{number} · {from} → {to}"),
                        )
                    }
                };
                if keys.len() > 500 {
                    return Err(
                        "This document has more than 500 files; select a narrower comparison"
                            .into(),
                    );
                }
                let mut rows = 0;
                let mut bytes = 0;
                let mut document = Vec::with_capacity(keys.len());
                for key in &keys {
                    if cancel.is_cancelled() {
                        return Err("Review loading cancelled".into());
                    }
                    let data = diff::load(
                        &path,
                        key,
                        DiffOptions {
                            no_textconv: true,
                            ..Default::default()
                        },
                        dark,
                    )?;
                    rows += data.lines.len();
                    bytes +=
                        data.header.len() + data.lines.iter().map(|l| l.text.len()).sum::<usize>();
                    check_budget(rows, bytes)?;
                    document.push(data);
                }
                Ok((keys, identity, document))
            },
            move |tab, result, _| {
                if tab.review_document.owns(request) {
                    tab.review_document.data = Some(result.map(|(keys, identity, data)| {
                        tab.review_document.keys = keys;
                        tab.review_document.identity = identity;
                        data
                    }));
                }
            },
        );
    }
}

impl ReviewState {
    fn owns(&self, request: u64) -> bool {
        self.open && self.request == request
    }
    fn invalidate(&mut self) {
        self.cancel.0.cancel();
        self.cancel = Default::default();
        self.request += 1;
        self.keys.clear();
        self.data = None;
        self.identity.clear();
    }
}
#[cfg(test)]
#[path = "continuous/fluent_review_tests.rs"]
mod fluent_review_tests;
#[cfg(test)]
#[path = "continuous/tests.rs"]
mod tests;

mod presentation;

mod document;
use document::{check_budget, file_intro, picks, range_keys, visible_rows, working_keys};
