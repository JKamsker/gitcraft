use super::{Action, RepoTab, saved_views::reader::Reader};
use crate::git::investigations::{
    dossier::Seed,
    symbols::{self, Answer},
};
pub(super) struct State {
    pub open: bool,
    revision: String,
    path: String,
    name: String,
    line: Option<usize>,
    depth: usize,
    reader: Reader<Answer>,
    result: Option<Answer>,
    error: Option<String>,
    selected: usize,
    forge_reader: Reader<crate::forge::history::Evidence>,
    evidence_for: Option<Seed>,
    evidence: Option<Result<crate::forge::history::Evidence, String>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            revision: "HEAD".into(),
            path: String::new(),
            name: String::new(),
            line: None,
            depth: 256,
            reader: Default::default(),
            result: None,
            error: None,
            selected: 0,
            forge_reader: Default::default(),
            evidence_for: None,
            evidence: None,
        }
    }
}
impl State {
    pub fn close(&mut self) {
        self.open = false;
        self.reader.cancel();
        self.forge_reader.cancel();
        self.result = None;
        self.evidence = None;
        self.evidence_for = None;
        self.error = None;
    }
}
impl RepoTab {
    pub(super) fn open_symbol_biography(&mut self, seed: Option<Seed>, ctx: &egui::Context) {
        self.dossier.close();
        self.symbols.close();
        self.symbols.open = true;
        self.symbols.revision = self.refs.head.clone();
        self.symbols.path = self
            .file_view
            .as_ref()
            .map_or_else(String::new, |v| v.path.clone());
        self.symbols.name.clear();
        self.symbols.line = None;
        if let Some(seed) = seed {
            self.symbols.revision = seed.commit;
            self.symbols.path = seed.path;
            self.symbols.line = Some(seed.line);
            self.trace_symbol(ctx);
        }
    }
    fn trace_symbol(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        let revision = self.symbols.revision.clone();
        let path = self.symbols.path.clone();
        let name = self.symbols.name.clone();
        let line = self.symbols.line;
        let depth = self.symbols.depth;
        self.symbols.result = None;
        self.symbols.error = None;
        self.symbols.selected = 0;
        self.symbols.evidence = None;
        self.symbols.evidence_for = None;
        self.symbols.forge_reader.cancel();
        self.symbols.reader.queue(move |active| {
            symbols::investigate(&repo, &revision, &path, &name, line, depth, active)
        });
        ctx.request_repaint();
    }
    pub(super) fn poll_symbol_biography(&mut self, ctx: &egui::Context) {
        self.symbols.reader.bind(self.tx.cancel.clone());
        self.symbols.forge_reader.bind(self.tx.cancel.clone());
        if !self.shown {
            self.symbols.close();
        }
        if let Some(result) = self.symbols.reader.poll(ctx)
            && self.symbols.open
        {
            match result {
                Ok(r) => self.symbols.result = Some(r),
                Err(e) => self.symbols.error = Some(e),
            }
        }
        if let Some(result) = self.symbols.forge_reader.poll(ctx) {
            self.symbols.evidence = Some(result);
        }
        if self.symbols.open
            && let Some(Answer::History(history)) = &self.symbols.result
            && let Some(step) = history.steps.get(self.symbols.selected)
        {
            let symbol = step.symbol.as_ref().or_else(|| {
                history.steps[..self.symbols.selected]
                    .iter()
                    .rev()
                    .find_map(|s| s.symbol.as_ref())
            });
            if let Some(symbol) = symbol {
                let at = Seed {
                    commit: step.commit.clone(),
                    path: symbol.path.clone(),
                    line: symbol.first,
                };
                if self.symbols.evidence_for.as_ref() != Some(&at) {
                    self.symbols.evidence_for = Some(at.clone());
                    self.symbols.evidence = None;
                    let query = crate::forge::history::Query {
                        at,
                        file_oid: symbol.blob.clone(),
                        comments: false,
                    };
                    let repo = self.path.clone();
                    let hosting = self.hosting.clone();
                    self.symbols.forge_reader.queue(move |active| {
                        if let Some(why) = crate::network::refused() {
                            return Err(why.why().into());
                        }
                        crate::forge::history::lookup(
                            &repo,
                            &hosting.ok_or(
                                "No supported forge remote; local history remains available",
                            )?,
                            &query,
                            active,
                        )
                    });
                }
            }
        }
    }
    pub(super) fn symbol_biography_view(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let mut trace = false;
        let mut choose = None;
        let t = crate::theme::Tokens::get(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            ui.heading("Symbol biography");
            if ui.button("Close biography").clicked() {
                acts.push(Action::CloseSymbolBiography);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Commit");
            ui.add(
                egui::TextEdit::singleline(&mut self.symbols.revision)
                    .desired_width(150.)
                    .char_limit(256),
            );
            ui.label("Path");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.symbols.path)
                        .desired_width(220.)
                        .char_limit(4096),
                )
                .changed()
            {
                self.symbols.line = None;
            }
            ui.label("Function");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.symbols.name)
                        .desired_width(160.)
                        .char_limit(1000),
                )
                .changed()
            {
                self.symbols.line = None;
            }
            ui.label("History depth");
            ui.add(egui::DragValue::new(&mut self.symbols.depth).range(16..=2048));
            if ui.button("Trace function").clicked() {
                trace = true;
            }
            if self.symbols.reader.busy() {
                ui.add(crate::spinner::Spinner::new());
                if ui.button("Cancel trace").clicked() {
                    self.symbols.reader.cancel();
                    self.symbols.error = Some("Trace cancelled".into());
                }
            }
        });
        ui.small("Rust · Python · JavaScript/JSX · TypeScript/TSX · Go · C/C++ · committed UTF-8 source. Enter a function name, or leave it empty to choose. Rename detection and structural body similarity follow its lifetime.");
        if let Some(error) = &self.symbols.error {
            ui.colored_label(t.danger, error);
        }
        match &self.symbols.result {
            Some(Answer::Choices(symbols)) => {
                ui.label("Choose the function to trace:");
                egui::ScrollArea::vertical()
                    .id_salt("symbol_choices")
                    .show_rows(ui, 25., symbols.len(), |ui, range| {
                        for i in range {
                            let s = &symbols[i];
                            if ui
                                .button(format!("{} · {}:{}", s.qualified, s.path, s.first))
                                .clicked()
                            {
                                choose = Some((self.symbols.revision.clone(), s.clone()));
                            }
                        }
                    });
            }
            Some(Answer::History(history)) => {
                ui.small(format!(
                    "Pinned tip {} · {} events{}",
                    super::short(&history.tip),
                    history.steps.len(),
                    if history.limited {
                        " · incomplete lineage (see notes)"
                    } else {
                        ""
                    }
                ));
                ui.collapsing("Scope, uncertainty and limits", |ui| {
                    for note in &history.notes {
                        ui.label(note);
                    }
                });
                if !history.steps.is_empty() {
                    scrub(ui, &mut self.symbols.selected, history.steps.len());
                    egui::ScrollArea::horizontal()
                        .id_salt("symbol_events")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (i, step) in history.steps.iter().enumerate() {
                                    if ui
                                        .selectable_label(
                                            self.symbols.selected == i,
                                            format!(
                                                "{} {}",
                                                super::short(&step.commit),
                                                step.event
                                            ),
                                        )
                                        .on_hover_text(&step.subject)
                                        .clicked()
                                    {
                                        self.symbols.selected = i;
                                    }
                                }
                            })
                        });
                    let step = &history.steps[self.symbols.selected];
                    ui.heading(&step.event);
                    ui.label(format!(
                        "{} · {} · {}",
                        step.author, step.time, step.subject
                    ));
                    if let Some(confidence) = step.confidence {
                        ui.small(format!(
                            "Structural body similarity {confidence}% (heuristic)"
                        ));
                    }
                    if ui
                        .button(format!("Open commit {}", super::short(&step.commit)))
                        .clicked()
                    {
                        acts.push(Action::SelectHash(step.commit.clone()));
                    }
                    if self.symbols.forge_reader.busy() {
                        ui.horizontal(|ui| {
                            ui.add(crate::spinner::Spinner::new());
                            ui.label("Loading associated pull requests…");
                        });
                    }
                    if self
                        .symbols
                        .evidence_for
                        .as_ref()
                        .is_some_and(|s| s.commit == step.commit)
                        && let Some(evidence) = &self.symbols.evidence
                    {
                        match evidence {
                            Ok(e) => {
                                for p in &e.pulls {
                                    ui.hyperlink_to(
                                        format!("#{} {} · {}", p.number, p.title, p.association),
                                        &p.url,
                                    );
                                }
                                for n in &e.notes {
                                    ui.small(n);
                                }
                                if e.pulls.is_empty() {
                                    ui.small("No associated pull request found in checked forge evidence.");
                                }
                            }
                            Err(e) => {
                                ui.small(e);
                            }
                        }
                    }
                    for related in &step.related {
                        if ui
                            .button(format!(
                                "Follow possible split: {} · {}:{}",
                                related.qualified, related.path, related.first
                            ))
                            .clicked()
                        {
                            choose = Some((step.commit.clone(), related.clone()));
                        }
                    }
                    if let Some(symbol) = &step.symbol {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "{} · {}:{}–{}",
                                symbol.qualified, symbol.path, symbol.first, symbol.last
                            ));
                            if ui.button("Open pinned source").clicked() {
                                acts.push(Action::OpenPinnedFileLine(Seed {
                                    commit: step.commit.clone(),
                                    path: symbol.path.clone(),
                                    line: symbol.first,
                                }));
                            }
                        });
                        let lines = symbol.body.lines().collect::<Vec<_>>();
                        egui::ScrollArea::both()
                            .id_salt(("symbol_body", &step.commit))
                            .show_rows(
                                ui,
                                ui.text_style_height(&egui::TextStyle::Monospace),
                                lines.len(),
                                |ui, range| {
                                    for i in range {
                                        ui.monospace(format!(
                                            "{:>5} {}",
                                            symbol.first + i,
                                            if lines[i].chars().count() > 2000 {
                                                format!(
                                                    "{} … [long line clipped; open pinned source]",
                                                    lines[i].chars().take(2000).collect::<String>()
                                                )
                                            } else {
                                                lines[i].to_owned()
                                            }
                                        ));
                                    }
                                },
                            );
                    } else {
                        ui.label("This function was deleted at this commit.");
                    }
                }
            }
            None => {}
        }
        if let Some((commit, symbol)) = choose {
            self.symbols.revision = commit;
            self.symbols.path = symbol.path;
            self.symbols.name = symbol.qualified;
            self.symbols.line = Some(symbol.first);
            trace = true;
        }
        if trace {
            self.trace_symbol(ui.ctx());
        }
    }
}
fn scrub(ui: &mut egui::Ui, selected: &mut usize, len: usize) {
    if len == 0 {
        return;
    }
    *selected = (*selected).min(len - 1);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(*selected > 0, egui::Button::new("Previous version"))
            .clicked()
        {
            *selected -= 1;
        }
        ui.add(egui::Slider::new(selected, 0..=len - 1).text("Version"));
        if ui
            .add_enabled(*selected + 1 < len, egui::Button::new("Next version"))
            .clicked()
        {
            *selected += 1;
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn locate(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<egui::Pos2> {
        fn visit(s: &egui::Shape, n: &str) -> Option<egui::Pos2> {
            match s {
                egui::Shape::Text(t) if t.galley.job.text == n => {
                    Some(t.pos + t.galley.size() / 2.)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| visit(s, n)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| visit(&s.shape, needle))
    }
    #[test]
    fn timeline_pointer_steps_forward_and_back() -> Result<(), String> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut selected = 0;
        let mut time = 0.;
        let mut frame = |selected: &mut usize, events| {
            time += 0.05;
            let mut o = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 300.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| scrub(ui, selected, 3));
                },
            );
            o.textures_delta.clear();
            o.shapes
        };
        for (label, expected) in [
            ("Next version", 1),
            ("Next version", 2),
            ("Previous version", 1),
        ] {
            let mut pos = None;
            for _ in 0..5 {
                pos = locate(&frame(&mut selected, vec![]), label);
                if pos.is_some() {
                    break;
                }
            }
            let pos = pos.ok_or("scrub button painted")?;
            for pressed in [true, false] {
                frame(
                    &mut selected,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            assert_eq!(selected, expected);
        }
        Ok(())
    }
}
