//! Split group cards and move destinations.

use super::*;

/// The new commits: each with its colour, number of changes and message; the selected one
/// gets what is clicked.
pub(super) fn groups(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("split_groups_list")
        .auto_shrink(false)
        .show(ui, |ui| {
            if editor.busy {
                ui.disable();
            }
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-surgery-groups-e87a043a"))
                    .color(t.text_dim)
                    .small(),
            );
            ui.add_space(6.0);
            let n = editor.groups.len();
            for g in 0..n {
                ui.push_id(("split_group", editor.id, g), |ui| {
                    if group_card(ui, t, editor, g, &mut remove) {
                        editor.active = g;
                    }
                });
                ui.add_space(6.0);
            }
            if ui
                .add_enabled(
                    n < MAX_GROUPS,
                    egui::Button::new(crate::i18n::text("repo-workflows-surgery-groups-25dc2988")),
                )
                .on_disabled_hover_text(crate::i18n::message(
                    "repo-workflows-surgery-groups-536f4c75",
                    &[("MAX-GROUPS", (MAX_GROUPS).into())],
                ))
                .clicked()
            {
                editor.add_group();
            }
        });
    if let Some(g) = remove {
        editor.remove_group(g);
    }
}

/// New commit `g`'s card; whether it was clicked (or typed in), which selects it.
pub(super) fn group_card(
    ui: &mut Ui,
    t: &Tokens,
    editor: &mut SplitEditor,
    g: usize,
    remove: &mut Option<usize>,
) -> bool {
    let n = editor.groups.len();
    let selected = editor.active == g;
    let count = editor.counts.get(g).copied().unwrap_or(0);
    let frame = egui::Frame::new()
        .fill(if selected { t.row_selected } else { t.field })
        .stroke(Stroke::new(
            1.0,
            if selected {
                color(t, g)
            } else {
                t.field_border
            },
        ))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8));
    let shown = frame.show(ui, |ui| {
        let mut picked = false;
        ui.horizontal(|ui| {
            chip(ui, t, g);
            let title = ui.add(
                egui::Label::new(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-surgery-group-summary",
                        &[("index", (g + 1).into()), ("count", (count).into())],
                    ))
                    .font(theme::medium(12.5)),
                )
                .selectable(false)
                .sense(Sense::click()),
            );
            picked |= title.clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let heir = SplitEditor::heir(g);
                let tip = if heir == g {
                    crate::i18n::text("repo-workflows-surgery-group-card-7aca66ea").to_owned()
                } else {
                    crate::i18n::message(
                        "repo-workflows-surgery-group-card-45e7c009",
                        &[("heir-1-1", (heir + 1).into())],
                    )
                };
                if n > 2 && icons::small_button(ui, "x", &tip).clicked() {
                    *remove = Some(g);
                }
            });
        });
        let Some(group) = editor.groups.get_mut(g) else {
            return picked;
        };
        let subject = ui.add(
            egui::TextEdit::singleline(&mut group.subject)
                .hint_text(crate::i18n::text(
                    "repo-workflows-surgery-group-card-77f7cfc2",
                ))
                .desired_width(f32::INFINITY),
        );
        let body = ui.add(
            egui::TextEdit::multiline(&mut group.body)
                .hint_text(crate::i18n::text(
                    "repo-workflows-surgery-group-card-526e0087",
                ))
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        picked || subject.gained_focus() || body.gained_focus()
    });
    shown.inner || shown.response.interact(Sense::click()).clicked()
}

/// Where moved changes can go: the other commits of the branch, newest first, or the working
/// changes.
pub(super) fn destinations(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let Mode::Move(m) = &editor.mode else {
        return;
    };
    let mut picked = None;
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-surgery-destinations-6f10c3fa",
        ))
        .color(t.text_dim)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        chip(ui, t, 0);
        ui.label(RichText::new(&editor.groups[0].subject).color(t.text_dim));
    });
    ui.horizontal(|ui| {
        chip(ui, t, 1);
        ui.add(egui::Label::new(RichText::new(&editor.groups[1].subject)).truncate());
    });
    ui.add_space(8.0);
    if editor.busy {
        ui.disable();
    }
    if ui
        .selectable_label(
            m.dest == Some(Dest::Work),
            crate::i18n::text("repo-workflows-surgery-destinations-b4861450"),
        )
        .on_hover_text(crate::i18n::text(
            "repo-workflows-surgery-destinations-9b4ffd73",
        ))
        .clicked()
    {
        picked = Some(Dest::Work);
    }
    ui.separator();
    // Newest first, the commit itself in its place (not a choice): above it newer commits,
    // below it older ones, from which more of the branch is rewritten.
    let rows = m.targets.len() + 1 + usize::from(m.truncated);
    let mine = m.targets.iter().take_while(|t| t.depth < m.depth).count();
    egui::ScrollArea::vertical()
        .id_salt(("move_targets", editor.id))
        .auto_shrink(false)
        .show_rows(ui, 22.0, rows, |ui, range| {
            for row in range {
                if row == mine {
                    ui.label(
                        RichText::new(crate::i18n::message(
                            "repo-workflows-surgery-destinations-9e3eee5d",
                            &[("short-editor-hash-1", (short(&editor.hash)).into())],
                        ))
                        .color(t.text_faint)
                        .italics(),
                    );
                    continue;
                }
                let i = if row < mine { row } else { row - 1 };
                let Some(target) = m.targets.get(i) else {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-surgery-destinations-aea2bb52",
                        ))
                        .color(t.text_faint)
                        .small(),
                    );
                    continue;
                };
                let text = format!("{}  {}", short(&target.hash), target.summary);
                if ui
                    .add(egui::Button::selectable(m.dest == Some(Dest::Commit(i)), text).truncate())
                    .clicked()
                {
                    picked = Some(Dest::Commit(i));
                }
            }
        });
    if m.targets.is_empty() && !m.truncated {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-surgery-destinations-a3863fa7",
            ))
            .color(t.text_faint)
            .small(),
        );
    }
    if let Some(dest) = picked {
        editor.set_dest(dest);
    }
}
