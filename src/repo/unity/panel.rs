//! The Unity set-up panel: what is set up for git and what isn't, with a fix for each thing
//! gitcraft can fix, and Unity's Smart Merge.

use egui::{RichText, Ui};

use super::{UnityAct, UnityState};
use crate::git::UnitySetup;
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// What the panel needs besides the Unity state.
struct Context {
    remote: bool,
    /// git-lfs is installed where the repository is, and it uses LFS.
    lfs: bool,
}

impl RepoTab {
    pub(super) fn unity_panel(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if !self.unity.panel_open {
            return;
        }
        let state = &self.unity;
        let here = Context {
            remote: crate::remote::is_remote(&self.path),
            lfs: self.lfs,
        };
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
                ui.set_width(560.0);
                close = header(ui, &t, state, acts);
                ui.add_space(8.0);
                if state.project.is_none() {
                    ui.label(RichText::new("No Unity project here any more.").color(t.text_dim));
                    return;
                }
                checks(ui, &t, state, &here, acts);
                ui.add_space(12.0);
                ui.separator();
                smart_merge(ui, &t, state, &here, acts);
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
        ui.label(RichText::new("Unity set-up").font(theme::semibold(16.0)));
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
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(RichText::new(format!("{busy}…")).color(t.text_dim));
        });
    }
    close
}

/// One line of the checklist: a mark and what it is, the action on its right, and advice
/// below it.
fn check_row(
    ui: &mut Ui,
    t: &Tokens,
    ok: Option<bool>,
    text: &str,
    advice: Option<&str>,
    action: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        let (icon, color) = match ok {
            Some(true) => ("check", t.success),
            Some(false) => ("circle-alert", t.warning),
            None => ("circle-alert", t.text_faint),
        };
        icons::inline(ui, icon, 14.0, color);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            action(ui);
            ui.add(egui::Label::new(RichText::new(text)).wrap());
        });
    });
    if let Some(advice) = advice {
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            ui.label(RichText::new(advice).color(t.text_dim).small());
        });
    }
}

/// "a, b, c, d…" for a list cut at four.
fn some_of(list: &[String]) -> String {
    let shown: Vec<&str> = list.iter().take(4).map(String::as_str).collect();
    let more = if list.len() > 4 { "…" } else { "" };
    format!("{}{more}", shown.join(", "))
}

/// Where the Version Control setting is: Project Settings → Version Control from Unity 2020.1,
/// under Editor before.
fn vcs_place(version: Option<&str>) -> &'static str {
    let major: u32 = version
        .and_then(|v| v.split('.').next())
        .and_then(|m| m.parse().ok())
        .unwrap_or(9999);
    if major >= 2020 {
        "In Unity: Edit → Project Settings → Version Control → Mode: Visible Meta Files"
    } else {
        "In Unity: Edit → Project Settings → Editor → Version Control → Mode: Visible Meta Files"
    }
}

fn checks(ui: &mut Ui, t: &Tokens, state: &UnityState, here: &Context, acts: &mut Vec<Action>) {
    let setup: &UnitySetup = match &state.setup {
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
    let version = state.project.as_ref().and_then(|p| p.version.as_deref());
    let (text, advice) = match setup.force_text {
        Some(true) => (
            "Assets are saved as text (Force Text): git can diff and merge them.",
            None,
        ),
        Some(false) => (
            "Assets aren't saved as text: git can't merge scenes and prefabs.",
            Some(
                "In Unity: Edit → Project Settings → Editor → Asset Serialization → Mode: Force Text",
            ),
        ),
        None => (
            "Couldn't read how assets are saved (ProjectSettings/EditorSettings.asset).",
            None,
        ),
    };
    check_row(ui, t, setup.force_text, text, advice, |_| {});
    let mode = setup.vcs_mode.as_deref();
    let text = match (setup.visible_meta(), mode) {
        (Some(true), Some(m)) if m.contains("Visible Meta Files") => {
            ".meta files are visible, so they can be committed with their assets.".to_owned()
        }
        (Some(true), Some(m)) => format!("Version Control mode is {m}: for git, use Visible Meta Files."),
        (Some(false), _) => ".meta files are hidden: make them visible so they are committed with their assets.".to_owned(),
        _ => "Couldn't read whether .meta files are visible (ProjectSettings/VersionControlSettings.asset).".to_owned(),
    };
    let fine = mode.map(|m| m.contains("Visible Meta Files"));
    let advice = (fine == Some(false)).then(|| vcs_place(version));
    check_row(ui, t, fine, &text, advice, |_| {});
    let yaml = &setup.missing_yaml;
    let text = if yaml.is_empty() {
        "Scenes, prefabs and Unity's other YAML files merge with Smart Merge, with LF line ends."
            .to_owned()
    } else {
        format!(
            "No Smart Merge for {} of Unity's YAML types ({}).",
            yaml.len(),
            some_of(yaml)
        )
    };
    check_row(ui, t, Some(yaml.is_empty()), &text, None, |ui| {
        if !yaml.is_empty()
            && ui
                .add_enabled(idle, egui::Button::new("Add them"))
                .on_hover_text("Appends merge=unityyamlmerge eol=lf lines to .gitattributes. Commit it to share them.")
                .clicked()
        {
            acts.push(Action::Unity(UnityAct::AddAttributes { lfs: false }));
        }
    });
    let bins = &setup.missing_lfs;
    let text = if bins.is_empty() {
        "Unity's usual binaries (textures, models, audio) are kept in LFS.".to_owned()
    } else {
        format!(
            "Not in LFS: {} binary types ({}).",
            bins.len(),
            some_of(bins)
        )
    };
    let advice = (!bins.is_empty() && !here.lfs)
        .then_some("Optional: this repository doesn't use LFS (or git-lfs isn't installed).");
    check_row(
        ui,
        t,
        bins.is_empty().then_some(true),
        &text,
        advice,
        |ui| {
            if !bins.is_empty()
            && here.lfs
            && ui
                .add_enabled(idle, egui::Button::new("Add them"))
                .on_hover_text(
                    "Appends filter=lfs lines to .gitattributes. Files already committed stay as \
                     they are until `git lfs migrate import` moves them.",
                )
                .clicked()
        {
            acts.push(Action::Unity(UnityAct::AddAttributes { lfs: true }));
        }
        },
    );
    let text = if setup.unignored.is_empty() {
        "Unity's generated folders (Library, Temp, Logs…) are ignored.".to_owned()
    } else {
        format!("Not ignored: {}.", setup.unignored.join(", "))
    };
    check_row(ui, t, Some(setup.unignored.is_empty()), &text, None, |ui| {
        if !setup.unignored.is_empty()
            && ui
                .add_enabled(idle, egui::Button::new("Ignore them"))
                .on_hover_text("Appends them to .gitignore. Commit it to share it.")
                .clicked()
        {
            acts.push(Action::Unity(UnityAct::AddIgnores));
        }
    });
    if !setup.tracked_generated.is_empty() {
        let text = format!(
            "Committed although Unity makes them: {}.",
            setup.tracked_generated.join(", ")
        );
        check_row(ui, t, Some(false), &text, None, |ui| {
            if ui
                .add_enabled(idle, egui::Button::new("Stop tracking"))
                .on_hover_text("git rm -r --cached: out of git, kept on disk. Commit to share it.")
                .clicked()
            {
                acts.push(Action::Unity(UnityAct::UntrackGenerated));
            }
        });
    }
}

fn smart_merge(
    ui: &mut Ui,
    t: &Tokens,
    state: &UnityState,
    here: &Context,
    acts: &mut Vec<Action>,
) {
    ui.label(RichText::new("Smart Merge").font(theme::semibold(13.0)));
    ui.label(
        RichText::new(
            "UnityYAMLMerge merges scenes and prefabs by their structure. Once registered, git \
             runs it itself for the files .gitattributes marks merge=unityyamlmerge. Either way, \
             the conflict editor offers Solve with UnityYAMLMerge.",
        )
        .color(t.text_dim)
        .small(),
    );
    let idle = state.busy.is_none();
    let found = match &state.tool {
        Some(tool) => format!("UnityYAMLMerge: {}", tool.display()),
        None => "UnityYAMLMerge wasn't found in Unity Hub's editors: choose it if Unity is installed elsewhere.".to_owned(),
    };
    check_row(ui, t, Some(state.tool.is_some()), &found, None, |ui| {
        if ui
            .add_enabled(idle, egui::Button::new("Choose…"))
            .on_hover_text("Pick UnityYAMLMerge from an editor's Tools folder")
            .clicked()
        {
            acts.push(Action::Unity(UnityAct::ChooseTool));
        }
    });
    let registered = state.driver.is_some();
    let text = match (&state.driver, here.remote) {
        (Some(_), _) => "Registered as the merge driver for Unity's YAML files.",
        (None, true) => {
            "This repository is on another machine: register UnityYAMLMerge there. Solve with UnityYAMLMerge still works from here."
        }
        (None, false) => "Not registered as this repository's merge driver.",
    };
    check_row(ui, t, Some(registered), text, None, |ui| {
        let can = idle && state.tool.is_some() && !here.remote;
        let label = if registered {
            "Register again"
        } else {
            "Register"
        };
        let why = if state.tool.is_none() {
            "Choose UnityYAMLMerge first"
        } else {
            "Register it on the repository's own machine"
        };
        let r = ui
            .add_enabled(can, egui::Button::new(label))
            .on_hover_text(format!(
                "Sets merge.unityyamlmerge and mergetool.unityyamlmerge in this repository's config{}",
                state.driver.as_deref().map(|d| format!(" (now: {d})")).unwrap_or_default()
            ))
            .on_disabled_hover_text(why);
        if r.clicked() {
            acts.push(Action::Unity(UnityAct::RegisterDriver));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{some_of, vcs_place};

    #[test]
    fn lists_cut_short_and_where_settings_are() {
        let l = |n: usize| (0..n).map(|i| format!("*.{i}")).collect::<Vec<_>>();
        assert_eq!(some_of(&l(2)), "*.0, *.1");
        assert_eq!(some_of(&l(6)), "*.0, *.1, *.2, *.3…");
        assert!(vcs_place(Some("2019.4.1f1")).contains("Editor → Version Control"));
        assert!(vcs_place(Some("2022.3.1f1")).contains("Settings → Version Control"));
        assert!(vcs_place(None).contains("Settings → Version Control"));
    }
}
