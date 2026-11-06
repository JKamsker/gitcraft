//! The package panel.

use egui::{Context, RichText, Ui};

use super::{PackageAct, PackageView};
use crate::package::{Note, Published, Report};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How many rows of a list are shown; the rest are counted.
const SHOWN: usize = 400;

impl RepoTab {
    pub(in crate::repo) fn package_panel(&mut self, ctx: &Context, acts: &mut Vec<Action>) {
        let Some(view) = &self.package else { return };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let room = (ctx.content_rect().height() - 200.0).max(160.0);
        let response = egui::Modal::new(egui::Id::new("gitcraft-package"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(600.0);
                contents(ui, view, room, acts);
            });
        if response.should_close() {
            acts.push(Action::Package(PackageAct::Close));
        }
    }
}

fn contents(ui: &mut Ui, view: &PackageView, room: f32, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("What a package will publish").font(theme::semibold(15.0)));
    let list = match &view.packages {
        None => {
            ui.add(crate::spinner::Spinner::new());
            buttons(ui, acts, false);
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, format!("Couldn't find the packages: {e}"));
            buttons(ui, acts, true);
            return;
        }
        Some(Ok(list)) if list.is_empty() => {
            ui.label(
                RichText::new(
                    "Nothing here is published: package.json is private, or every crate has \
                     publish = false.",
                )
                .color(t.text_dim),
            );
            buttons(ui, acts, true);
            return;
        }
        Some(Ok(list)) => list,
    };
    let pkg = &list[view.selected.min(list.len() - 1)];
    ui.horizontal(|ui| {
        ui.label(RichText::new("Package").color(t.text_dim));
        egui::ComboBox::from_id_salt("package-choice")
            .selected_text(format!("{} {}", pkg.name, pkg.version))
            .show_ui(ui, |ui| {
                for (i, p) in list.iter().enumerate() {
                    let label = if p.dir.is_empty() {
                        format!("{} {}", p.name, p.version)
                    } else {
                        format!("{} {} ({})", p.name, p.version, p.dir)
                    };
                    if ui.selectable_label(i == view.selected, label).clicked() {
                        acts.push(Action::Package(PackageAct::Select(i)));
                    }
                }
            });
    });
    ui.label(
        RichText::new(format!(
            "From {}, run in {}",
            pkg.ecosystem.tool(),
            if pkg.dir.is_empty() {
                "the repository's root"
            } else {
                &pkg.dir
            }
        ))
        .color(t.text_faint)
        .small(),
    );
    ui.add_space(6.0);
    match &view.report {
        None => {
            ui.add(crate::spinner::Spinner::new());
            ui.label(
                RichText::new(format!("Asking {}…", pkg.ecosystem.tool()))
                    .color(t.text_faint)
                    .small(),
            );
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, format!("Couldn't list the package: {e}"));
        }
        Some(Ok(report)) => {
            egui::ScrollArea::vertical()
                .max_height(room)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    report_ui(
                        ui,
                        report,
                        view.published.as_ref(),
                        pkg.ecosystem.registry(),
                    );
                });
        }
    }
    buttons(ui, acts, view.report.is_some());
}

fn report_ui(
    ui: &mut Ui,
    report: &Report,
    published: Option<&Result<Option<Published>, String>>,
    registry: &str,
) {
    let t = Tokens::get(ui.ctx());
    if let Some(hint) = &report.hint {
        ui.add(egui::Label::new(RichText::new(hint).color(t.warning).small()).wrap());
    }
    notes(
        ui,
        "LOOKS SHIPPED BY ACCIDENT",
        &report.accidental,
        t.warning,
    );
    notes(ui, "USUALLY SHIPPED, LEFT OUT", &report.missing, t.warning);
    heading(ui, &format!("COMPARED WITH {}", registry.to_uppercase()));
    match published {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                faint(ui, &format!("Asking {registry}…"));
            });
        }
        Some(Ok(None)) => faint(ui, &format!("Never published on {registry}.")),
        Some(Err(e)) => faint(ui, &format!("Couldn't compare: {e}")),
        Some(Ok(Some(p))) => {
            if p.added.is_empty() && p.gone.is_empty() {
                faint(ui, &format!("Same files as {} on {registry}.", p.version));
            }
            for f in p.added.iter().take(SHOWN) {
                let tip = format!("New since {}", p.version);
                line(ui, &format!("+ {f}"), t.success, &tip);
            }
            more(ui, p.added.len());
            for f in p.gone.iter().take(SHOWN) {
                let tip = format!("In {}, not in this one", p.version);
                line(ui, &format!("− {f}"), t.danger, &tip);
            }
            more(ui, p.gone.len());
        }
    }
    heading(ui, &format!("FILES ({})", report.files.len()));
    for (f, size) in report.files.iter().take(SHOWN) {
        let text = match size {
            Some(s) => format!("{f}  ·  {}", crate::git::human_size(*s)),
            None => f.clone(),
        };
        line(ui, &text, t.text_dim, "");
    }
    more(ui, report.files.len());
}

fn notes(ui: &mut Ui, title: &str, notes: &[Note], color: egui::Color32) {
    heading(ui, &format!("{title} ({})", notes.len()));
    if notes.is_empty() {
        faint(ui, "None found.");
    }
    for n in notes.iter().take(SHOWN) {
        line(ui, &format!("{} — {}", n.path, n.why), color, "");
    }
    more(ui, notes.len());
}

/// The count of the rows of a list of `len` not shown.
fn more(ui: &mut Ui, len: usize) {
    if len > SHOWN {
        faint(ui, &format!("and {} more", len - SHOWN));
    }
}

fn line(ui: &mut Ui, text: &str, color: egui::Color32, tip: &str) {
    let r =
        ui.add(egui::Label::new(RichText::new(text).font(theme::mono(11.5)).color(color)).wrap());
    if !tip.is_empty() {
        r.on_hover_text(tip);
    }
}

fn heading(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(
        RichText::new(text)
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
}

fn faint(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(text).color(t.text_faint));
}

/// Refresh (once there is something to read again) and Close.
fn buttons(ui: &mut Ui, acts: &mut Vec<Action>, refresh: bool) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                refresh,
                egui::Button::new("Refresh").min_size(egui::vec2(80.0, 30.0)),
            )
            .on_hover_text("Lists the packages and their files again, after a change to them")
            .clicked()
        {
            acts.push(Action::Package(PackageAct::Refresh));
        }
        if ui
            .add(egui::Button::new("Close").min_size(egui::vec2(80.0, 30.0)))
            .on_hover_text("Esc")
            .clicked()
        {
            acts.push(Action::Package(PackageAct::Close));
        }
    });
}
