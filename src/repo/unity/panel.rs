//! The Unity set-up panel: what is set up for git and what isn't, with a fix for each thing
//! gitcraft can fix, and Unity's Smart Merge.

use egui::{RichText, Ui};

use super::{UnityAct, UnityState};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn unity_panel(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if !self.unity.panel_open {
            return;
        }
        let state = &self.unity;
        let remote = crate::remote::is_remote(&self.path);
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut close = false;
        let resp = egui::Modal::new(egui::Id::new("unity-setup"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(540.0);
                close = header(ui, &t, state, acts);
                ui.add_space(8.0);
                checks(ui, &t, state, acts);
                ui.add_space(12.0);
                ui.separator();
                smart_merge(ui, &t, state, remote, acts);
            });
        if close || resp.should_close() {
            acts.push(Action::Unity(UnityAct::ClosePanel));
        }
    }
}

fn header(ui: &mut Ui, t: &Tokens, state: &UnityState, acts: &mut Vec<Action>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        icons::inline(ui, "gamepad-2", 16.0, t.icon);
        ui.label(RichText::new("Unity project").font(theme::semibold(16.0)));
        if let Some(p) = &state.project {
            let at = if p.root.is_empty() {
                "at the root".to_owned()
            } else {
                format!("in {}", p.root)
            };
            let version = p
                .version
                .as_deref()
                .map(|v| format!(" · Unity {v}"))
                .unwrap_or_default();
            ui.label(RichText::new(format!("{at}{version}")).color(t.text_dim));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                close = true;
            }
            if icons::small_button(ui, "refresh-cw", "Check again").clicked() {
                acts.push(Action::Unity(UnityAct::Check));
            }
        });
    });
    if let Some(busy) = &state.busy {
        ui.label(RichText::new(format!("{busy}…")).color(t.text_dim));
    }
    close
}

/// One line of the checklist: a mark, what it is, and what to do about it.
fn check_row(ui: &mut Ui, t: &Tokens, ok: Option<bool>, text: &str, then: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let (icon, color) = match ok {
            Some(true) => ("check", t.success),
            Some(false) => ("circle-alert", t.warning),
            None => ("circle-alert", t.text_faint),
        };
        icons::inline(ui, icon, 14.0, color);
        ui.add(egui::Label::new(RichText::new(text)).wrap());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), then);
    });
}

fn checks(ui: &mut Ui, t: &Tokens, state: &UnityState, acts: &mut Vec<Action>) {
    let setup = match &state.setup {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(RichText::new("Checking the project…").color(t.text_dim));
            });
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, format!("Could not check the project: {e}"));
            return;
        }
        Some(Ok(s)) => s,
    };
    let idle = state.busy.is_none();
    let unity_fix = |ui: &mut Ui, where_: &str| {
        ui.label(RichText::new(where_).color(t.text_dim).small());
    };
    let text = match setup.force_text {
        Some(true) => "Assets are saved as text (Force Text): git can diff and merge them.",
        Some(false) => "Assets aren't saved as text: git can't merge scenes and prefabs.",
        None => "Couldn't read how assets are saved (ProjectSettings/EditorSettings.asset).",
    };
    check_row(ui, t, setup.force_text, text, |ui| {
        if setup.force_text == Some(false) {
            unity_fix(
                ui,
                "In Unity: Project Settings → Editor → Asset Serialization: Force Text",
            );
        }
    });
    let text = match setup.visible_meta {
        Some(true) => ".meta files are visible, so they are committed with their assets.",
        Some(false) => ".meta files are hidden: they must be committed with their assets.",
        None => "Couldn't read whether .meta files are visible.",
    };
    check_row(ui, t, setup.visible_meta, text, |ui| {
        if setup.visible_meta == Some(false) {
            unity_fix(
                ui,
                "In Unity: Project Settings → Version Control → Mode: Visible Meta Files",
            );
        }
    });
    let missing = setup.missing_attributes.len();
    let text = if missing == 0 {
        ".gitattributes merges Unity's YAML with Smart Merge and keeps binaries in LFS.".to_owned()
    } else {
        format!(
            ".gitattributes has no line for {missing} of Unity's file types ({}…).",
            setup
                .missing_attributes
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    check_row(ui, t, Some(missing == 0), &text, |ui| {
        if missing > 0
            && ui
                .add_enabled(idle, egui::Button::new("Add them"))
                .on_hover_text(
                    "Appends the lines to .gitattributes: YAML merged by unityyamlmerge with LF \
                     line ends, binaries in LFS. Commit it to share them.",
                )
                .clicked()
        {
            acts.push(Action::Unity(UnityAct::AddAttributes));
        }
    });
    let text = if setup.unignored.is_empty() {
        "Unity's generated folders (Library, Temp, Logs…) are ignored.".to_owned()
    } else {
        format!("Not ignored: {}.", setup.unignored.join(", "))
    };
    check_row(ui, t, Some(setup.unignored.is_empty()), &text, |ui| {
        if !setup.unignored.is_empty()
            && ui
                .add_enabled(idle, egui::Button::new("Ignore them"))
                .on_hover_text("Appends them to .gitignore. Commit it to share it.")
                .clicked()
        {
            acts.push(Action::Unity(UnityAct::AddIgnores));
        }
    });
}

fn smart_merge(ui: &mut Ui, t: &Tokens, state: &UnityState, remote: bool, acts: &mut Vec<Action>) {
    ui.label(RichText::new("Smart Merge").font(theme::semibold(13.0)));
    ui.label(
        RichText::new(
            "UnityYAMLMerge merges scenes and prefabs by their structure. Registered, git uses it \
             for the files .gitattributes marks merge=unityyamlmerge; the conflict editor offers \
             Solve with UnityYAMLMerge either way.",
        )
        .color(t.text_dim)
        .small(),
    );
    let idle = state.busy.is_none();
    let found = match &state.tool {
        Some(tool) => format!("Found: {}", tool.display()),
        None => "UnityYAMLMerge wasn't found in Unity Hub's editors.".to_owned(),
    };
    check_row(ui, t, Some(state.tool.is_some()), &found, |ui| {
        if ui
            .add_enabled(idle, egui::Button::new("Choose…"))
            .on_hover_text("Pick UnityYAMLMerge from an editor's Tools folder")
            .clicked()
        {
            acts.push(Action::Unity(UnityAct::ChooseTool));
        }
    });
    let registered = state.driver.is_some();
    let text = match &state.driver {
        Some(d) => format!("Registered as this repository's merge driver: {d}"),
        None if remote => {
            "Not registered: on a repository elsewhere, set it up on its own machine.".to_owned()
        }
        None => "Not registered as this repository's merge driver.".to_owned(),
    };
    check_row(ui, t, Some(registered), &text, |ui| {
        let can = idle && state.tool.is_some() && !remote;
        let label = if registered {
            "Register again"
        } else {
            "Register"
        };
        if ui
            .add_enabled(can, egui::Button::new(label))
            .on_hover_text("Sets merge.unityyamlmerge and mergetool.unityyamlmerge in this repository's config")
            .clicked()
        {
            acts.push(Action::Unity(UnityAct::RegisterDriver));
        }
    });
}
