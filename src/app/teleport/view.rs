//! Drawing the teleport dialog: the machines, where the work goes there, then the teleport's
//! progress (and the question a large bundle asks).

use egui::{RichText, Stroke, Ui, vec2};

use super::{Loading, Place, TeleportForm};
use crate::remote;
use crate::teleport::{Machine, Step};
use crate::theme::{self, Tokens};

/// Draws the dialog; whether it is to close.
pub(super) fn show(form: &mut TeleportForm, ctx: &egui::Context) -> bool {
    let t = Tokens::get(ctx);
    let frame = egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::same(18))
        .shadow(ctx.global_style().visuals.window_shadow);
    let mut close = false;
    let resp = egui::Modal::new(egui::Id::new("teleport-dialog"))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(110))
        .show(ctx, |ui| {
            ui.set_width(540.0);
            ui.label(RichText::new("Teleport to another machine").font(theme::semibold(15.0)));
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "The branch, its staged and unstaged changes and the untracked files arrive \
                     in a new worktree there. Nothing here is changed.",
                )
                .color(t.text_dim)
                .small(),
            );
            ui.add_space(10.0);
            if form.running.is_some() {
                progress(ui, form, &t);
            } else {
                close = fields(ui, form, &t);
            }
        });
    if resp.should_close() {
        match &form.running {
            // Stopped first: the dialog closes once what it made is taken back.
            Some(running) => running.job.cancel(),
            None => close = true,
        }
    }
    close
}

/// The form; whether it is to close.
fn fields(ui: &mut Ui, form: &mut TeleportForm, t: &Tokens) -> bool {
    let ctx = ui.ctx().clone();
    source_line(ui, form, t);
    let ready = matches!(form.info, Loading::Ready(_));
    if ready {
        ui.add_space(8.0);
        label(ui, "To", t);
        machine_list(ui, form, &ctx, t);
        if form.machine.is_some() {
            ui.add_space(8.0);
            place_fields(ui, form, t);
        }
    }
    if let Some(err) = &form.error {
        ui.add_space(6.0);
        ui.colored_label(t.danger, err);
    }
    ui.add_space(12.0);
    let mut close = false;
    ui.horizontal(|ui| {
        let can = ready && form.machine.is_some();
        if crate::repo::primary_button(ui, "Teleport", can, 100.0).clicked() {
            form.start(&ctx);
        }
        if ui
            .add(egui::Button::new("Cancel").min_size(vec2(80.0, 30.0)))
            .clicked()
        {
            close = true;
        }
    });
    close
}

fn label(ui: &mut Ui, text: &str, t: &Tokens) {
    ui.label(RichText::new(text).color(t.text_dim).small());
}

/// Where the work leaves from, or why it can't.
fn source_line(ui: &mut Ui, form: &TeleportForm, t: &Tokens) {
    label(ui, "From", t);
    match &form.info {
        Loading::Waiting(_) => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(remote::display(&form.source));
            });
        }
        Loading::Failed(e) => {
            ui.label(remote::display(&form.source));
            ui.colored_label(t.danger, e);
        }
        Loading::Ready(info) => {
            let on = match &info.branch {
                Some(branch) => branch.clone(),
                None => format!("detached at {}", &info.head[..info.head.len().min(8)]),
            };
            ui.label(format!("{}  ·  {on}", remote::display(&form.source)));
        }
    }
}

/// The machines, one to pick.
fn machine_list(ui: &mut Ui, form: &mut TeleportForm, ctx: &egui::Context, t: &Tokens) {
    let machines = form.machines();
    if machines.is_empty() {
        ui.label(
            RichText::new("No other machine is known: hosts come from ~/.ssh/config.")
                .color(t.text_faint),
        );
    }
    let mut picked = None;
    egui::ScrollArea::vertical()
        .id_salt("teleport-machines")
        .max_height(140.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for machine in machines {
                let name = match &machine {
                    Machine::Here => "This machine".to_owned(),
                    Machine::There(host) => host.label(),
                };
                let on = form.machine.as_ref() == Some(&machine);
                if ui.selectable_label(on, name).clicked() {
                    picked = Some(machine);
                }
            }
            if form.distros.is_some() {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    ui.label(RichText::new("Listing WSL distributions…").color(t.text_faint));
                });
            }
        });
    if let Some(machine) = picked {
        form.pick(machine, ctx);
    }
}

/// Where on the machine: a clone found there, another repository, a clone made now or a new
/// repository; then the branch and the new worktree's folder.
fn place_fields(ui: &mut Ui, form: &mut TeleportForm, t: &Tokens) {
    label(ui, "Into", t);
    let before = (form.place, form.other_repo.clone(), form.new_repo.clone());
    match &form.clones {
        Loading::Waiting(_) => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(
                    RichText::new("Looking for clones of this project there…").color(t.text_faint),
                );
            });
        }
        Loading::Ready(clones) => {
            for (i, clone) in clones.iter().enumerate() {
                ui.radio_value(
                    &mut form.place,
                    Place::Found(i),
                    format!("A new worktree of {}", remote::git_path(clone)),
                );
            }
        }
        Loading::Failed(_) => {}
    }
    ui.radio_value(
        &mut form.place,
        Place::Other,
        "A new worktree of another repository there",
    );
    if form.place == Place::Other {
        path_field(ui, &mut form.other_repo, "/home/me/repo");
    }
    if !form.clone_url.is_empty() {
        ui.radio_value(
            &mut form.place,
            Place::Clone,
            format!("A clone of {}, made there first", form.clone_url),
        );
    }
    ui.radio_value(
        &mut form.place,
        Place::Create,
        "A new repository, made from what is sent",
    );
    if matches!(form.place, Place::Clone | Place::Create) {
        path_field(ui, &mut form.new_repo, "~/repo");
    }
    ui.add_space(6.0);
    label(ui, "Branch there (empty: detached)", t);
    let branch_changed = path_field(ui, &mut form.branch, "");
    if before != (form.place, form.other_repo.clone(), form.new_repo.clone()) || branch_changed {
        form.derive_folder();
    }
    if form.place != Place::Create {
        ui.add_space(6.0);
        label(ui, "New worktree", t);
        if path_field(ui, &mut form.folder, "") {
            form.folder_edited = true;
        }
    }
}

/// A one-line field; whether it changed.
fn path_field(ui: &mut Ui, text: &mut String, hint: &str) -> bool {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    )
    .changed()
}

/// Where the teleport is; a large bundle's question; the button that stops it.
fn progress(ui: &mut Ui, form: &TeleportForm, t: &Tokens) {
    let Some(running) = &form.running else {
        return;
    };
    let step = running.job.step();
    if let Step::Confirm(size) = step {
        ui.label(format!(
            "The bundle is {}, more than {}. Send it to {}?",
            crate::git::human_size(size),
            crate::git::human_size(crate::teleport::LARGE),
            running.machine.label()
        ));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if crate::repo::primary_button(ui, "Send", true, 90.0).clicked() {
                running.job.answer(true);
            }
            if ui
                .add(egui::Button::new("Cancel").min_size(vec2(80.0, 30.0)))
                .clicked()
            {
                running.job.cancel();
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        ui.add(crate::spinner::Spinner::new());
        ui.label(step.label());
    });
    if let Step::Sending { sent, total } = step {
        let done = if total == 0 {
            1.0
        } else {
            sent as f32 / total as f32
        };
        // As wide as the dialog: an endless one would leave the dialog no width to centre by.
        let width = ui.available_width();
        ui.add(egui::ProgressBar::new(done).desired_width(width));
    }
    ui.label(
        RichText::new(format!("To {}", running.machine.label()))
            .color(t.text_faint)
            .small(),
    );
    ui.add_space(10.0);
    if ui
        .add(egui::Button::new("Cancel").min_size(vec2(80.0, 30.0)))
        .clicked()
    {
        running.job.cancel();
    }
}
