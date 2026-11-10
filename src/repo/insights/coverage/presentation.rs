//! Coverage controls, trust disclosure, and selected hunk details.

use super::*;

/// The lines looked at, the report read, and a report's path to type.
pub(super) fn controls(
    ui: &mut Ui,
    state: &mut State,
    subjects: &[(String, Ask)],
    acts: &mut Vec<Action>,
) {
    let asked = state.ask.clone().unwrap_or(Ask::WorkTree);
    ui.horizontal_wrapped(|ui| {
        let shown = subjects
            .iter()
            .find(|(_, a)| *a == asked)
            .map_or_else(|| "The change".to_owned(), |(l, _)| l.clone());
        egui::ComboBox::from_id_salt("coverage-subject")
            .selected_text(shown)
            .width(260.0)
            .show_ui(ui, |ui| {
                for (label, ask) in subjects {
                    if ui.selectable_label(*ask == asked, label).clicked() && *ask != asked {
                        acts.push(act(Act::Ask(ask.clone())));
                    }
                }
            });
        if let Some(data) = state.data.ready() {
            let current = data.report.as_ref().map(|r| r.path.clone());
            if current.is_some() || !data.others.is_empty() {
                egui::ComboBox::from_id_salt("coverage-report")
                    .selected_text(current.clone().unwrap_or_else(|| "No report".to_owned()))
                    .show_ui(ui, |ui| {
                        for other in current.iter().chain(&data.others) {
                            if ui
                                .selectable_label(Some(other) == current.as_ref(), other)
                                .clicked()
                                && Some(other) != current.as_ref()
                            {
                                acts.push(act(Act::Report(Some(other.clone()))));
                            }
                        }
                    });
            }
        }
        let field = ui.add(
            egui::TextEdit::singleline(&mut state.typed)
                .hint_text("Another report: path/to/lcov.info")
                .desired_width(220.0),
        );
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Read").clicked() || entered) && !state.typed.trim().is_empty() {
            acts.push(act(Act::Report(Some(state.typed.trim().to_owned()))));
        }
        if state.report.is_some() && ui.small_button("Usual places").clicked() {
            acts.push(act(Act::Report(None)));
        }
    });
}

/// How far the report is trusted for these lines, and what follows from it.
pub(super) fn trust(ui: &mut Ui, data: &Coverage) {
    let t = Tokens::get(ui.ctx());
    let Some(report) = &data.report else {
        common::unavailable(
            ui,
            "No coverage report found",
            &format!(
                "Looked for {}. Type a report's path above, or make one (cargo llvm-cov \
                 --lcov --output-path lcov.info) and write the commit it tested to \
                 lcov.info.sha beside it. Every changed line is unknown until then.",
                data.searched.join(", ")
            ),
        );
        return;
    };
    let (word, color) = match report.freshness.evidence {
        Evidence::Observed => ("Matches the revision", t.success),
        Evidence::Inferred => ("Likely matches", t.warning),
        Evidence::Unknown => ("Revision not matched", t.text_faint),
    };
    let mut lines = vec![format!("{word}: {}.", report.freshness.said)];
    if report.freshness.evidence == Evidence::Unknown {
        lines.push(format!(
            "What it says of the changed lines isn't taken: they show as unknown. Write the \
             tested commit's id to {}{} to match it.",
            report.path,
            core::SIDECAR
        ));
    }
    if report.per_test && report.freshness.evidence == Evidence::Unknown {
        lines.push(
            "It names tests, but which ran what isn't shown for an unmatched report.".to_owned(),
        );
    } else if !report.per_test {
        lines.push(format!(
            "Aggregate coverage only: {} records no tests per line{}.",
            report.format.label(),
            if report.tests.len() == 1 {
                " (it names one, for everything)"
            } else {
                ""
            }
        ));
    }
    for (i, line) in lines.into_iter().enumerate() {
        let text = RichText::new(line).size(12.0);
        ui.add(
            egui::Label::new(if i == 0 {
                text.color(color)
            } else {
                text.color(t.text_faint)
            })
            .wrap(),
        );
    }
}

pub(super) fn legend(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    common::legend(
        ui,
        &[
            (Mark::Glyph("C", t.success), "covered"),
            (Mark::Glyph("!", t.danger), "uncovered lines"),
            (Mark::Glyph("-", t.text_dim), "not instrumented"),
            (
                Mark::Glyph("?", t.text_faint),
                "unknown (no matching report)",
            ),
            (Mark::Glyph("T", t.accent), "a test the report names"),
            (
                Mark::Line(t.text_dim, false),
                "solid: matched by .sha (observed)",
            ),
            (Mark::Line(t.text_dim, true), "dashed: likely (inferred)"),
            (Mark::Dotted(t.text_dim), "dotted: unknown"),
        ],
    );
}

/// The changed lines a trusted report says no test ran, each range opening its hunk.
pub(super) fn uncovered(ui: &mut Ui, data: &Coverage, acts: &mut Vec<Action>) {
    let mut rows: Vec<(usize, usize, String)> = Vec::new();
    for (fi, f) in data.files.iter().enumerate() {
        for (hi, h) in f.hunks.iter().enumerate() {
            for (a, b) in uncovered_ranges(h) {
                let lines = if a == b {
                    format!("line {a}")
                } else {
                    format!("lines {a}-{b}")
                };
                rows.push((fi, hi, format!("{}: {lines}", f.path)));
            }
        }
    }
    if rows.is_empty() {
        return;
    }
    common::heading(
        ui,
        &format!("Uncovered changed lines ({} ranges)", rows.len()),
    );
    egui::ScrollArea::vertical()
        .id_salt("coverage-uncovered")
        .max_height(110.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (fi, hi, text) in rows {
                if common::table_row(ui, &[], &[text], false, false)
                    .on_hover_text("Open its diff")
                    .clicked()
                {
                    acts.extend(open_hunk(data, fi, hi));
                }
            }
        });
}

/// The runs of uncovered lines in a hunk.
fn uncovered_ranges(h: &HunkCov) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for l in h.lines.iter().filter(|l| l.cov == LineCov::Uncovered) {
        match out.last_mut() {
            Some((_, end)) if *end + 1 == l.line => *end = l.line,
            _ => out.push((l.line, l.line)),
        }
    }
    out
}

/// The test lane's results for the commit tested.
pub(super) fn lane(ui: &mut Ui, data: &Coverage, acts: &mut Vec<Action>) {
    if data.lane.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Test lane at {}:", short(&data.sha))).color(t.text_dim));
        for (i, v) in data.lane.iter().enumerate() {
            if ui
                .button(format!("{} {}", v.name, v.verdict.word()))
                .on_hover_text("Show the commit, and what its test printed if it ran here")
                .clicked()
            {
                acts.extend(open_node(data, NodeRef::Lane(i), None));
            }
        }
    });
}

/// What is selected in the picture, line by line.
pub(super) fn details(
    ui: &mut Ui,
    data: &Coverage,
    laid: &Laid,
    pick: Pick,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let Pick::Node(i) = pick else {
        return;
    };
    let Some(&node) = laid.nodes.get(i) else {
        return;
    };
    let tests: &[String] = data.report.as_ref().map_or(&[], |r| &r.tests);
    egui::Frame::new()
        .fill(t.card)
        .corner_radius(t.radius)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let title = match node {
                    NodeRef::File(f) => data.files[f].path.clone(),
                    NodeRef::Hunk(f, h) => {
                        format!(
                            "{} {}",
                            data.files[f].path,
                            picture::hunk_label(&data.files[f].hunks[h])
                        )
                    }
                    NodeRef::Test(x) => format!(
                        "{}: the hunks it ran",
                        tests.get(x).map_or("", String::as_str)
                    ),
                    NodeRef::Lane(x) => format!("Test lane: {}", data.lane[x].name),
                };
                ui.label(RichText::new(title).strong());
                if let NodeRef::Hunk(f, h) = node
                    && ui.button("Open the diff").clicked()
                {
                    acts.extend(open_hunk(data, f, h));
                }
                if ui.small_button("Clear").on_hover_text("Esc").clicked() {
                    acts.push(act(Act::Select(None)));
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("coverage-details")
                .max_height(130.0)
                .auto_shrink([false, true])
                .show(ui, |ui| match node {
                    NodeRef::Hunk(f, h) => {
                        for l in &data.files[f].hunks[h].lines {
                            let said = match (l.cov, l.hits) {
                                (LineCov::Covered(n), _) => format!("covered, ran {n} times"),
                                (LineCov::Unknown, Some(n)) => {
                                    format!("unknown (the unmatched report says {n} hits)")
                                }
                                (cov, _) => cov.label().to_owned(),
                            };
                            ui.label(format!("line {}: {said}", l.line));
                        }
                    }
                    NodeRef::Test(x) => {
                        for (fi, f) in data.files.iter().enumerate() {
                            for (hi, h) in f.hunks.iter().enumerate() {
                                if h.tests.contains(&x)
                                    && common::table_row(
                                        ui,
                                        &[],
                                        &[format!("{} {}", f.path, picture::hunk_label(h))],
                                        false,
                                        false,
                                    )
                                    .clicked()
                                {
                                    acts.extend(open_hunk(data, fi, hi));
                                }
                            }
                        }
                    }
                    NodeRef::File(f) => {
                        for h in &data.files[f].hunks {
                            ui.label(format!(
                                "{}: {}",
                                picture::hunk_label(h),
                                picture::counts_text(h.counts())
                            ));
                        }
                    }
                    NodeRef::Lane(x) => {
                        ui.label(format!(
                            "{} at {}",
                            data.lane[x].verdict.word(),
                            short(&data.sha)
                        ));
                    }
                });
        });
    ui.add_space(4.0);
}
