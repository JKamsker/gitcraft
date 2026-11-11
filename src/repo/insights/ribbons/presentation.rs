//! Ribbons comparison readout, controls, and selected commit details.

use super::*;

/// The line above the picture: what is compared (a menu of what can be), drawing in or still.
pub(super) fn bar(
    ui: &mut Ui,
    state: &State,
    sources: &[Source],
    reduced: bool,
    acts: &mut Vec<Action>,
) {
    ui.horizontal_wrapped(|ui| {
        let shown = state
            .source
            .as_ref()
            .map_or_else(|| "Nothing to compare".to_owned(), Source::label);
        egui::ComboBox::from_id_salt("ribbons-source")
            .selected_text(shown)
            .width(360.0)
            .show_ui(ui, |ui| {
                for source in sources {
                    let on = state.source.as_ref() == Some(source);
                    if ui.selectable_label(on, source.label()).clicked() && !on {
                        acts.push(Action::Insights(InsightsAct::Ribbons(Act::Show(
                            source.clone(),
                        ))));
                    }
                }
                if sources.is_empty() {
                    ui.label("Nothing to compare");
                }
            });
        let mut still = state.still || reduced;
        let toggle = ui.add_enabled(!reduced, egui::Checkbox::new(&mut still, "Static"));
        let toggle = if reduced {
            toggle.on_disabled_hover_text("The system asks for reduced motion: always static")
        } else {
            toggle.on_hover_text("Show the ribbons without drawing them in")
        };
        if toggle.changed() {
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::ToggleStill)));
        }
        if let Some(data) = state.data.ready() {
            let counts: Vec<String> = data
                .counts()
                .iter()
                .map(|(c, n)| format!("{n} {}", c.label()))
                .collect();
            let t = Tokens::get(ui.ctx());
            ui.label(RichText::new(counts.join(", ")).color(t.text_dim));
        }
    });
}

/// The colour a class is drawn in; its letter says the same.
pub(in crate::repo::insights) fn color(class: Class, t: &Tokens) -> egui::Color32 {
    match class {
        Class::Unchanged => t.text_faint,
        Class::Reordered => t.accent,
        Class::Reworded => t.lanes[4],
        Class::Modified => t.warning,
        Class::Squashed => t.lanes[6],
        Class::Split => t.lanes[8],
        Class::Dropped => t.danger,
        Class::Added => t.success,
    }
}

/// What a class is called where `sides` (two branches, neither older) makes added and dropped
/// "only on the right / left".
pub(in crate::repo::insights) fn label(class: Class, sides: bool) -> &'static str {
    match class {
        Class::Dropped if sides => "only on the left",
        Class::Added if sides => "only on the right",
        _ => class.label(),
    }
}

pub(super) fn legend(ui: &mut Ui, sides: bool) {
    let t = Tokens::get(ui.ctx());
    let entries: Vec<(Mark, &str)> = Class::ALL
        .iter()
        .map(|&c| (Mark::Glyph(c.glyph(), color(c, &t)), label(c, sides)))
        .chain([
            (
                common::evidence_line(Evidence::Observed, t.text_dim),
                "exact: the same patch-id",
            ),
            (
                common::evidence_line(Evidence::Inferred, t.text_dim),
                "inferred: similar, or sharing changed lines",
            ),
        ])
        .collect();
    common::legend(ui, &entries);
}

/// What was clicked: something to show, a commit to select.
#[derive(Default)]
pub(in crate::repo::insights) struct Picked {
    pub(super) pick: Option<Pick>,
    pub(super) commit: Option<String>,
}

/// A commit as a row names it.
pub(super) fn named(c: &git::SeriesCommit) -> String {
    let empty = if c.empty { " (changes nothing)" } else { "" };
    format!("{}. {} {}{empty}", c.index, short(&c.hash), c.summary)
}

/// The ribbons and the commits without one, as a table.
pub(super) fn table_ui(
    ui: &mut Ui,
    data: &Ribbons,
    selected: Option<Pick>,
    sides: bool,
    h: f32,
) -> Picked {
    let mut picked = Picked::default();
    let widths = [100.0, 80.0, 240.0, 240.0];
    let head = ["What", "Evidence", "Old", "New", "Why"].map(str::to_owned);
    common::table_row(ui, &widths, &head, true, false);
    let mut rows: Vec<(Pick, [String; 5])> = Vec::new();
    let mut order: Vec<usize> = (0..data.ribbons.len()).collect();
    order.sort_by_key(|&i| (data.ribbons[i].new, data.ribbons[i].old));
    for i in order {
        let r = &data.ribbons[i];
        let moved = if r.moved { ", moved" } else { "" };
        rows.push((
            Pick::Ribbon(i),
            [
                format!("{}{moved}", label(r.class, sides)),
                r.evidence.label().to_owned(),
                named(&data.old[r.old].commit),
                named(&data.new[r.new].commit),
                r.why.clone(),
            ],
        ));
    }
    let unlinked = |old: bool, at: usize| data.ribbons_of(old, at).next().is_none();
    for (i, end) in data
        .old
        .iter()
        .enumerate()
        .filter(|(i, _)| unlinked(true, *i))
    {
        rows.push((
            Pick::Old(i),
            [
                label(end.class, sides).to_owned(),
                Evidence::Observed.label().to_owned(),
                named(&end.commit),
                "—".to_owned(),
                "range-diff paired it with nothing, nor do its changed lines match".to_owned(),
            ],
        ));
    }
    for (i, end) in data
        .new
        .iter()
        .enumerate()
        .filter(|(i, _)| unlinked(false, *i))
    {
        rows.push((
            Pick::New(i),
            [
                label(end.class, sides).to_owned(),
                Evidence::Observed.label().to_owned(),
                "—".to_owned(),
                named(&end.commit),
                "range-diff paired it with nothing, nor do its changed lines match".to_owned(),
            ],
        ));
    }
    egui::ScrollArea::vertical()
        .id_salt("ribbons-table")
        .max_height(h)
        .auto_shrink([false, true])
        .show_rows(ui, 22.0, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (pick, cells) in &rows[range] {
                let on = selected == Some(*pick);
                if common::table_row(ui, &widths, cells, false, on).clicked() {
                    picked.pick = Some(*pick);
                }
            }
        });
    picked
}

/// What is selected: what it is, where to go from it, and its patch.
pub(super) fn detail(
    ui: &mut Ui,
    data: &Ribbons,
    pick: Pick,
    patch: &Slot<diff::DiffData>,
    source: &Source,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let sides = source.sides();
    let (title, line, commits, what) = match pick {
        Pick::Ribbon(i) => {
            let Some(r) = data.ribbons.get(i) else {
                return;
            };
            let (old, new) = (&data.old[r.old].commit, &data.new[r.new].commit);
            let moved = if r.moved { ", in another place" } else { "" };
            (
                format!("{} → {}", named(old), named(new)),
                format!(
                    "{}{moved}: {} · {}: {}",
                    label(r.class, sides),
                    r.class.meaning(),
                    r.evidence.label(),
                    r.why
                ),
                vec![
                    ("Old commit", old.hash.clone()),
                    ("New commit", new.hash.clone()),
                ],
                if r.pair.is_some() {
                    "How its patch changed, as range-diff shows it"
                } else {
                    "How the two patches differ (range-diff didn't pair them)"
                },
            )
        }
        Pick::Old(i) | Pick::New(i) => {
            let old = matches!(pick, Pick::Old(_));
            let list = if old { &data.old } else { &data.new };
            let Some(end) = list.get(i) else {
                return;
            };
            (
                named(&end.commit),
                format!("{}: {}", label(end.class, sides), end.class.meaning()),
                vec![("Select the commit", end.commit.hash.clone())],
                if old {
                    "Its patch, in the old version"
                } else {
                    "Its patch, in the new version"
                },
            )
        }
    };
    ui.label(RichText::new(title).font(theme::medium(12.5)).color(t.text));
    ui.add(egui::Label::new(RichText::new(line).color(t.text_dim)).wrap());
    ui.horizontal_wrapped(|ui| {
        for (label, hash) in commits {
            if ui.button(label).on_hover_text(hash.clone()).clicked() {
                acts.push(Action::SelectHash(hash));
            }
        }
        let (act, tip) = match source {
            Source::Rewrite(r) => (
                VersionsAct::Rewrite(r.clone()),
                "Every pair in the comparison of versions".to_owned(),
            ),
            Source::Upstream { branch, upstream } => (
                VersionsAct::Branches {
                    old: crate::repo::branch_visibility::remote_ref(upstream),
                    old_label: upstream.clone(),
                    new: crate::repo::branch_visibility::local_ref(branch),
                    new_label: branch.clone(),
                },
                "Every pair in the comparison of versions".to_owned(),
            ),
            Source::Pr { number, .. } => (
                VersionsAct::PrSince(*number),
                format!(
                    "The comparison of versions, which then takes this version of #{number} as \
                     seen, as it always does"
                ),
            ),
        };
        if ui
            .button("Open in the versions view")
            .on_hover_text(tip)
            .clicked()
        {
            acts.push(Action::Versions(act));
        }
        if ui.button("Close").on_hover_text("Esc").clicked() {
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::Deselect)));
        }
    });
    ui.label(RichText::new(what).color(t.text_faint));
    match common::slot_state(ui, patch, "the patch") {
        SlotState::Retry => {
            // Picking it again would let go of it: let go first, then pick.
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::Deselect)));
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::Select(pick))));
        }
        SlotState::Waiting => {}
        SlotState::Ready => {
            let Some(found) = patch.ready() else {
                return;
            };
            if found.lines.is_empty() {
                common::empty(
                    ui,
                    "Nothing changed: the same message and the same changes.",
                );
                return;
            }
            let h = ui.available_height().max(160.0);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            diff::interdiff_rows(&mut inner, found, ("ribbons-patch", format!("{pick:?}")));
        }
    }
}
