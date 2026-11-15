//! _Repository roots_: the profile's configured roots (add, change, remove, turn on or off,
//! depth, rescan or stop), the nearby folders inferred from repositories opened, and the
//! paths hidden from the library.

use std::path::PathBuf;

use egui::{RichText, Stroke, Ui, vec2};

use super::model::{Library, MAX_DEPTH, Source};
use super::scheduler::Scheduler;
use super::status;
use crate::app::App;
use crate::app::home_recent::now_secs;
use crate::icons;
use crate::theme::{self, Tokens};

/// What the dialog was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) enum RootsClick {
    Add,
    Close,
    Remove(u64),
    Enable(u64, bool),
    Depth(u64, u8),
    Move(u64),
    Rescan(Source),
    Stop(Source),
    RescanAll,
    Nearby(bool),
    ForgetNearby(PathBuf),
    UnhideAll,
}

/// How a depth reads in the dialog.
pub(in crate::app) fn depth_label(depth: u8) -> String {
    match depth {
        0 => crate::i18n::text("interaction-library-depth-0").to_owned(),
        n => crate::i18n::message("interaction-library-depth", &[("depth", n.into())]),
    }
}

/// The dialog's content for `lib`; returns what was clicked.
pub(in crate::app) fn roots_body(
    ui: &mut Ui,
    lib: &Library,
    sched: &Scheduler,
    profile: u64,
    t: &Tokens,
) -> Option<RootsClick> {
    use crate::i18n::text;
    let now = now_secs();
    let mut click = None;
    ui.label(RichText::new(text("interaction-library-roots-about")).color(t.text_dim));
    ui.add_space(8.0);
    if lib.roots.is_empty() {
        ui.label(RichText::new(text("interaction-library-no-roots")).color(t.text_faint));
    }
    egui::ScrollArea::vertical()
        .id_salt("library-roots")
        .max_height(300.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for root in &lib.roots {
                let source = Source::Root(root.id);
                let activity = sched.activity(profile, &source);
                let state = status::state(root.enabled, activity, root.last.as_ref(), now);
                let line = status::line(root.enabled, activity, root.last.as_ref(), now);
                let found = lib.count(&source);
                ui.horizontal(|ui| {
                    let mut on = root.enabled;
                    if ui
                        .checkbox(&mut on, "")
                        .on_hover_text(text("interaction-library-enabled"))
                        .changed()
                    {
                        click = Some(RootsClick::Enable(root.id, on));
                    }
                    ui.vertical(|ui| {
                        ui.set_width(250.0);
                        ui.label(
                            RichText::new(crate::remote::display(&root.path))
                                .font(theme::medium(12.5)),
                        );
                        let color = if state.is_problem() {
                            t.warning
                        } else {
                            t.text_faint
                        };
                        let listed = crate::i18n::message(
                            "interaction-library-listed",
                            &[("count", found.into())],
                        );
                        ui.label(
                            RichText::new(format!("{line} · {listed}"))
                                .color(color)
                                .small(),
                        );
                    });
                    let mut depth = root.depth;
                    egui::ComboBox::from_id_salt(("library-depth", root.id))
                        .selected_text(depth_label(depth))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for d in 0..=MAX_DEPTH {
                                ui.selectable_value(&mut depth, d, depth_label(d));
                            }
                        });
                    if depth != root.depth {
                        click = Some(RootsClick::Depth(root.id, depth));
                    }
                    click =
                        scan_button(ui, root.enabled, activity.is_some(), &source).or(click.take());
                    if icons::small_button(ui, "folder-open", text("interaction-library-move"))
                        .clicked()
                    {
                        click = Some(RootsClick::Move(root.id));
                    }
                    if icons::small_button(ui, "x", text("interaction-library-remove")).clicked() {
                        click = Some(RootsClick::Remove(root.id));
                    }
                });
                ui.add_space(4.0);
            }
            ui.separator();
            click = nearby_part(ui, lib, sched, profile, t).or(click.take());
        });
    if !lib.hidden.is_empty() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-library-hidden",
                    &[("count", lib.hidden.len().into())],
                ))
                .color(t.text_faint),
            );
            if ui.button(text("interaction-library-unhide")).clicked() {
                click = Some(RootsClick::UnhideAll);
            }
        });
    }
    if sched.waiting_on_slow() {
        ui.label(
            RichText::new(text("interaction-library-slow"))
                .color(t.warning)
                .small(),
        );
    }
    click
}

/// Rescan, or Stop while it scans.
fn scan_button(ui: &mut Ui, enabled: bool, busy: bool, source: &Source) -> Option<RootsClick> {
    use crate::i18n::text;
    if busy {
        ui.button(text("interaction-scan-stop"))
            .clicked()
            .then(|| RootsClick::Stop(source.clone()))
    } else {
        ui.add_enabled(
            enabled,
            egui::Button::new(text("interaction-library-rescan")),
        )
        .clicked()
        .then(|| RootsClick::Rescan(source.clone()))
    }
}

/// Looking next to opened repositories: the switch, and the folders looked in lately.
fn nearby_part(
    ui: &mut Ui,
    lib: &Library,
    sched: &Scheduler,
    profile: u64,
    t: &Tokens,
) -> Option<RootsClick> {
    use crate::i18n::text;
    let now = now_secs();
    let mut click = None;
    let mut on = lib.nearby_enabled;
    if ui
        .checkbox(&mut on, text("interaction-library-nearby-switch"))
        .on_hover_text(text("interaction-library-nearby-tip"))
        .changed()
    {
        click = Some(RootsClick::Nearby(on));
    }
    for nearby in lib.nearby.iter().filter(|_| lib.nearby_enabled) {
        let source = Source::Nearby(nearby.parent.clone());
        let activity = sched.activity(profile, &source);
        let line = status::line(true, activity, nearby.last.as_ref(), now);
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.set_width(374.0);
                ui.label(crate::remote::display(&nearby.parent));
                ui.label(RichText::new(line).color(t.text_faint).small());
            });
            click = scan_button(ui, true, activity.is_some(), &source).or(click.take());
            if icons::small_button(ui, "x", text("interaction-library-forget-nearby")).clicked() {
                click = Some(RootsClick::ForgetNearby(nearby.parent.clone()));
            }
        });
    }
    click
}

impl App {
    /// The dialog, while it is open.
    pub(in crate::app) fn library_dialog(&mut self, ctx: &egui::Context) {
        if !self.library_open {
            return;
        }
        let t = Tokens::get(ctx);
        let profile = self.settings.active_profile;
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let empty = Library::default();
        let state = &self.library;
        let lib = state.get(profile).unwrap_or(&empty);
        let mut click = None;
        let resp = egui::Modal::new(egui::Id::new("library-roots-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(640.0);
                let title = crate::i18n::text("interaction-library-roots-title");
                ui.label(RichText::new(title).font(theme::semibold(15.0)));
                ui.add_space(6.0);
                click = roots_body(ui, lib, &state.scheduler, profile, &t);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let add = crate::i18n::text("interaction-library-add-root");
                    if crate::repo::primary_button(ui, add, true, 180.0).clicked() {
                        click = Some(RootsClick::Add);
                    }
                    let all = crate::i18n::text("interaction-library-rescan-all");
                    if ui
                        .add(egui::Button::new(all).min_size(vec2(80.0, 30.0)))
                        .clicked()
                    {
                        click = Some(RootsClick::RescanAll);
                    }
                    let close = crate::i18n::text("interaction-scan-close");
                    if ui
                        .add(egui::Button::new(close).min_size(vec2(80.0, 30.0)))
                        .clicked()
                    {
                        click = Some(RootsClick::Close);
                    }
                });
            });
        if resp.should_close() && click.is_none() {
            click = Some(RootsClick::Close);
        }
        if let Some(click) = click {
            self.roots_click(click);
        }
    }

    /// Does what the dialog asked.
    pub(in crate::app) fn roots_click(&mut self, click: RootsClick) {
        let profile = self.settings.active_profile;
        let lib = &mut self.library;
        match click {
            RootsClick::Add => self.pick_library_root(None),
            RootsClick::Close => self.library_open = false,
            RootsClick::Remove(id) => lib.remove_root(profile, id),
            RootsClick::Enable(id, on) => lib.set_enabled(profile, id, on),
            RootsClick::Depth(id, depth) => lib.set_depth(profile, id, depth),
            RootsClick::Move(id) => self.pick_library_root(Some(id)),
            RootsClick::Rescan(source) => lib.rescan(profile, &source),
            RootsClick::Stop(source) => lib.stop(&source),
            RootsClick::RescanAll => lib.rescan_all(profile),
            RootsClick::Nearby(on) => lib.set_nearby(profile, on),
            RootsClick::ForgetNearby(parent) => lib.forget_nearby(profile, &parent),
            RootsClick::UnhideAll => lib.unhide_all(profile),
        }
    }
}
