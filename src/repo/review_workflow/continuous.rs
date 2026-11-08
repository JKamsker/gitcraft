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
        if ui.button("Review all changes").clicked() {
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
                    .as_ref()
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
    pub(in crate::repo) fn review_document_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.heading("Review all changes");
            if ui.button("Reload").clicked() {
                acts.push(Action::Review(ReviewAct::Reload));
            }
            if ui.button("Close").clicked() {
                acts.push(Action::Review(ReviewAct::Close));
            }
        });
        if let Some(view) = &mut self.pr_view
            && let Some(draft) = self.pr_drafts.get_mut(&view.number)
            && draft.target.is_some()
        {
            egui::Panel::bottom("continuous-line-comment")
                .resizable(true)
                .show(ui, |ui| {
                    super::super::pr_review::line_panel(ui, view, draft, None, acts);
                });
        }
        let state = &self.review_document;
        ui.weak(&state.identity);
        let Some(data) = &state.data else {
            ui.add(crate::spinner::Spinner::new());
            return;
        };
        let data = match data {
            Ok(data) => data,
            Err(e) => {
                ui.label(e);
                return;
            }
        };
        let idle = self.busy_op.is_none();
        egui::ScrollArea::both()
            .id_salt(("continuous-review", state.request))
            .show(ui, |ui| {
                for (file, (key, data)) in state.keys.iter().zip(data).enumerate() {
                    egui::CollapsingHeader::new(format!(
                        "{}{}",
                        key.path(),
                        match key {
                            DiffKey::Work(f) if f.area == Area::Staged => " · staged",
                            _ => "",
                        }
                    ))
                    .id_salt(file)
                    .default_open(true)
                    .show(ui, |ui| {
                        file_intro(ui, key, data, acts);
                        let review = self.diff_review(key);
                        // Clip fixed-height rows before building widgets. Offscreen hunks do no layout work.
                        let row_height = 22.0;
                        let top = ui.cursor().top();
                        let visible = visible_rows(
                            data.lines.len(),
                            top,
                            ui.clip_rect().top(),
                            ui.clip_rect().bottom(),
                            row_height,
                        );
                        ui.add_space(visible.start as f32 * row_height);
                        for hunk in visible.clone() {
                            let line = &data.lines[hunk];
                            let row_top = ui.cursor().top();
                            if line.kind == diff::Kind::Hunk {
                                ui.horizontal(|ui| {
                                    ui.monospace(&line.text);
                                    for &pick in picks(key, data.partial, data.revertable) {
                                        if ui
                                            .add_enabled(idle, egui::Button::new(pick.verb()))
                                            .clicked()
                                        {
                                            acts.push(Action::Review(ReviewAct::Pick {
                                                request: state.request,
                                                file,
                                                hunk,
                                                pick,
                                            }));
                                        }
                                    }
                                    if ui.small_button("Open file diff").clicked() {
                                        acts.push(Action::OpenDiff(key.clone()));
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    if let Some(review) = &review
                                        && let Some((side, number)) =
                                            super::super::pr_review::target_of(
                                                line.kind, line.old, line.new, None,
                                            )
                                        && ui
                                            .small_button(format!("{}", number))
                                            .on_hover_text("Comment on this line")
                                            .clicked()
                                    {
                                        acts.push(Action::Pr(
                                            super::super::pr_review::PrAct::OpenLine(
                                                super::super::pr_review::LineTarget {
                                                    path: review.path.clone(),
                                                    line: number,
                                                    side,
                                                },
                                            ),
                                        ));
                                    }
                                    let color = match line.kind {
                                        diff::Kind::Add => egui::Color32::from_rgb(85, 185, 120),
                                        diff::Kind::Del => egui::Color32::from_rgb(230, 110, 110),
                                        _ => ui.visuals().text_color(),
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!(
                                                "{:>5} {:>5} {}",
                                                line.old.map(|n| n.to_string()).unwrap_or_default(),
                                                line.new.map(|n| n.to_string()).unwrap_or_default(),
                                                line.text
                                            ))
                                            .monospace()
                                            .color(color),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                    );
                                });
                            }
                            let used = ui.cursor().top() - row_top;
                            if used < row_height {
                                ui.add_space(row_height - used);
                            }
                        }
                        ui.add_space(
                            data.lines.len().saturating_sub(visible.end) as f32 * row_height,
                        );
                    });
                }
            });
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
fn working_keys(work: git::WorkStatus) -> Vec<DiffKey> {
    work.unstaged
        .into_iter()
        .chain(work.staged)
        .chain(work.conflicted)
        .map(DiffKey::Work)
        .collect()
}
fn range_keys(from: &str, to: Option<&str>, files: Vec<git::ChangedFile>) -> Vec<DiffKey> {
    files
        .into_iter()
        .map(|f| DiffKey::Range {
            from: from.into(),
            to: to.map(str::to_owned),
            path: f.path,
        })
        .collect()
}
fn check_budget(rows: usize, bytes: usize) -> Result<(), String> {
    if rows > 250_000 || bytes > 32 * 1024 * 1024 {
        Err("Review exceeds 250,000 lines or 32 MiB; select a narrower comparison. No partial document was accepted.".into())
    } else {
        Ok(())
    }
}
fn visible_rows(
    count: usize,
    top: f32,
    clip_top: f32,
    clip_bottom: f32,
    height: f32,
) -> std::ops::Range<usize> {
    let start = ((clip_top - top).max(0.0) / height).floor() as usize;
    let end = ((clip_bottom - top).max(0.0) / height).ceil() as usize;
    start.min(count)..end.saturating_add(1).min(count).max(start.min(count))
}
fn file_intro(
    ui: &mut Ui,
    key: &DiffKey,
    data: &diff::DiffData,
    acts: &mut Vec<Action>,
) -> egui::Response {
    let response = ui.button("Open file diff");
    if response.clicked() {
        acts.push(Action::OpenDiff(key.clone()));
    }
    metadata_ui(ui, key, data);
    response
}
fn metadata_ui(ui: &mut Ui, key: &DiffKey, data: &diff::DiffData) {
    if let DiffKey::Work(file) = key {
        ui.label(format!("Status {} · {:?}", file.code, file.area));
        if let Some(old) = &file.old_path {
            ui.label(format!("{old} → {}", file.path));
        }
    }
    if !data.header.is_empty() {
        ui.monospace(&data.header);
    }
    if data.lines.is_empty() {
        ui.weak(if data.binary.is_some() || data.image.is_some() || data.lfs.is_some() {
            "Binary, image or LFS content: open the file comparison to inspect both versions."
        } else {
            "No textual hunks. Review the metadata above for mode, rename, copy or empty-file changes."
        });
    }
}
fn picks(key: &DiffKey, partial: bool, revertable: bool) -> &'static [diff::Pick] {
    match key {
        DiffKey::Work(f) if partial && f.area == Area::Staged => &[diff::Pick::Unstage],
        DiffKey::Work(_) if partial => &[diff::Pick::Stage, diff::Pick::Discard],
        DiffKey::Commit { .. } if revertable => &[diff::Pick::Revert],
        _ => &[],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn work_file(path: &str, area: Area) -> git::StatusFile {
        git::StatusFile {
            path: path.into(),
            old_path: None,
            code: 'M',
            area,
        }
    }
    #[test]
    fn r2_pure_fresh_membership_staging_and_close_invalidate_old_ownership() {
        let old = working_keys(git::WorkStatus {
            unstaged: vec![work_file("a", Area::Unstaged)],
            ..Default::default()
        });
        let fresh = working_keys(git::WorkStatus {
            staged: vec![work_file("a", Area::Staged)],
            unstaged: vec![work_file("b", Area::Untracked)],
            ..Default::default()
        });
        assert_eq!(old.len(), 1);
        assert_eq!(fresh.len(), 2);
        assert!(
            fresh
                .iter()
                .any(|k| matches!(k, DiffKey::Work(f) if f.path == "a" && f.area == Area::Staged))
        );
        let mut state = ReviewState {
            open: true,
            ..Default::default()
        };
        let token = state.cancel.0.clone();
        state.invalidate();
        assert!(token.is_cancelled());
        assert!(!state.owns(0));
        assert!(state.owns(1));
        state.open = false;
        assert!(!state.owns(1));
    }
    #[test]
    fn r2_pure_stale_hunk_refusal_and_late_callback_preserve_new_document() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/r2-pure-review".into(), &ctx);
        tab.review_document.open = true;
        tab.finish_review_hunk(0, diff::Pick::Stage, Err("source moved".into()), &ctx);
        assert!(
            matches!(&tab.review_document.data, Some(Err(e)) if e == "Hunk refused: source moved")
        );
        tab.review_document.invalidate();
        tab.review_document.data = Some(Ok(Vec::new()));
        tab.finish_review_hunk(0, diff::Pick::Stage, Ok("never-run".into()), &ctx);
        assert!(matches!(&tab.review_document.data, Some(Ok(_))));
        assert!(tab.busy_op.is_none());
    }
    #[test]
    fn r2_pure_row_budget_and_viewport_bound_large_documents() {
        assert!(check_budget(250_001, 0).is_err());
        assert!(check_budget(0, 32 * 1024 * 1024 + 1).is_err());
        assert!(check_budget(250_000, 32 * 1024 * 1024).is_ok());
        assert_eq!(
            visible_rows(50_000, 0.0, 22000.0, 22440.0, 22.0),
            1000..1021
        );
        assert_eq!(visible_rows(3, 500.0, 0.0, 300.0, 22.0), 0..1);
    }
    #[test]
    fn r2_pure_metadata_only_sections_describe_changes_and_open_comparison() {
        for header in [
            "old mode 100644\nnew mode 100755",
            "similarity index 100%\nrename from old.txt\nrename to new.txt",
            "similarity index 100%\ncopy from old.txt\ncopy to new.txt",
            "new file mode 100644",
            "deleted file mode 100644",
            "Binary files a/picture and b/picture differ",
        ] {
            let ctx = egui::Context::default();
            let key = DiffKey::Work(git::StatusFile {
                old_path: Some("old.txt".into()),
                ..work_file("new.txt", Area::Staged)
            });
            let mut data = diff::DiffData::default();
            data.header = header.into();
            let mut acts = Vec::new();
            let mut rect = egui::Rect::NOTHING;
            let mut render = |events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        rect = file_intro(ui, &key, &data, &mut acts).rect;
                    },
                );
                output.textures_delta.clear();
                output
            };
            let output = render(Vec::new());
            let text = output
                .shapes
                .iter()
                .filter_map(|s| {
                    if let egui::epaint::Shape::Text(t) = &s.shape {
                        Some(t.galley.text())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains(header), "{text}");
            assert!(text.contains("old.txt → new.txt"));
            assert!(text.contains("No textual hunks"));
            drop(render);
            let at = rect.center();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 600.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(at),
                        egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                        egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    file_intro(ui, &key, &data, &mut acts);
                },
            );
            output.textures_delta.clear();
            assert!(
                acts.iter()
                    .any(|a| matches!(a, Action::OpenDiff(k) if k == &key))
            );
        }
    }
    #[test]
    fn a_document_hunk_stages_one_file_without_staging_its_neighbor() -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("continuous-hunk")?;
        r.commit("a.rs", "old\n", "a")?;
        r.commit("b.rs", "neighbor\n", "b")?;
        std::fs::write(r.path().join("a.rs"), "new\n").map_err(|e| e.to_string())?;
        std::fs::write(r.path().join("b.rs"), "changed neighbor\n").map_err(|e| e.to_string())?;
        let status = git::status(r.path())?;
        let key = DiffKey::Work(
            status
                .unstaged
                .iter()
                .find(|f| f.path == "a.rs")
                .ok_or("a absent")?
                .clone(),
        );
        let data = diff::load(
            r.path(),
            &key,
            DiffOptions {
                no_textconv: true,
                ..Default::default()
            },
            false,
        )?;
        let hunk = data
            .lines
            .iter()
            .position(|l| l.kind == diff::Kind::Hunk)
            .ok_or("no hunk")?;
        let patch = diff::partial_patch(
            &data.header,
            &data.lines,
            &diff::hunk_changes(&data.lines, hunk),
            diff::Pick::Stage,
        )
        .ok_or("no patch")?;
        git::run_input(Some(r.path()), &["apply", "--cached", "-"], &patch)?;
        assert_eq!(r.git(&["show", ":a.rs"])?, "new\n");
        assert_eq!(r.git(&["show", ":b.rs"])?, "neighbor\n");
        Ok(())
    }
    #[test]
    fn hunk_actions_respect_source_and_patch_capabilities() {
        let staged = DiffKey::Work(git::StatusFile {
            path: "a".into(),
            old_path: None,
            code: 'M',
            area: Area::Staged,
        });
        assert_eq!(picks(&staged, true, false), [diff::Pick::Unstage]);
        assert!(picks(&staged, false, false).is_empty());
        assert_eq!(
            picks(
                &DiffKey::Commit {
                    hash: "h".into(),
                    path: "a".into()
                },
                false,
                true
            ),
            [diff::Pick::Revert]
        );
        assert!(
            picks(
                &DiffKey::Range {
                    from: "b".into(),
                    to: Some("h".into()),
                    path: "a".into()
                },
                true,
                true
            )
            .is_empty()
        );
    }
}
