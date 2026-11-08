//! Settings → Custom commands: the commands added to the menus and the palette, and the
//! repositories whose own commands were trusted.

use egui::{RichText, Ui};

use super::layout::{self, Card, Filter, section};
use super::{COMBO_WIDTH, Settings};
use crate::custom_commands::{self, CustomCommand, OutputTo, Placement, Prompt, PromptKind};
use crate::theme::Tokens;

/// The page: the commands, the variables they can use, the trusted repository files.
pub(super) fn page(settings: &mut Settings, ui: &mut Ui, f: &Filter) {
    section(
        ui,
        f,
        "Custom commands",
        Some(crate::i18n::text("interaction-preferences-commands-intro")),
        |card| commands(card, &mut settings.custom_commands),
    );
    section(
        ui,
        f,
        "Variables",
        Some(crate::i18n::text("interaction-preferences-variables-intro")),
        |card| {
            card.compact();
            for (name, what) in custom_commands::BUILT_IN {
                card.full(&format!("{name} {what} variable"), |ui| {
                    ui.horizontal(|ui| {
                        let width = layout::label_width(ui);
                        let start = ui.cursor().left();
                        ui.label(RichText::new(format!("${name}")).font(crate::theme::mono(12.5)));
                        let used = ui.cursor().left() - start;
                        ui.add_space((width + 8.0 - used).max(8.0));
                        layout::note(ui, custom_commands::variable_description(name, what));
                    });
                });
            }
        },
    );
    section(
        ui,
        f,
        "Repository commands",
        Some(crate::i18n::text("interaction-preferences-repo-intro")),
        |card| trusted(card, &mut settings.trusted_command_files),
    );
}

/// Every command, then the button that adds one.
fn commands(card: &mut Card, list: &mut Vec<CustomCommand>) {
    let mut remove = None;
    let mut up = None;
    for (i, command) in list.iter_mut().enumerate() {
        let found_by = format!("{} {} command", command.name, command.command);
        card.full(&found_by, |ui| match command_ui(ui, i, command) {
            Edit::Remove => remove = Some(i),
            Edit::Up => up = Some(i),
            Edit::None => {}
        });
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    if let Some(i) = up.filter(|&i| i > 0) {
        list.swap(i, i - 1);
    }
    card.full("add new custom command script", |ui| {
        ui.horizontal(|ui| {
            if ui
                .button(crate::i18n::text("interaction-preferences-add-command"))
                .clicked()
            {
                list.push(CustomCommand::default());
            }
            if list.is_empty() {
                layout::note(ui, crate::i18n::text("interaction-preferences-none"));
            }
        });
    });
}

/// What a command's row asks of the list.
enum Edit {
    None,
    Remove,
    Up,
}

/// One command: name, where, output, confirmation, command line, prompts; what is wrong with it.
fn command_ui(ui: &mut Ui, i: usize, c: &mut CustomCommand) -> Edit {
    let t = Tokens::get(ui.ctx());
    let mut edit = Edit::None;
    ui.horizontal(|ui| {
        let room = ui.available_width() - 60.0;
        ui.add(
            layout::field(&mut c.name)
                .hint_text(crate::i18n::text("interaction-preferences-name"))
                .desired_width((room * 0.4).min(220.0)),
        );
        egui::ComboBox::from_id_salt(("custom-where", i))
            .width(layout::fit(ui, COMBO_WIDTH * 0.7))
            .selected_text(c.placement.label())
            .show_ui(ui, |ui| {
                for p in Placement::ALL {
                    ui.selectable_value(&mut c.placement, p, p.label());
                }
            });
        egui::ComboBox::from_id_salt(("custom-output", i))
            .width(layout::fit(ui, 110.0))
            .selected_text(crate::i18n::message(
                "interaction-preferences-output",
                &[("destination", c.output.label().into())],
            ))
            .show_ui(ui, |ui| {
                for o in OutputTo::ALL {
                    ui.selectable_value(&mut c.output, o, o.label());
                }
            });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::icons::small_button(
                ui,
                "x",
                crate::i18n::text("interaction-preferences-remove"),
            )
            .clicked()
            {
                edit = Edit::Remove;
            }
            if i > 0
                && crate::icons::small_button(
                    ui,
                    "chevron-up",
                    crate::i18n::text("interaction-preferences-up"),
                )
                .clicked()
            {
                edit = Edit::Up;
            }
        });
    });
    ui.add(
        egui::TextEdit::multiline(&mut c.command)
            .font(crate::theme::mono(12.5))
            .hint_text("git log -1 --format=%B \"$SHA\"")
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut c.confirm,
            crate::i18n::text("interaction-preferences-confirm"),
        );
        if ui
            .button(crate::i18n::text("interaction-preferences-add-prompt"))
            .clicked()
        {
            c.prompts.push(Prompt {
                variable: format!("INPUT{}", c.prompts.len() + 1),
                ..Prompt::default()
            });
        }
    });
    prompts_ui(ui, i, &mut c.prompts);
    let fresh = c.name.trim().is_empty() && c.command.trim().is_empty();
    if let Some(problem) = c.problem().filter(|_| !fresh) {
        ui.label(
            RichText::new(crate::i18n::message(
                "interaction-preferences-problem",
                &[("problem", custom_commands::display_problem(&problem).into())],
            ))
            .color(t.danger)
            .small(),
        );
    }
    edit
}

/// The prompts of command `i`: variable, question, kind, what the field starts with.
fn prompts_ui(ui: &mut Ui, i: usize, prompts: &mut Vec<Prompt>) {
    let mut remove = None;
    for (j, p) in prompts.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let room = ui.available_width() - 130.0;
            ui.label("$");
            ui.add(
                layout::field(&mut p.variable)
                    .font(crate::theme::mono(12.5))
                    .hint_text("NAME")
                    .desired_width((room * 0.25).min(120.0)),
            );
            ui.add(
                layout::field(&mut p.label)
                    .hint_text(crate::i18n::text("interaction-preferences-question"))
                    .desired_width((room * 0.4).min(200.0)),
            );
            egui::ComboBox::from_id_salt(("custom-prompt", i, j))
                .width(80.0)
                .selected_text(p.kind.label())
                .show_ui(ui, |ui| {
                    for k in PromptKind::ALL {
                        ui.selectable_value(&mut p.kind, k, k.label());
                    }
                });
            let rest = (ui.available_width() - 32.0).max(60.0);
            ui.add(
                layout::field(&mut p.default)
                    .hint_text(crate::i18n::text("interaction-preferences-default"))
                    .desired_width(rest),
            );
            if crate::icons::small_button(
                ui,
                "x",
                crate::i18n::text("interaction-preferences-remove-prompt"),
            )
            .clicked()
            {
                remove = Some(j);
            }
        });
    }
    if let Some(j) = remove {
        prompts.remove(j);
    }
}

/// The repositories whose file was trusted, each with a button to forget it.
fn trusted(card: &mut Card, list: &mut Vec<custom_commands::TrustedFile>) {
    let mut forget = None;
    for (i, file) in list.iter().enumerate() {
        card.full(&format!("{} trusted repository", file.repo), |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::remote::display(std::path::Path::new(&file.repo)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(crate::i18n::text("interaction-preferences-forget"))
                        .on_hover_text(crate::i18n::text("interaction-preferences-forget-help"))
                        .clicked()
                    {
                        forget = Some(i);
                    }
                });
            });
        });
    }
    if let Some(i) = forget {
        list.remove(i);
    }
    if list.is_empty() {
        card.full("trusted repository none", |ui| {
            layout::note(
                ui,
                crate::i18n::text("interaction-preferences-none-trusted"),
            );
        });
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_command_editor_translates_controls_and_preserves_command_and_prompt_variables() {
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, command: &mut CustomCommand| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                command_ui(ui, 0, command);
            },
            CustomCommand {
                name: "User 日本語".into(),
                command: "git show \"$SHA\"".into(),
                ..CustomCommand::default()
            },
        );
        h.run();
        h.get_by_value("Commit-Menü");
        h.get_by_value("Ausgabe: Kurzmeldung");
        h.get_by_label("Vor dem Ausführen nachfragen").click();
        h.run();
        assert!(!h.state().confirm);
        h.get_by_label("Abfrage hinzufügen").click();
        h.run();
        assert_eq!(h.state().prompts[0].variable, "INPUT1");
        assert_eq!(h.state().command, "git show \"$SHA\"");
        assert_eq!(h.state().name, "User 日本語");
        assert_eq!(h.state().placement, Placement::Commit);
        assert_eq!(serde_json::to_value(h.state()).unwrap()["where"], "commit");
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Ask before running");
        h.get_by_value("Output: Toast");
        assert_eq!(h.state().prompts[0].variable, "INPUT1");
    }
}
