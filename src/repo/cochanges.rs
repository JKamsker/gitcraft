//! Staging suggestions retain their source commits and can always be dismissed.
use super::{Action, RepoTab, saved_views::reader::Reader};
use crate::git::investigations::cochange::{self, Hint, History};
use std::collections::BTreeSet;
#[derive(Default)]
pub(super) struct State {
    head: String,
    reader: Reader<History>,
    history: Option<History>,
    error: Option<String>,
    ranked_for: Option<(BTreeSet<String>, BTreeSet<String>)>,
    hints: Vec<Hint>,
    dismissed: BTreeSet<(String, String)>,
}
impl RepoTab {
    pub(super) fn poll_cochanges(&mut self, ctx: &egui::Context) {
        self.cochanges.reader.bind(self.tx.cancel.clone());
        if !self.shown || self.sleeping() {
            self.cochanges.reader.cancel();
            if self.cochanges.history.is_none() {
                self.cochanges.head.clear();
            }
            return;
        }
        if let Some(result) = self.cochanges.reader.poll(ctx) {
            match result {
                Ok(h) => {
                    self.cochanges.history = Some(h);
                    self.cochanges.error = None;
                    self.cochanges.ranked_for = None;
                }
                Err(e) => self.cochanges.error = Some(e),
            }
        }
        let staged = self
            .work
            .staged
            .iter()
            .map(|f| f.path.clone())
            .collect::<BTreeSet<_>>();
        if staged.is_empty() {
            self.cochanges.hints.clear();
            self.cochanges.ranked_for = None;
            return;
        }
        if self.cochanges.head != self.refs.head {
            self.cochanges.head = self.refs.head.clone();
            self.cochanges.history = None;
            self.cochanges.error = None;
            self.cochanges.hints.clear();
            self.cochanges.dismissed.clear();
            self.cochanges.ranked_for = None;
            if crate::git::investigations::oid(&self.refs.head) {
                let path = self.path.clone();
                let head = self.refs.head.clone();
                self.cochanges
                    .reader
                    .queue(move |active| cochange::load(&path, &head, active));
                self.cochanges.reader.poll(ctx);
            }
        }
        let changed = self
            .work
            .staged
            .iter()
            .chain(&self.work.unstaged)
            .chain(&self.work.conflicted)
            .map(|f| f.path.clone())
            .collect::<BTreeSet<_>>();
        let key = (staged, changed);
        if self.cochanges.ranked_for.as_ref() != Some(&key)
            && let Some(history) = &self.cochanges.history
        {
            self.cochanges.hints = cochange::rank(history, &key.0, &key.1);
            self.cochanges.ranked_for = Some(key);
        }
    }
    pub(super) fn cochange_hints(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        if self.work.staged.is_empty() {
            return;
        }
        egui::CollapsingHeader::new("What usually comes with this?").id_salt("cochange-hints").default_open(true).show(ui,|ui|{
        if self.cochanges.reader.busy(){ui.horizontal(|ui|{ui.add(crate::spinner::Spinner::new());ui.label("Reading local change coupling…");});}
        if let Some(error)=&self.cochanges.error{ui.label(format!("History hints unavailable: {error}"));}
        if let Some(history)=&self.cochanges.history {for note in &history.notes{ui.small(note);}ui.small(format!("{} qualifying commits; {} bulk/generated/unsupported commits excluded{}",history.records.len(),history.skipped,if history.limited{"; sample is incomplete"}else{""}));}
        let mut dismissed=None;
        let mut shown=0;
        for hint in &self.cochanges.hints{
            if self.cochanges.dismissed.contains(&(hint.source.clone(),hint.companion.clone())){continue;}
            shown+=1;
            ui.group(|ui|{
                ui.horizontal_wrapped(|ui|{ui.label(format!("{} usually changes with {} ({}/{} source changes, {}%) — it has no working or staged change",hint.companion,hint.source,hint.evidence.len(),hint.source_changes,hint.evidence.len()*100/hint.source_changes));if ui.small_button("Dismiss").clicked(){dismissed=Some((hint.source.clone(),hint.companion.clone()));}});
                egui::CollapsingHeader::new(format!("Evidence: {} shared commits",hint.evidence.len())).id_salt((&hint.source,&hint.companion)).show(ui,|ui|{for (hash,subject) in &hint.evidence {if ui.button(format!("{} · {}",super::short(hash),subject)).on_hover_text("Open this actual past commit to inspect both file changes").clicked(){acts.push(Action::SelectHash(hash.clone()));}}});
            });
        }
        if let Some(key)=dismissed{self.cochanges.dismissed.insert(key);}
        if shown==0&&!self.cochanges.reader.busy(){ui.small("No qualifying undismissed companion-file hints for the staged paths.");}
        ui.horizontal(|ui|{if !self.cochanges.dismissed.is_empty()&&ui.small_button("Restore dismissed hints").clicked(){self.cochanges.dismissed.clear();}
if ui.small_button("Refresh history hints").clicked(){self.cochanges.head.clear();self.cochanges.reader.cancel();}});
    });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn locate(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<egui::Pos2> {
        fn visit(shape: &egui::Shape, needle: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == needle => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| visit(s, needle)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| visit(&s.shape, needle))
    }
    #[test]
    fn painted_hint_can_be_dismissed_restored_and_open_its_commit_evidence() -> Result<(), String> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred("/recording-only/cochange-ui".into(), &ctx);
        tab.work.staged.push(crate::git::StatusFile {
            path: "serializer.rs".into(),
            old_path: None,
            code: 'M',
            area: crate::git::Area::Staged,
        });
        let hash = "a".repeat(40);
        tab.cochanges.hints.push(Hint {
            source: "serializer.rs".into(),
            companion: "compat.fixture".into(),
            source_changes: 3,
            evidence: vec![(hash.clone(), "Change serializer and fixture".into()); 3],
        });
        let mut time = 0.;
        let mut acts = Vec::new();
        let mut frame = |tab: &mut RepoTab, events: Vec<egui::Event>, acts: &mut Vec<Action>| {
            time += 0.05;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 900.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| tab.cochange_hints(ui, acts));
                },
            );
            output.textures_delta.clear();
            output.shapes
        };
        let mut click =
            |tab: &mut RepoTab, label: &str, acts: &mut Vec<Action>| -> Result<(), String> {
                let mut pos = None;
                for _ in 0..8 {
                    pos = locate(&frame(tab, vec![], acts), label);
                    if pos.is_some() {
                        break;
                    }
                }
                let pos = pos.ok_or_else(|| format!("Missing visible label {label}"))?;
                for pressed in [true, false] {
                    frame(
                        tab,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                        acts,
                    );
                }
                Ok(())
            };
        click(&mut tab, "Dismiss", &mut acts)?;
        assert_eq!(tab.cochanges.dismissed.len(), 1);
        click(&mut tab, "Restore dismissed hints", &mut acts)?;
        assert!(tab.cochanges.dismissed.is_empty());
        click(&mut tab, "Evidence: 3 shared commits", &mut acts)?;
        click(
            &mut tab,
            &format!(
                "{} · Change serializer and fixture",
                super::super::short(&hash)
            ),
            &mut acts,
        )?;
        assert!(
            acts.iter()
                .any(|a| matches!(a,Action::SelectHash(id) if id==&hash))
        );
        Ok(())
    }
}
