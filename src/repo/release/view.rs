//! The release dialog: the version, the notes, what to do, what each step will do, and then how
//! each went.

use egui::{Context, RichText, Ui};

use super::{Form, ReleaseAct, ReleaseView, Run, Status, plan_of};
use crate::forge::Forge;
use crate::release::{self, Draft};
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// The dialog, while a release is prepared or cut.
    pub(in crate::repo) fn release_dialog(&mut self, ctx: &Context, acts: &mut Vec<Action>) {
        let forge = self.forge();
        let Some(view) = &mut self.release else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let modal = egui::Modal::new(egui::Id::new("gitcraft-release"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110));
        // What scrolls leaves room for the buttons below it, on a short window too.
        let room = (ctx.content_rect().height() - 220.0).max(160.0);
        let response = modal.show(ctx, |ui| {
            ui.set_width(560.0);
            contents(ui, view, forge, room, acts);
        });
        if response.should_close() {
            acts.push(Action::Release(ReleaseAct::Close));
        }
    }
}

fn contents(
    ui: &mut Ui,
    view: &mut ReleaseView,
    forge: Option<Forge>,
    room: f32,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-release-contents-0aca2ca2",
        ))
        .font(theme::semibold(15.0)),
    );
    let draft = match &view.draft {
        None => {
            ui.add_space(10.0);
            ui.add(crate::spinner::Spinner::new());
            close_button(ui, acts, true);
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(
                t.danger,
                crate::i18n::message(
                    "repo-workflows-release-contents-5a8565a0",
                    &[("e", release::display_error(e).into())],
                ),
            );
            close_button(ui, acts, true);
            return;
        }
        Some(Ok(draft)) => draft.clone(),
    };
    let since = match &draft.last_tag {
        Some(tag) => crate::i18n::message(
            "repo-workflows-release-changes-since",
            &[
                ("count", (draft.changes.len()).into()),
                ("tag", (tag.as_str()).into()),
            ],
        ),
        None => crate::i18n::text("repo-workflows-release-contents-917ec9b5").to_owned(),
    };
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-release-contents-1effa1c5",
            &[
                ("short-draft-commit-1", (short(&draft.commit)).into()),
                ("since", (since).clone().into()),
            ],
        ))
        .color(t.text_dim),
    );
    ui.add_space(8.0);
    let started = view.run.is_some();
    let plan = plan_of(&draft, &view.form);
    let problem = release::tag_problem(&plan.tag, &draft.tags).or_else(|| {
        (plan.bump.as_ref().is_some_and(|b| !b.manifests.is_empty())
            && release::version::Version::parse(&plan.tag).is_none())
        .then(|| crate::i18n::text("repo-workflows-release-contents-08a090b7").to_owned())
    });
    egui::ScrollArea::vertical()
        .max_height(room)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add_enabled_ui(!started, |ui| form(ui, &draft, &mut view.form, forge, acts));
            ui.add_space(8.0);
            match &view.run {
                None => preview(ui, &plan, forge),
                Some(run) => steps(ui, run),
            }
        });
    ui.add_space(8.0);
    match &view.run {
        None => {
            if let Some(p) = &problem {
                ui.colored_label(
                    t.warning,
                    release::display_problem(p, &plan.tag, &draft.tags),
                );
            }
            ui.horizontal(|ui| {
                if widgets::primary_button(
                    ui,
                    crate::i18n::text("repo-workflows-release-contents-0aca2ca2"),
                    problem.is_none(),
                    110.0,
                )
                .clicked()
                {
                    acts.push(Action::Release(ReleaseAct::Run));
                }
                close_button(ui, acts, false);
            });
        }
        Some(run) => {
            let running = run.running();
            let failed = run
                .steps
                .iter()
                .any(|(_, s)| matches!(s, Status::Failed(_)));
            ui.horizontal(|ui| {
                if failed
                    && widgets::primary_button(
                        ui,
                        crate::i18n::text("repo-workflows-release-contents-d640c742"),
                        !running,
                        90.0,
                    )
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-release-contents-1b129f4d",
                    ))
                    .clicked()
                {
                    acts.push(Action::Release(ReleaseAct::Run));
                }
                if failed
                    && ui
                        .add_enabled(
                            !running,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-release-contents-9af278eb",
                            )),
                        )
                        .on_hover_text(crate::i18n::text(
                            "repo-workflows-release-contents-adc3fa2f",
                        ))
                        .clicked()
                {
                    acts.push(Action::Release(ReleaseAct::Skip));
                }
                if running {
                    ui.add(crate::spinner::Spinner::new());
                }
                close_button(ui, acts, !running);
            });
        }
    }
}

/// What the release will do, before it runs.
fn preview(ui: &mut Ui, plan: &release::Plan, forge: Option<Forge>) {
    let t = Tokens::get(ui.ctx());
    heading(
        ui,
        crate::i18n::text("repo-workflows-release-preview-3d5a5069"),
    );
    for (i, (_, text)) in plan.display_preview(forge).iter().enumerate() {
        ui.add(egui::Label::new(format!("{}. {text}", i + 1)).wrap());
    }
    ui.label(
        RichText::new(crate::i18n::text("repo-workflows-release-preview-f9709725"))
            .color(t.text_faint)
            .small(),
    );
}

/// The steps as they went.
fn steps(ui: &mut Ui, run: &Run) {
    let t = Tokens::get(ui.ctx());
    heading(
        ui,
        crate::i18n::text("repo-workflows-release-steps-979cfabc"),
    );
    for (step, status) in &run.steps {
        let (mark, color) = match status {
            Status::Waiting => ("·", t.text_faint),
            Status::Running => ("…", t.text_dim),
            Status::Done => ("✔", t.success),
            Status::Skipped => ("–", t.text_faint),
            Status::Failed(_) => ("✖", t.danger),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(mark).color(color));
            ui.label(step.display_label());
            if *status == Status::Skipped {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-release-steps-389595a4"))
                        .color(t.text_faint)
                        .small(),
                );
            }
        });
        if let Status::Failed(e) = status {
            egui::ScrollArea::vertical()
                .id_salt("release-error")
                .max_height(120.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(release::display_error(e))
                                .color(t.danger)
                                .small(),
                        )
                        .wrap(),
                    );
                });
        }
    }
    if let Some(url) = &run.done.url {
        ui.hyperlink_to(
            crate::i18n::message(
                "repo-workflows-release-steps-47f6d38f",
                &[("url", (url).clone().into())],
            ),
            url,
        );
    }
}

/// The choices.
fn form(ui: &mut Ui, draft: &Draft, form: &mut Form, forge: Option<Forge>, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let gitlab = forge == Some(Forge::GitLab);
    if gitlab {
        form.prerelease = false;
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-release-form-dd167905"))
                .color(t.text_dim),
        );
        ui.add(egui::TextEdit::singleline(&mut form.tag).desired_width(140.0))
            .on_hover_text(crate::i18n::text("repo-workflows-release-form-78f0c939"));
        ui.add_enabled(
            !gitlab && form.publish,
            egui::Checkbox::new(
                &mut form.prerelease,
                crate::i18n::text("repo-workflows-release-form-a90d2f73"),
            ),
        )
        .on_hover_text(crate::i18n::text("repo-workflows-release-form-83f72bcc"))
        .on_disabled_hover_text(if gitlab {
            crate::i18n::text("repo-workflows-release-form-1dff93e4")
        } else {
            crate::i18n::text("repo-workflows-release-form-1252d7c7")
        });
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(crate::i18n::text("repo-workflows-release-form-05c6a12b"))
            .color(t.text_dim)
            .small(),
    );
    egui::ScrollArea::vertical()
        .id_salt("release-notes")
        .max_height(180.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut form.notes)
                    .desired_width(f32::INFINITY)
                    .desired_rows(7)
                    .font(theme::mono(11.5)),
            );
        });
    ui.add_space(4.0);
    let files: Vec<&str> = draft.manifests.iter().map(|(k, _)| k.files().0).collect();
    if !files.is_empty() {
        ui.checkbox(
            &mut form.bump,
            crate::i18n::message(
                "repo-workflows-release-form-bde96a2d",
                &[("files-join-1", (files.join(", ")).clone().into())],
            ),
        )
        .on_hover_text(crate::i18n::text("repo-workflows-release-form-4951860a"));
    }
    if draft.changelog {
        ui.add_enabled(
            draft.manifest_note.is_none() && draft.branch.is_some(),
            egui::Checkbox::new(
                &mut form.changelog,
                crate::i18n::text("repo-workflows-release-form-b42f9849"),
            ),
        );
    }
    if let Some(note) = &draft.manifest_note {
        ui.label(
            RichText::new(release::display_error(note))
                .color(t.text_faint)
                .small(),
        );
    }
    ui.checkbox(
        &mut form.sign,
        crate::i18n::text("repo-workflows-release-form-81677780"),
    )
    .on_hover_text(crate::i18n::text("repo-workflows-release-form-2cf49623"));
    let push = match &draft.remote {
        Some(remote) => crate::i18n::message(
            "repo-workflows-release-form-bac3cf56",
            &[("remote", (remote).clone().into())],
        ),
        None => crate::i18n::text("repo-workflows-release-form-c5ecd20d").to_owned(),
    };
    ui.add_enabled(
        draft.remote.is_some(),
        egui::Checkbox::new(&mut form.push, push),
    );
    let publish = match (forge, form.push) {
        (None, _) => crate::i18n::text("repo-workflows-release-form-4ffa3ce1").to_owned(),
        (Some(f), false) => crate::i18n::message(
            "repo-workflows-release-form-2f3dc697",
            &[("f-name-1", (f.name()).into())],
        ),
        (Some(f), true) => crate::i18n::message(
            "repo-workflows-release-form-e81f07e0",
            &[("f-name-1", (f.name()).into())],
        ),
    };
    ui.add_enabled(
        form.push && forge.is_some(),
        egui::Checkbox::new(&mut form.publish, publish),
    );
    if form.publish && form.push {
        ui.horizontal_wrapped(|ui| {
            for (i, file) in form.assets.iter().enumerate() {
                let mut name = file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.chars().count() > 40 {
                    name = name.chars().take(39).collect::<String>() + "…";
                }
                if ui
                    .button(format!("{name} ✖"))
                    .on_hover_text(crate::i18n::message(
                        "repo-workflows-release-form-c97b11c5",
                        &[("file-display-1", (file.display()).to_string().into())],
                    ))
                    .clicked()
                {
                    acts.push(Action::Release(ReleaseAct::RemoveAsset(i)));
                }
            }
            if ui
                .button(crate::i18n::text("repo-workflows-release-form-dc106d28"))
                .on_hover_text(crate::i18n::text("repo-workflows-release-form-9e86c805"))
                .clicked()
            {
                acts.push(Action::Release(ReleaseAct::AddAssets));
            }
        });
    }
}

fn heading(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(6.0);
    ui.label(
        RichText::new(text)
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
}

fn close_button(ui: &mut Ui, acts: &mut Vec<Action>, enabled: bool) {
    if ui
        .add_enabled(
            enabled,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-release-close-button-7d9eb7ac",
            ))
            .min_size(egui::vec2(80.0, 30.0)),
        )
        .clicked()
    {
        acts.push(Action::Release(ReleaseAct::Close));
    }
}
#[cfg(test)]
mod fluent_release_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn supplied() -> ReleaseView {
        ReleaseView {
            commit: "abcdef1234567890".into(),
            draft: Some(Ok(Draft {
                commit: "abcdef1234567890".into(),
                last_tag: None,
                changes: vec![],
                suggested: release::version::Version {
                    prefix: "v".into(),
                    major: 1,
                    minor: 2,
                    patch: 3,
                    pre: String::new(),
                },
                branch: Some("feature/ß".into()),
                manifests: vec![],
                manifest_note: Some(
                    "Only the tip of the branch checked out can move the version in its files"
                        .into(),
                ),
                changelog: false,
                sign: false,
                remote: Some("origin".into()),
                remote_ref: Some("refs/heads/feature/ß".into()),
                tags: vec!["v1.2.3".into()],
            })),
            form: Form {
                tag: "v1.2.3".into(),
                ..Form::default()
            },
            run: None,
        }
    }

    #[test]
    fn fluent_partition_release_actual_paint_keeps_validation_notes_errors_and_plan_raw()
    -> Result<(), String> {
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            let original = supplied();
            let expected_draft = original.draft.clone();
            let mut initialized = false;
            let mut h = Harness::new_ui_state(
                |ui, state: &mut ReleaseView| {
                    if !initialized {
                        crate::theme::install_fonts(ui.ctx());
                        initialized = true;
                        ui.ctx().request_discard("install supplied release fonts");
                        return;
                    }
                    contents(ui, state, Some(Forge::GitHub), 1000.0, &mut Vec::new());
                },
                original,
            );
            h.get_by_label(if language == "de" {
                "v1.2.3 ist bereits ein Tag"
            } else {
                "v1.2.3 is a tag already"
            });
            h.get_by_label(if language == "de" { "Nur an der Spitze des ausgecheckten Branches kann die Version in den Dateien geändert werden" } else { "Only the tip of the branch checked out can move the version in its files" });
            assert_eq!(h.state().draft, expected_draft);
            assert_eq!(h.state().form.tag, "v1.2.3");
            let draft = h
                .state()
                .draft
                .as_ref()
                .and_then(|x| x.as_ref().ok())
                .ok_or("missing supplied draft")?;
            assert_eq!(
                release::tag_problem(&h.state().form.tag, &draft.tags).as_deref(),
                Some("v1.2.3 is a tag already")
            );
            let plan = plan_of(draft, &h.state().form);
            let expected_preview = plan.preview(Some(Forge::GitHub));
            let original_failure = "Couldn't write Cargo.toml: Provider detail / ß";
            h.state_mut().run = Some(Run {
                plan,
                steps: vec![(release::Step::Tag, Status::Failed(original_failure.into()))],
                done: release::Done::default(),
                recorded: false,
                hosting: None,
                forge_repository_generation: 0,
            });
            h.run();
            h.get_by_label(if language == "de" {
                "Cargo.toml konnte nicht geschrieben werden: Provider detail / ß"
            } else {
                original_failure
            });
            let run = h.state().run.as_ref().ok_or("missing supplied run")?;
            assert_eq!(run.steps[0].1, Status::Failed(original_failure.into()));
            assert_eq!(run.steps[0].0.label(), "Tag");
            assert_eq!(run.plan.preview(Some(Forge::GitHub)), expected_preview);
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}
