//! The release dialog: the version, the notes, what to do, what each step will do, and then how
//! each went.

use egui::{Context, RichText, Ui};

use super::{Form, ReleaseAct, ReleaseView, Status, plan_of};
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
            .inner_margin(egui::Margin::same(18));
        let modal = egui::Modal::new(egui::Id::new("gitcraft-release"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110));
        let response = modal.show(ctx, |ui| {
            ui.set_width(560.0);
            contents(ui, view, forge, acts);
        });
        if response.should_close() {
            acts.push(Action::Release(ReleaseAct::Close));
        }
    }
}

fn contents(
    ui: &mut Ui,
    view: &mut ReleaseView,
    forge: Option<crate::forge::Forge>,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("Cut release").font(theme::semibold(15.0)));
    let draft = match &view.draft {
        None => {
            ui.add_space(10.0);
            ui.add(crate::spinner::Spinner::new());
            close_button(ui, acts, true);
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, format!("Couldn't read the history: {e}"));
            close_button(ui, acts, true);
            return;
        }
        Some(Ok(draft)) => draft.clone(),
    };
    let since = match &draft.last_tag {
        Some(tag) => format!(
            "{} change{} since {tag}",
            draft.changes.len(),
            if draft.changes.len() == 1 { "" } else { "s" }
        ),
        None => "No release before it: every commit up to it".to_owned(),
    };
    ui.label(RichText::new(format!("Of {} · {since}", short(&draft.commit))).color(t.text_dim));
    ui.add_space(8.0);
    let started = view.run.is_some();
    ui.add_enabled_ui(!started, |ui| form(ui, &draft, &mut view.form, acts));
    ui.add_space(8.0);
    match &view.run {
        None => {
            let plan = plan_of(&draft, &view.form);
            let problem = release::tag_problem(&plan.tag, &draft.tags);
            heading(ui, "WHAT IT WILL DO");
            for (i, (_, text)) in plan.preview(forge).iter().enumerate() {
                ui.add(egui::Label::new(format!("{}. {text}", i + 1)).wrap());
            }
            ui.label(
                RichText::new(
                    "Ctrl+Z takes the version commit and the tag back off here; what was pushed \
                     and the release on the forge stay (delete them there).",
                )
                .color(t.text_faint)
                .small(),
            );
            if let Some(p) = &problem {
                ui.colored_label(t.warning, p);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, "Cut release", problem.is_none(), 110.0).clicked() {
                    acts.push(Action::Release(ReleaseAct::Run));
                }
                close_button(ui, acts, false);
            });
        }
        Some(run) => {
            heading(ui, "STEPS");
            for (step, status) in &run.steps {
                let (mark, color) = match status {
                    Status::Waiting => ("·", t.text_faint),
                    Status::Running => ("…", t.text_dim),
                    Status::Done => ("✔", t.success),
                    Status::Failed(_) => ("✖", t.danger),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(mark).color(color));
                    ui.label(step.label());
                });
                if let Status::Failed(e) = status {
                    ui.add(egui::Label::new(RichText::new(e).color(t.danger).small()).wrap());
                }
            }
            if let Some(url) = &run.done.url {
                ui.hyperlink_to(format!("The release: {url}"), url);
            }
            ui.add_space(10.0);
            let running = run.running();
            let failed = run
                .steps
                .iter()
                .any(|(_, s)| matches!(s, Status::Failed(_)));
            ui.horizontal(|ui| {
                if failed
                    && widgets::primary_button(ui, "Resume", !running, 90.0)
                        .on_hover_text("Goes on from the step that failed")
                        .clicked()
                {
                    acts.push(Action::Release(ReleaseAct::Run));
                }
                if running {
                    ui.add(crate::spinner::Spinner::new());
                }
                close_button(ui, acts, !running);
            });
        }
    }
}

/// The choices.
fn form(ui: &mut Ui, draft: &Draft, form: &mut Form, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new("Version").color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut form.tag).desired_width(140.0))
            .on_hover_text("The tag; suggested from the Conventional Commits since the last one");
        ui.checkbox(&mut form.prerelease, "Pre-release");
    });
    ui.add_space(4.0);
    ui.label(RichText::new("Notes (Markdown)").color(t.text_dim).small());
    egui::ScrollArea::vertical()
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
            format!("Move the version in {}", files.join(", ")),
        )
        .on_hover_text("And in their lock files: one commit, “chore(release): …”");
    }
    if draft.changelog {
        ui.add_enabled(
            draft.manifest_note.is_none() && draft.branch.is_some(),
            egui::Checkbox::new(&mut form.changelog, "Add the notes to CHANGELOG.md"),
        );
    }
    if let Some(note) = &draft.manifest_note {
        ui.label(RichText::new(note).color(t.text_faint).small());
    }
    ui.checkbox(&mut form.sign, "Sign the tag")
        .on_hover_text("git tag -s, with your signing key; otherwise annotated (git tag -a)");
    let push = match &draft.remote {
        Some(remote) => format!("Push to {remote}"),
        None => "Push (this repository has no remote)".to_owned(),
    };
    ui.add_enabled(
        draft.remote.is_some(),
        egui::Checkbox::new(&mut form.push, push),
    );
    ui.add_enabled(
        form.push,
        egui::Checkbox::new(&mut form.publish, "Make the release on the forge"),
    );
    if form.publish && form.push {
        ui.horizontal_wrapped(|ui| {
            for (i, file) in form.assets.iter().enumerate() {
                let name = file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ui
                    .button(format!("{name} ✖"))
                    .on_hover_text(format!("{}\nClick to leave it out", file.display()))
                    .clicked()
                {
                    acts.push(Action::Release(ReleaseAct::RemoveAsset(i)));
                }
            }
            if ui.button("Attach files…").clicked() {
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
            egui::Button::new("Close").min_size(egui::vec2(80.0, 30.0)),
        )
        .clicked()
    {
        acts.push(Action::Release(ReleaseAct::Close));
    }
}
