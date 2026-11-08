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
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-unity-unity-panel-a07caecd",
                        ))
                        .color(t.text_dim),
                    );
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
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-unity-header-77866eb1"))
                .font(theme::semibold(16.0)),
        );
        if let Some(p) = &state.project {
            let at = if p.root.is_empty() {
                crate::i18n::text("repo-workflows-unity-header-464ed4ae").to_owned()
            } else {
                crate::i18n::message(
                    "repo-workflows-unity-header-2fd224a8",
                    &[("p-root-1", (p.root).clone().into())],
                )
            };
            let version = p
                .version
                .as_deref()
                .map(|v| format!(" · Unity {v}"))
                .unwrap_or_default();
            ui.label(RichText::new(format!("{at}{version}")).color(t.text_dim));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(
                ui,
                "x",
                crate::i18n::text("repo-workflows-unity-header-961b2a04"),
            )
            .clicked()
            {
                close = true;
            }
            if icons::small_button(
                ui,
                "refresh-cw",
                crate::i18n::text("repo-workflows-unity-header-fb7099ad"),
            )
            .clicked()
            {
                acts.push(Action::Unity(UnityAct::Check));
            }
        });
    });
    if let Some(busy) = &state.busy {
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(
                RichText::new(format!(
                    "{}…",
                    crate::repo::messages::display_operation(busy)
                ))
                .color(t.text_dim),
            );
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
        crate::i18n::text("repo-workflows-unity-vcs-place-bc141862")
    } else {
        crate::i18n::text("repo-workflows-unity-vcs-place-49a234f8")
    }
}

fn checks(ui: &mut Ui, t: &Tokens, state: &UnityState, here: &Context, acts: &mut Vec<Action>) {
    let setup: &UnitySetup = match &state.setup {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-unity-checks-78c80640"))
                        .color(t.text_dim),
                );
            });
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(
                t.danger,
                crate::i18n::message(
                    "repo-workflows-unity-checks-d41c81db",
                    &[("e", (e).clone().into())],
                ),
            );
            return;
        }
        Some(Ok(s)) => s,
    };
    let idle = state.busy.is_none();
    let version = state.project.as_ref().and_then(|p| p.version.as_deref());
    let (text, advice) = match setup.force_text {
        Some(true) => (
            crate::i18n::text("repo-workflows-unity-checks-1ba49cd4"),
            None,
        ),
        Some(false) => (
            crate::i18n::text("repo-workflows-unity-checks-d4b3a742"),
            Some(crate::i18n::text("repo-workflows-unity-checks-50526421")),
        ),
        None => (
            crate::i18n::text("repo-workflows-unity-checks-395e243e"),
            None,
        ),
    };
    check_row(ui, t, setup.force_text, text, advice, |_| {});
    let mode = setup.vcs_mode.as_deref();
    let text = match (setup.visible_meta(), mode) {
        (Some(true), Some(m)) if m.contains("Visible Meta Files") => {
            crate::i18n::text("repo-workflows-unity-checks-b10c6093").to_owned()
        }
        (Some(true), Some(m)) => {
            crate::i18n::message("repo-workflows-unity-checks-a5316b9c", &[("m", (m).into())])
        }
        (Some(false), _) => crate::i18n::text("repo-workflows-unity-checks-36e9ce17").to_owned(),
        _ => crate::i18n::text("repo-workflows-unity-checks-905615a3").to_owned(),
    };
    let fine = mode.map(|m| m.contains("Visible Meta Files"));
    let advice = (fine == Some(false)).then(|| vcs_place(version));
    check_row(ui, t, fine, &text, advice, |_| {});
    let yaml = &setup.missing_yaml;
    let text = if yaml.is_empty() {
        crate::i18n::text("repo-workflows-unity-checks-45316f68").to_owned()
    } else {
        crate::i18n::message(
            "repo-workflows-unity-checks-1c43ea2f",
            &[
                ("yaml-len-1", (yaml.len()).into()),
                ("some-of-yaml-2", (some_of(yaml)).clone().into()),
            ],
        )
    };
    check_row(ui, t, Some(yaml.is_empty()), &text, None, |ui| {
        if !yaml.is_empty()
            && ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text("repo-workflows-unity-checks-aa43c4de")),
                )
                .on_hover_text(crate::i18n::text("repo-workflows-unity-checks-27b09b87"))
                .clicked()
        {
            acts.push(Action::Unity(UnityAct::AddAttributes { lfs: false }));
        }
    });
    let bins = &setup.missing_lfs;
    let text = if bins.is_empty() {
        crate::i18n::text("repo-workflows-unity-checks-bfe76618").to_owned()
    } else {
        crate::i18n::message(
            "repo-workflows-unity-checks-d4bedd88",
            &[
                ("bins-len-1", (bins.len()).into()),
                ("some-of-bins-2", (some_of(bins)).clone().into()),
            ],
        )
    };
    let advice = (!bins.is_empty() && !here.lfs)
        .then_some(crate::i18n::text("repo-workflows-unity-checks-be32fbfa"));
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
                    .add_enabled(
                        idle,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-unity-checks-aa43c4de",
                        )),
                    )
                    .on_hover_text(crate::i18n::text("repo-workflows-unity-checks-9d25cdb4"))
                    .clicked()
            {
                acts.push(Action::Unity(UnityAct::AddAttributes { lfs: true }));
            }
        },
    );
    let text = if setup.unignored.is_empty() {
        crate::i18n::text("repo-workflows-unity-checks-83893d2c").to_owned()
    } else {
        crate::i18n::message(
            "repo-workflows-unity-checks-3cdb2b44",
            &[(
                "setup-unignored-join-1",
                (setup.unignored.join(", ")).clone().into(),
            )],
        )
    };
    check_row(ui, t, Some(setup.unignored.is_empty()), &text, None, |ui| {
        if !setup.unignored.is_empty()
            && ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text("repo-workflows-unity-checks-f64c2525")),
                )
                .on_hover_text(crate::i18n::text("repo-workflows-unity-checks-def8323f"))
                .clicked()
        {
            acts.push(Action::Unity(UnityAct::AddIgnores));
        }
    });
    if !setup.tracked_generated.is_empty() {
        let text = crate::i18n::message(
            "repo-workflows-unity-checks-aa9a77c3",
            &[(
                "setup-tracked-generated-join-1",
                (setup.tracked_generated.join(", ")).clone().into(),
            )],
        );
        check_row(ui, t, Some(false), &text, None, |ui| {
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text("repo-workflows-unity-checks-b18a0b47")),
                )
                .on_hover_text(crate::i18n::text("repo-workflows-unity-checks-07cd7cdf"))
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
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-unity-smart-merge-c7b982e4",
        ))
        .font(theme::semibold(13.0)),
    );
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-unity-smart-merge-6b8c12f4",
        ))
        .color(t.text_dim)
        .small(),
    );
    let idle = state.busy.is_none();
    let found = match &state.tool {
        Some(tool) => format!("UnityYAMLMerge: {}", tool.display()),
        None => crate::i18n::text("repo-workflows-unity-smart-merge-1ff6c570").to_owned(),
    };
    check_row(ui, t, Some(state.tool.is_some()), &found, None, |ui| {
        if ui
            .add_enabled(
                idle,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-unity-smart-merge-7ca41615",
                )),
            )
            .on_hover_text(crate::i18n::text(
                "repo-workflows-unity-smart-merge-cf3161f1",
            ))
            .clicked()
        {
            acts.push(Action::Unity(UnityAct::ChooseTool));
        }
    });
    let registered = state.driver.is_some();
    let text = match (&state.driver, here.remote) {
        (Some(_), _) => crate::i18n::text("repo-workflows-unity-smart-merge-8fec67ee"),
        (None, true) => crate::i18n::text("repo-workflows-unity-smart-merge-52f6735a"),
        (None, false) => crate::i18n::text("repo-workflows-unity-smart-merge-a4fecc79"),
    };
    check_row(ui, t, Some(registered), text, None, |ui| {
        let can = idle && state.tool.is_some() && !here.remote;
        let label = if registered {
            crate::i18n::text("repo-workflows-unity-smart-merge-dfedc705")
        } else {
            crate::i18n::text("repo-workflows-unity-smart-merge-bb7234ec")
        };
        let why = if state.tool.is_none() {
            crate::i18n::text("repo-workflows-unity-smart-merge-4822ac9a")
        } else {
            crate::i18n::text("repo-workflows-unity-smart-merge-58740afb")
        };
        let r = ui
            .add_enabled(can, egui::Button::new(label))
            .on_hover_text(crate::i18n::message(
                "repo-workflows-unity-smart-merge-f6ef6d29",
                &[(
                    "state-driver-as-deref-map-d-form-1",
                    (state
                        .driver
                        .as_deref()
                        .map(|d| {
                            crate::i18n::message(
                                "repo-workflows-unity-smart-merge-a472cbc6",
                                &[("d", (d).into())],
                            )
                        })
                        .unwrap_or_default())
                    .clone()
                    .into(),
                )],
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
