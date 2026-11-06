//! Settings → Git → External tools: which diff and merge tool _Open in diff tool_ and _Resolve
//! in merge tool_ start, git's own or a preset, and where a preset's program is.

use egui::Ui;

use super::layout::{self, Card};
use super::{COMBO_WIDTH, FIELD_WIDTH, Settings};
use crate::git::external_tool::{Kind, Preset, ToolChoice};

/// What the section says under its title.
pub(super) const INTRO: &str = "Open in diff tool and Resolve in merge tool (a file's right-click \
     menu, the conflict editor) start one of these and refresh the view when it closes. In WSL, \
     a preset is the Windows program, given the files through \\\\wsl.localhost; over SSH, the \
     tool runs on that machine and has to be installed there.";

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
        let git_own = format!("As set in git ({})", kind.key());
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
                    format!(
                        "git {} --tool=<{}> with your git config's tool",
                        kind.command(),
                        kind.key()
                    ),
                );
            }
            Some(preset) => {
                ui.add(
                    layout::field(&mut choice.program)
                        .hint_text(program_hint(preset))
                        .desired_width(layout::fit(ui, FIELD_WIDTH)),
                )
                .on_hover_text(
                    "The program to run, when it isn't found by itself (on the PATH, or in \
                     Program Files on Windows). Empty: found by itself.",
                );
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
    format!("Found by itself, or the full path of {exe}")
}
