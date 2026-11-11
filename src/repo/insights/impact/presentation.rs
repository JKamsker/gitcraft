//! Impact controls, evidence legend, and selected graph-node details.

use super::*;

/// The change picked, the filters and the layout.
pub(super) fn controls(
    ui: &mut Ui,
    state: &State,
    subjects: &[(String, Ask)],
    asked: &Ask,
    acts: &mut Vec<Action>,
) {
    ui.horizontal_wrapped(|ui| {
        let shown = state
            .data
            .ready()
            .map(|d| d.range.label.clone())
            .or_else(|| {
                subjects
                    .iter()
                    .find(|(_, a)| a == asked)
                    .map(|(l, _)| l.clone())
            })
            .unwrap_or_else(|| "The change".to_owned());
        egui::ComboBox::from_id_salt("impact-subject")
            .selected_text(graph::elide(&shown, 56))
            .width(300.0)
            .show_ui(ui, |ui| {
                for (label, ask) in subjects {
                    if ui.selectable_label(ask == asked, label).clicked() && ask != asked {
                        acts.push(act(Act::Ask(ask.clone())));
                    }
                }
            });
        let mut filter = state.filter.clone();
        if ui
            .checkbox(&mut filter.public_only, "Public API only")
            .on_hover_text("Only items declared `pub` (not `pub(crate)` or narrower)")
            .changed()
        {
            acts.push(act(Act::Filter(filter.clone())));
        }
        egui::ComboBox::from_id_salt("impact-scope")
            .selected_text(state.filter.scope.label())
            .show_ui(ui, |ui| {
                for scope in Scope::ALL {
                    if ui
                        .selectable_label(scope == state.filter.scope, scope.label())
                        .clicked()
                    {
                        acts.push(act(Act::Filter(Filter {
                            scope,
                            ..state.filter.clone()
                        })));
                    }
                }
            });
        if let Some(data) = state.data.ready()
            && !data.symbols.is_empty()
        {
            let symbol_label = |i: Option<usize>| match i.and_then(|i| data.symbols.get(i)) {
                Some(s) => graph::elide(&s.path, 40),
                None => "Every changed symbol".to_owned(),
            };
            egui::ComboBox::from_id_salt("impact-symbol")
                .selected_text(symbol_label(state.filter.symbol))
                .show_ui(ui, |ui| {
                    let choices = std::iter::once(None).chain((0..data.symbols.len()).map(Some));
                    for choice in choices {
                        if ui
                            .selectable_label(state.filter.symbol == choice, symbol_label(choice))
                            .clicked()
                        {
                            acts.push(act(Act::Filter(Filter {
                                symbol: choice,
                                ..state.filter.clone()
                            })));
                        }
                    }
                });
        }
        if ui
            .selectable_label(!state.force, "Columns")
            .on_hover_text("Changed symbols, the code referring to them, the tests")
            .clicked()
            && state.force
        {
            acts.push(act(Act::Force(false)));
        }
        if ui
            .selectable_label(state.force, "Force")
            .on_hover_text("Referring code pulled towards what it refers to")
            .clicked()
            && !state.force
        {
            acts.push(act(Act::Force(true)));
        }
    });
}

/// What the marks, lines and shapes mean.
pub(super) fn legend(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    common::legend(
        ui,
        &[
            (
                Mark::Line(t.text_dim, false),
                "solid: in the diff (observed)",
            ),
            (
                Mark::Line(t.text_dim, true),
                "dashed: a name match (inferred; may be another item of that name)",
            ),
            (Mark::Dotted(t.text_dim), "dotted: unknown (no analyzer)"),
            (Mark::Glyph("+", t.success), "added"),
            (Mark::Glyph("-", t.danger), "removed"),
            (Mark::Glyph("~", t.accent), "modified"),
            (Mark::Glyph("T", t.text_dim), "test"),
        ],
    );
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (diamond, color, label) in [
            (false, t.text_dim, "code in a changed file"),
            (
                true,
                t.warning,
                "code in a file the change left alone (affected, unchanged)",
            ),
        ] {
            let (rect, _) = ui.allocate_exact_size(vec2(16.0, 12.0), Sense::hover());
            let c = rect.center();
            let stroke = Stroke::new(1.5, color);
            if diamond {
                let r = 5.5;
                let points = vec![
                    c + vec2(0.0, -r),
                    c + vec2(r, 0.0),
                    c + vec2(0.0, r),
                    c + vec2(-r, 0.0),
                ];
                ui.painter().add(egui::Shape::closed_line(points, stroke));
            } else {
                ui.painter().circle_stroke(c, 5.0, stroke);
            }
            ui.label(RichText::new(label).size(11.5).color(t.text_dim));
            ui.add_space(8.0);
        }
    });
}

/// What was left out or can't be known, in a few lines.
pub(super) fn notes(ui: &mut Ui, data: &Impact) {
    let t = Tokens::get(ui.ctx());
    let mut lines = Vec::new();
    if !data.other.is_empty() {
        let mut why: Vec<&str> = data.other.iter().map(|o| o.why.as_str()).collect();
        why.sort();
        why.dedup();
        lines.push(format!(
            "{} changed file{} in other languages: what they affect is unknown ({})",
            data.other.len(),
            if data.other.len() == 1 { "" } else { "s" },
            why.join(", ")
        ));
    }
    let same: u32 = data.symbols.iter().map(|s| s.same_named).sum();
    if same > 0 {
        lines.push(format!(
            "{same} other definition{} share a changed symbol's name: some references may be \
             to them",
            if same == 1 { "" } else { "s" }
        ));
    }
    if data.in_comments > 0 {
        lines.push(format!(
            "{} name match{} in comments or strings left out",
            data.in_comments,
            if data.in_comments == 1 { "" } else { "es" }
        ));
    }
    let outside: u32 = data.rust_files.iter().map(|(_, n)| n).sum();
    if outside > 0 {
        lines.push(format!(
            "{outside} changed line{} of Rust files are in no item (imports, blank lines)",
            if outside == 1 { "" } else { "s" }
        ));
    }
    for line in lines {
        ui.add(egui::Label::new(RichText::new(line).size(11.5).color(t.text_faint)).wrap());
    }
}

/// What is selected in the picture and the lines behind it, each opening at its revision.
pub(super) fn details(
    ui: &mut Ui,
    data: &Impact,
    laid: &Laid,
    pick: Pick,
    filter: &Filter,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let shown = filter.refs(data);
    let (title, refs, buttons): (String, Vec<usize>, Vec<(String, Action)>) = match pick {
        Pick::Edge(i) => {
            let Some(edge) = laid.edges.get(i) else {
                return;
            };
            (
                format!(
                    "{} refers to {} by name (inferred): the lines",
                    data.sites[edge.site].label, data.symbols[edge.symbol].name
                ),
                edge.refs.clone(),
                Vec::new(),
            )
        }
        Pick::Node(i) => match laid.nodes.get(i) {
            Some(&NodeRef::Symbol(s)) => {
                let symbol = &data.symbols[s];
                let mut buttons = Vec::new();
                for (side, span) in [("head", &symbol.head), ("base", &symbol.base)] {
                    if let Some(span) = span {
                        buttons.push((
                            format!("Definition at {side} {}", short(&span.rev)),
                            Action::BlameAt {
                                path: span.path.clone(),
                                rev: Some(span.rev.clone()),
                                line: span.line as usize,
                            },
                        ));
                    }
                }
                if let Some(span) = symbol.at() {
                    buttons.push(("Diff of the file".to_owned(), file_diff(data, &span.path)));
                }
                let mut title = format!(
                    "{} {} ({}): what refers to it by name",
                    symbol.kind.word(),
                    symbol.path,
                    symbol.change.word()
                );
                if symbol.refs_cut {
                    title.push_str(&format!(" (only the first {} kept)", core::MAX_REFS));
                }
                let refs = shown
                    .iter()
                    .copied()
                    .filter(|&r| data.refs[r].symbol == s)
                    .collect();
                (title, refs, buttons)
            }
            Some(&NodeRef::Site(s)) => (
                format!("{}: what it refers to", data.sites[s].label),
                shown
                    .iter()
                    .copied()
                    .filter(|&r| data.refs[r].site == s)
                    .collect(),
                Vec::new(),
            ),
            Some(&NodeRef::Other(o)) => {
                let other = &data.other[o];
                (
                    format!("{}: {}", other.path, other.why),
                    Vec::new(),
                    vec![("Diff of the file".to_owned(), file_diff(data, &other.path))],
                )
            }
            None => return,
        },
    };
    egui::Frame::new()
        .fill(t.card)
        .corner_radius(t.radius)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(title).strong());
                for (label, action) in buttons {
                    if ui.button(label).clicked() {
                        acts.push(action);
                    }
                }
                if ui.small_button("Clear").on_hover_text("Esc").clicked() {
                    acts.push(act(Act::Select(None)));
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("impact-details")
                .max_height(140.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for &r in &refs {
                        let reference = &data.refs[r];
                        let cells = [
                            format!(
                                "{}:{} @ {}",
                                reference.path,
                                reference.line,
                                short(&reference.rev)
                            ),
                            reference.text.clone(),
                        ];
                        if common::table_row(ui, &[300.0], &cells, false, false)
                            .on_hover_text("Open this line at that revision")
                            .clicked()
                        {
                            acts.push(open_ref(reference));
                        }
                    }
                });
        });
    ui.add_space(4.0);
}
