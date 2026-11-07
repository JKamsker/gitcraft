//! A single scroll document with the same patch builder and operation runner as file diffs.
use crate::{
    git::{self, Area, DiffOptions},
    repo::{Action, DiffKey, RepoTab, diff},
    theme::Tokens,
};
use egui::{RichText, Ui};

#[derive(Clone)]
pub(crate) enum ReviewAct {
    Open(Vec<DiffKey>),
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
    keys: Vec<DiffKey>,
    data: Option<Result<Vec<diff::DiffData>, String>>,
}

impl RepoTab {
    pub(in crate::repo) fn review_all_button(
        &self,
        ui: &mut Ui,
        keys: Vec<DiffKey>,
        acts: &mut Vec<Action>,
    ) {
        if ui
            .add_enabled(!keys.is_empty(), egui::Button::new("Review all changes"))
            .clicked()
        {
            acts.push(Action::Review(ReviewAct::Open(keys)));
        }
    }
    pub(in crate::repo) fn review_act(&mut self, act: ReviewAct, ctx: &egui::Context) {
        match act {
            ReviewAct::Close => self.review_document.open = false,
            ReviewAct::Reload => self.load_review_document(ctx),
            ReviewAct::Open(keys) => {
                if self.conflict.is_some() || self.rebase.is_some() {
                    return;
                }
                self.review_document.keys = keys;
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
                    tab.apply_picked(pick, "Review hunk", saved,ctx);
                    // Old hunks cannot be clicked again; reload after the operation finishes.
                    tab.review_document.data = Some(Err("Hunk submitted. Reload this page after the operation finishes.".into()));
                });
            }
        }
    }
    fn load_review_document(&mut self, ctx: &egui::Context) {
        self.review_document.request += 1;
        let request = self.review_document.request;
        let keys = self.review_document.keys.clone();
        let path = self.path.clone();
        let dark = Tokens::get(ctx).dark();
        self.review_document.data = None;
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                if keys.len() > 500 {
                    return Err(
                        "This document has more than 500 files; select a narrower comparison"
                            .into(),
                    );
                }
                keys.iter()
                    .map(|key| {
                        diff::load(
                            &path,
                            key,
                            DiffOptions {
                                no_textconv: true,
                                ..Default::default()
                            },
                            dark,
                        )
                    })
                    .collect()
            },
            move |tab, result, _| {
                if tab.review_document.request == request {
                    tab.review_document.data = Some(result);
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
        let Some(data) = &state.data else {
            ui.spinner();
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
                        if data.binary.is_some() || data.image.is_some() || data.lfs.is_some() {
                            if ui.button("Open binary / image comparison").clicked() {
                                acts.push(Action::OpenDiff(key.clone()));
                            }
                        }
                        let review = self.diff_review(key);
                        for (hunk, line) in data.lines.iter().enumerate() {
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
                                    if ui.small_button("Open hunk").clicked() {
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
                                    ui.label(
                                        RichText::new(format!(
                                            "{:>5} {:>5} {}",
                                            line.old.map(|n| n.to_string()).unwrap_or_default(),
                                            line.new.map(|n| n.to_string()).unwrap_or_default(),
                                            line.text
                                        ))
                                        .monospace()
                                        .color(color),
                                    );
                                });
                            }
                        }
                    });
                }
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
