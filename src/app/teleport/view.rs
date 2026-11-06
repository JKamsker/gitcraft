//! Drawing the teleport dialog: the machines, where the work goes there, then the teleport's
//! progress (and the question a large bundle asks).

use egui::{Key, RichText, Stroke, Ui, vec2};

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
    // ssh asking something (in its own dialog, over this one) has the keys.
    let asked = remote::askpass::pending().is_some();
    let mut close = false;
    let resp = egui::Modal::new(egui::Id::new("teleport-dialog"))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(110))
        .show(ctx, |ui| {
            ui.set_width(540.0);
            ui.label(RichText::new("Teleport").font(theme::semibold(15.0)));
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "The branch, its staged and unstaged changes and the untracked files arrive \
                     on the machine picked, in a new worktree (or a new repository). Nothing \
                     here is changed.",
                )
                .color(t.text_dim)
                .small(),
            );
            ui.add_space(10.0);
            if form.running.is_some() {
                progress(ui, form, &t);
            } else {
                close = fields(ui, form, &t, asked);
            }
        });
    // While it runs, only its Cancel stops it: not a click beside the dialog, nor Esc.
    if resp.should_close() && form.running.is_none() && !asked {
        close = true;
    }
    close
}

/// The form; whether it is to close.
fn fields(ui: &mut Ui, form: &mut TeleportForm, t: &Tokens, asked: bool) -> bool {
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
    if form.machine.is_some() && form.looking() {
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(RichText::new("Looking there…").color(t.text_faint));
        });
    }
    if let Some(err) = &form.error {
        ui.add_space(6.0);
        ui.colored_label(t.danger, err);
    }
    ui.add_space(12.0);
    let mut close = false;
    ui.horizontal(|ui| {
        let can = form.can_start();
        let enter = !asked && ui.input(|i| i.key_pressed(Key::Enter));
        if crate::repo::primary_button(ui, "Teleport", can, 100.0).clicked() || (enter && can) {
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
    if machines.is_empty() && form.distros.is_none() {
        ui.label(
            RichText::new(
                "No other machine found: type an SSH host below, or add one to ~/.ssh/config.",
            )
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
    ui.horizontal(|ui| {
        let typed = ui.add(
            egui::TextEdit::singleline(&mut form.typed_host)
                .hint_text("Another SSH host: user@host")
                .desired_width(ui.available_width() - 90.0)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        let entered = typed.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if ui
            .add(egui::Button::new("Use").min_size(vec2(80.0, 28.0)))
            .clicked()
            || entered
        {
            let host = form.typed_host.trim().to_owned();
            if remote::valid_ssh_host(&host) {
                form.ssh_hosts.push(host.clone());
                form.typed_host.clear();
                picked = Some(Machine::There(remote::Host::Ssh(host)));
            } else {
                form.error = Some(
                    "The host is what ssh takes: an alias of ~/.ssh/config or user@host."
                        .to_owned(),
                );
            }
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
    let (repo_hint, new_hint) = hints(form.machine.as_ref());
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
        path_field(ui, &mut form.other_repo, repo_hint);
    }
    if !form.clone_url.is_empty() {
        ui.radio_value(
            &mut form.place,
            Place::Clone,
            format!(
                "A clone of {}, made there first",
                crate::teleport::shown_url(&form.clone_url)
            ),
        );
    }
    ui.radio_value(
        &mut form.place,
        Place::Create,
        "A new repository, made from what is sent",
    );
    if matches!(form.place, Place::Clone | Place::Create) {
        path_field(ui, &mut form.new_repo, new_hint);
    }
    ui.add_space(6.0);
    label(ui, "Branch there (empty: detached)", t);
    let branch_changed = path_field(ui, &mut form.branch, "");
    let changed = before != (form.place, form.other_repo.clone(), form.new_repo.clone());
    if changed && before.0 != form.place {
        // Picked now: the clones found later don't change it, and the worktree's folder
        // follows the repository picked.
        form.place_picked = true;
        form.folder_edited = false;
    }
    if changed || branch_changed {
        form.error = None;
        form.derive_folder();
    }
    if form.place != Place::Create {
        ui.add_space(6.0);
        label(ui, "New worktree", t);
        if path_field(ui, &mut form.folder, "") {
            form.error = None;
            // Emptied, it is derived again.
            form.folder_edited = !form.folder.trim().is_empty();
            form.derive_folder();
        }
    }
}

/// What the path fields hint, for the machine picked: (a repository there, a new folder).
fn hints(machine: Option<&Machine>) -> (&'static str, &'static str) {
    match machine {
        Some(Machine::There(remote::Host::Ssh(_))) => ("/home/me/repo", "~/repo"),
        Some(Machine::There(remote::Host::Wsl(_))) => ("/home/me/repo", "/home/me/repo"),
        _ if cfg!(windows) => (r"C:\code\repo", r"C:\code\repo"),
        _ => ("/home/me/repo", "/home/me/repo"),
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
    let cancelling = running.job.cancelled();
    if let (Step::Confirm(size), false) = (step, cancelling) {
        ui.label(format!(
            "The bundle is {}, more than {}. Send it to {}?",
            crate::git::human_size(size),
            crate::git::human_size(crate::teleport::LARGE),
            running.machine.label()
        ));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            if crate::repo::primary_button(ui, "Send", true, 90.0).clicked() || enter {
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
        ui.label(if cancelling && step.stoppable() {
            "Cancelling…".to_owned()
        } else {
            step.label()
        });
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
    let can_cancel = step.stoppable() && !cancelling;
    if ui
        .add_enabled(
            can_cancel,
            egui::Button::new("Cancel").min_size(vec2(80.0, 30.0)),
        )
        .on_disabled_hover_text(if cancelling {
            "Stopping at the next step; what it made is taken back."
        } else {
            "The work is being unpacked there: it can't be stopped now. If it fails, what it made is taken back."
        })
        .clicked()
    {
        running.job.cancel();
    }
}
