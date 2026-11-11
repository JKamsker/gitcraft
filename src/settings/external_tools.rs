//! Settings → Git → External tools: which diff and merge tool _Open in diff tool_ and _Resolve
//! in merge tool_ start, git's own or a preset, and where a preset's program is.

use egui::Ui;

use super::layout::{self, Card};
use super::{COMBO_WIDTH, FIELD_WIDTH, Settings};
use crate::git::external_tool::{Kind, Preset, ToolChoice};

/// What the section says under its title.
pub(super) fn intro() -> &'static str {
    crate::i18n::text("interaction-tools-external-intro")
}

impl Settings {
    pub(super) fn external_tools_ui(&mut self, card: &mut Card) {
        card.row(
            "Diff tool",
            "difftool beyond compare araxis vs code meld kdiff3 p4merge winmerge external",
            |ui| tool_ui(ui, &mut self.diff_tool, Kind::Diff),
        );
        card.row(
            "Merge tool",
            "mergetool conflicts beyond compare araxis vs code meld kdiff3 p4merge winmerge external",
            |ui| tool_ui(ui, &mut self.merge_tool, Kind::Merge),
        );
    }
}

/// The tool picker, and the program of a preset.
fn tool_ui(ui: &mut Ui, choice: &mut ToolChoice, kind: Kind) {
    ui.vertical(|ui| {
        let git_own =
            crate::i18n::message("interaction-tools-git-tool", &[("key", kind.key().into())]);
        egui::ComboBox::from_id_salt(("settings-external-tool", kind.noun()))
            .width(layout::fit(ui, COMBO_WIDTH))
            .selected_text(
                choice
                    .preset
                    .map_or(git_own.clone(), |p| p.label().to_owned()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut choice.preset, None, git_own);
                for preset in Preset::ALL {
                    ui.selectable_value(&mut choice.preset, Some(preset), preset.label());
                }
            });
        ui.add_space(4.0);
        match choice.preset {
            None => {
                layout::note(
                    ui,
                    crate::i18n::text(match kind {
                        Kind::Diff => "interaction-tools-diff-fallback",
                        Kind::Merge => "interaction-tools-merge-fallback",
                    }),
                );
            }
            Some(preset) => {
                ui.add(
                    layout::field(&mut choice.program)
                        .hint_text(program_hint(preset))
                        .desired_width(layout::fit(ui, FIELD_WIDTH)),
                )
                .on_hover_text(crate::i18n::text("interaction-tools-program-help"));
            }
        }
    });
}

/// What the program field shows while empty.
fn program_hint(preset: Preset) -> String {
    let exe = if cfg!(windows) {
        preset.windows_places()[0]
            .1
            .rsplit('\\')
            .next()
            .unwrap_or_default()
    } else {
        preset.default_program()
    };
    crate::i18n::message("interaction-tools-program-hint", &[("program", exe.into())])
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_tool_picker_localizes_whole_fallback_and_preserves_choice_and_config_key() {
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, choice: &mut ToolChoice| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                tool_ui(ui, choice, Kind::Diff);
            },
            ToolChoice::default(),
        );
        h.run();
        h.get_by_value("Wie in Git konfiguriert (diff.tool)");
        h.get_by_label(
            "Das Werkzeug aus diff.tool in deiner Git-Konfiguration, ersatzweise aus merge.tool",
        );
        assert!(h.state().preset.is_none());
        assert_eq!(Kind::Diff.key(), "diff.tool");
        assert!(intro().contains(r"\\wsl.localhost"));
        assert!(
            program_hint(Preset::ALL[0])
                .starts_with("Automatisch gefunden oder vollständiger Pfad zu ")
        );
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_value("As set in git (diff.tool)");
        assert!(h.state().preset.is_none());
    }
}
