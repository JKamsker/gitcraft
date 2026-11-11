//! Inline diff note editor.

use super::*;

/// The panel under a diff to write a note on the line picked.
pub(in crate::repo) fn note_panel(
    ui: &mut Ui,
    t: &Tokens,
    agents: &mut AgentsState,
    data: &DiffData,
    source: &str,
    acts: &mut Vec<Action>,
) {
    let tray = &mut agents.tray;
    let Some(target) = tray.target.clone() else {
        return;
    };
    tray.shown = ui.ctx().cumulative_pass_nr();
    let code = data
        .lines
        .iter()
        .find(|l| match target.side {
            DiffSide::Left => l.kind != Kind::Add && l.old.map(u64::from) == Some(target.line),
            DiffSide::Right => l.kind != Kind::Del && l.new.map(u64::from) == Some(target.line),
        })
        .map(|l| l.text.clone())
        .unwrap_or_default();
    egui::Panel::bottom("agent_note")
        .frame(
            egui::Frame::new()
                .fill(t.card)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .stroke(Stroke::new(1.0, t.separator)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-agent-note-location",
                        &[
                            ("path", target.path.as_str().into()),
                            ("line", target.line.into()),
                            (
                                "old",
                                if target.side == DiffSide::Left {
                                    "yes"
                                } else {
                                    "no"
                                }
                                .into(),
                            ),
                        ],
                    ))
                    .font(theme::mono(11.5))
                    .color(t.text_dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::icons::small_button(
                        ui,
                        "x",
                        crate::i18n::text("repo-workflows-agents-note-panel-961b2a04"),
                    )
                    .clicked()
                    {
                        acts.push(tray_act(TrayAct::Close));
                    }
                });
            });
            let edit = ui.add(
                egui::TextEdit::multiline(&mut tray.text)
                    .hint_text(crate::i18n::text(
                        "repo-workflows-agents-note-panel-bb064d10",
                    ))
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut tray.focus) {
                edit.request_focus();
            }
            let submit = edit.has_focus()
                && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
            let valid = !tray.text.trim().is_empty();
            ui.horizontal(|ui| {
                if crate::repo::primary_button(
                    ui,
                    crate::i18n::text("repo-workflows-agents-note-panel-63565c04"),
                    valid,
                    120.0,
                )
                .on_hover_text(format!("{}+Enter", crate::manual::command()))
                .on_disabled_hover_text(crate::i18n::text(
                    "repo-workflows-agents-note-panel-4254e046",
                ))
                .clicked()
                    || submit && valid
                {
                    acts.push(tray_act(TrayAct::Add {
                        code: code.clone(),
                        source: source.to_owned(),
                    }));
                }
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-agents-note-panel-d065164e",
                    ))
                    .small()
                    .color(t.text_faint),
                );
            });
        });
}
