//! Evolution matrix controls, legend, cell styling, and grid painting.

use super::*;

/// The branch, the history limit, the axis, the colour, the rows and the filter.
pub(super) fn controls(
    ui: &mut Ui,
    state: &mut State,
    branches: &[String],
    acts: &mut Vec<Action>,
) {
    let push =
        |acts: &mut Vec<Action>, act: Act| acts.push(Action::Insights(InsightsAct::Evolution(act)));
    ui.horizontal_wrapped(|ui| {
        let shown = state.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
        egui::ComboBox::from_id_salt("insights-evolution-rev")
            .selected_text(shown)
            .width(140.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.rev.is_none(), "HEAD (checked out)")
                    .clicked()
                    && state.rev.is_some()
                {
                    push(acts, Act::Rev(None));
                }
                for name in branches {
                    let on = state.rev.as_deref() == Some(name.as_str());
                    if ui.selectable_label(on, name).clicked() && !on {
                        push(acts, Act::Rev(Some(name.clone())));
                    }
                }
            })
            .response
            .on_hover_text("The branch whose history is read");
        let limit = state.limit();
        egui::ComboBox::from_id_salt("insights-evolution-limit")
            .selected_text(format!("last {limit} commits"))
            .show_ui(ui, |ui| {
                for n in LIMITS {
                    if ui
                        .selectable_label(n == limit, format!("last {n} commits"))
                        .clicked()
                        && n != limit
                    {
                        push(acts, Act::Limit(n));
                    }
                }
            });
        ui.separator();
        let mut changed = false;
        for axis in Axis::ALL {
            let tip = match axis {
                Axis::Commits => "One column per commit, oldest on the left",
                _ => "Commits grouped by author date (UTC), every period shown",
            };
            if ui
                .selectable_label(state.options.axis == axis, axis.label())
                .on_hover_text(tip)
                .clicked()
            {
                changed |= state.options.axis != axis;
                state.options.axis = axis;
            }
        }
        ui.separator();
        for measure in Measure::ALL {
            if ui
                .selectable_label(state.options.measure == measure, measure.label())
                .clicked()
            {
                changed |= state.options.measure != measure;
                state.options.measure = measure;
            }
        }
        ui.separator();
        let group = |g: Option<usize>| match g {
            None => "Files".to_owned(),
            Some(d) => format!("Folders, {d} deep"),
        };
        egui::ComboBox::from_id_salt("insights-evolution-group")
            .selected_text(group(state.options.group))
            .show_ui(ui, |ui| {
                for g in std::iter::once(None).chain((1..=MAX_DEPTH).map(Some)) {
                    if ui
                        .selectable_label(state.options.group == g, group(g))
                        .clicked()
                    {
                        changed |= state.options.group != g;
                        state.options.group = g;
                    }
                }
            })
            .response
            .on_hover_text("Rows: files, or the folders they are in (deeper zooms in)");
        let filter = ui.add(
            egui::TextEdit::singleline(&mut state.options.filter)
                .hint_text("Filter paths")
                .desired_width(150.0),
        );
        changed |= filter.changed();
        let rows = state.options.max_rows;
        egui::ComboBox::from_id_salt("insights-evolution-rows")
            .selected_text(format!("{rows} rows at most"))
            .show_ui(ui, |ui| {
                for n in ROW_LIMITS {
                    if ui
                        .selectable_label(n == rows, format!("{n} rows at most"))
                        .clicked()
                    {
                        changed |= n != rows;
                        state.options.max_rows = n;
                    }
                }
            });
        if state.read.loading || state.built.loading {
            ui.add(crate::spinner::Spinner::new());
        }
        if changed {
            push(acts, Act::Rebuild);
        }
    });
}

/// What the colours and marks mean.
pub(super) fn legend(ui: &mut Ui, built: &Built) {
    let t = Tokens::get(ui.ctx());
    let m = &built.matrix;
    let mut entries: Vec<(Mark, String)> = match built.options.measure {
        Measure::Lines => vec![
            (Mark::Fill(common::heat(&t, 0.25)), "few lines".to_owned()),
            (
                Mark::Fill(common::heat(&t, 1.0)),
                format!("{} lines (log scale)", m.max_lines),
            ),
        ],
        Measure::Balance => vec![
            (Mark::Glyph("+", t.success), "mostly added".to_owned()),
            (Mark::Glyph("=", t.text_dim), "about even".to_owned()),
            (Mark::Glyph("−", t.danger), "mostly deleted".to_owned()),
        ],
        Measure::Frequency => vec![
            (Mark::Fill(common::heat(&t, 0.25)), "1 commit".to_owned()),
            (
                Mark::Fill(common::heat(&t, 1.0)),
                format!("{} commits", m.max_commits),
            ),
        ],
    };
    entries.push((Mark::Hatched(t.text_dim), "binary files only".to_owned()));
    entries.push((Mark::Glyph("R", t.text), "renamed".to_owned()));
    entries.push((
        Mark::Hollow(t.text_dim),
        "changed, no lines (mode, empty)".to_owned(),
    ));
    if m.boundary {
        entries.push((
            Mark::Hatched(t.warning),
            "older history not read".to_owned(),
        ));
    }
    let refs: Vec<(Mark, &str)> = entries.iter().map(|(m, s)| (*m, s.as_str())).collect();
    common::legend(ui, &refs);
}

/// The line saying the history was cut short, with a button to read more of it.
pub(super) fn boundary_note(ui: &mut Ui, built: &Built, limit: usize, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "Only the last {} commits of {} were read: the hatched column on the left is \
                 where they end, and nothing older is counted.",
                built.history.commits.len(),
                built.history.rev
            ))
            .size(11.5)
            .color(t.warning),
        );
        if let Some(&more) = LIMITS.iter().find(|&&n| n > limit)
            && ui.small_button(format!("Read {more}")).clicked()
        {
            acts.push(Action::Insights(InsightsAct::Evolution(Act::Limit(more))));
        }
    });
}

/// A cell's colour and mark.
fn cell_look(
    t: &Tokens,
    m: &Matrix,
    tally: &Tally,
    measure: Measure,
) -> (egui::Color32, Option<Mark>) {
    let v = m.value(tally, measure);
    let fill = match measure {
        Measure::Balance => common::diverging(t, v),
        Measure::Lines | Measure::Frequency => common::heat(t, v),
    };
    let on = common::text_on(fill);
    let mark = if tally.binary_only() {
        Some(Mark::Hatched(on))
    } else if tally.renamed > 0 {
        Some(Mark::Glyph("R", on))
    } else if tally.lines == 0 {
        Some(Mark::Hollow(t.text_dim))
    } else if measure == Measure::Balance {
        Some(match v {
            v if v <= -0.34 => Mark::Glyph("+", on),
            v if v >= 0.34 => Mark::Glyph("−", on),
            _ => Mark::Glyph("=", on),
        })
    } else {
        None
    };
    (fill, mark)
}

/// What a cell's tooltip (and the table) says of it.
pub(in crate::repo::insights) fn describe(tally: &Tally, folder: bool) -> String {
    let commits = tally.commits().len();
    let mut parts = vec![format!(
        "+{} −{} ({} lines) in {}",
        tally.added,
        tally.deleted,
        tally.lines,
        plural(commits, "commit")
    )];
    if folder {
        parts.push(plural(tally.changes.len(), "file change"));
    }
    if tally.binary > 0 {
        parts.push(format!(
            "{} binary",
            plural(tally.binary as usize, "change")
        ));
    }
    if tally.renamed > 0 {
        parts.push(format!(
            "{} renamed",
            plural(tally.renamed as usize, "file")
        ));
    }
    parts.join(" · ")
}

pub(super) fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// The heatmap: what was clicked or opened.
pub(super) fn grid(ui: &mut Ui, built: &Built, selected: Option<(usize, usize)>) -> Option<Opened> {
    let t = Tokens::get(ui.ctx());
    let m = &built.matrix;
    let measure = built.options.measure;
    // The boundary of a cut history is a column of its own, on the left.
    let off = usize::from(m.boundary);
    let label = |c: usize| -> String {
        if c < off {
            "⟨ older not read".to_owned()
        } else {
            m.columns
                .get(c - off)
                .map(|col| col.label.clone())
                .unwrap_or_default()
        }
    };
    let row_label = |r: usize| -> String {
        m.rows
            .get(r)
            .map(|row| {
                if row.folder {
                    format!("{}/", row.shown())
                } else {
                    row.shown().to_owned()
                }
            })
            .unwrap_or_default()
    };
    let open_hint = |r: usize, c: usize| -> &'static str {
        let folder = m.rows.get(r).is_some_and(|row| row.folder);
        let many = m.cells.get(&(r, c)).is_some_and(|t| t.commits().len() > 1);
        if folder || many {
            "Click: list the commits"
        } else {
            "Click: open the diff"
        }
    };
    let limit = built.history.limit;
    let cell = |r: usize, c: usize| -> Option<Cell> {
        if c < off {
            return Some(Cell {
                fill: t.field,
                mark: Some(Mark::Hatched(t.warning)),
                tip: format!(
                    "The history read ends here (limit {limit}): older changes aren't counted"
                ),
            });
        }
        let tally = m.cells.get(&(r, c - off))?;
        let (fill, mark) = cell_look(&t, m, tally, measure);
        let folder = m.rows.get(r).is_some_and(|row| row.folder);
        Some(Cell {
            fill,
            mark,
            tip: format!("{}\n{}", describe(tally, folder), open_hint(r, c - off)),
        })
    };
    let picked = heat_grid::show(
        ui,
        "insights-evolution",
        Layout {
            label_w: 260.0,
            ..Layout::default()
        },
        (m.rows.len(), m.columns.len() + off),
        row_label,
        label,
        cell,
        selected.map(|(r, c)| (r, c + off)),
    );
    if let Some((r, c)) = picked.cell
        && c >= off
    {
        return Some(Opened::Cell(r, c - off));
    }
    if let Some(c) = picked.column
        && c >= off
    {
        return Some(Opened::Column(c - off));
    }
    picked.row.map(Opened::Row)
}
