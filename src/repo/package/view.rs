//! The package panel.

use egui::{Context, RichText, Ui};

use super::{PackageAct, PackageView};
use crate::package::{Note, Report};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How many files of a list are shown; the rest are counted.
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
            close(ui, acts);
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.warning, e);
            close(ui, acts);
            return;
        }
        Some(Ok(list)) if list.is_empty() => {
            ui.label(
                RichText::new("No package of the repository can be published.").color(t.text_dim),
            );
            close(ui, acts);
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
            "As {} says it, in {}",
            pkg.ecosystem.tool(),
            if pkg.dir.is_empty() {
                "the repository's top"
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
                RichText::new("Asking the tool and the registry…")
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
                .show(ui, |ui| report_ui(ui, report, pkg.ecosystem.registry()));
        }
    }
    close(ui, acts);
}

fn report_ui(ui: &mut Ui, report: &Report, registry: &str) {
    let t = Tokens::get(ui.ctx());
    notes(
        ui,
        "LOOKS SHIPPED BY ACCIDENT",
        &report.accidental,
        t.warning,
    );
    notes(ui, "LEFT OUT", &report.missing, t.warning);
    heading(ui, &format!("COMPARED WITH {}", registry.to_uppercase()));
    match &report.published {
        Ok(None) => faint(ui, "Never published there."),
        Err(e) => faint(ui, &format!("Couldn't compare: {e}")),
        Ok(Some(p)) => {
            if p.added.is_empty() && p.gone.is_empty() {
                faint(ui, &format!("The same files as {} published.", p.version));
            }
            for f in p.added.iter().take(SHOWN) {
                line(
                    ui,
                    &format!("+ {f}"),
                    t.success,
                    &format!("New since {}", p.version),
                );
            }
            for f in p.gone.iter().take(SHOWN) {
                line(
                    ui,
                    &format!("− {f}"),
                    t.danger,
                    &format!("In {}, not in this one", p.version),
                );
            }
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
    if report.files.len() > SHOWN {
        faint(ui, &format!("and {} more", report.files.len() - SHOWN));
    }
}

fn notes(ui: &mut Ui, title: &str, notes: &[Note], color: egui::Color32) {
    heading(ui, &format!("{title} ({})", notes.len()));
    if notes.is_empty() {
        faint(ui, "Nothing.");
    }
    for n in notes {
        line(ui, &format!("{} — {}", n.path, n.why), color, "");
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

fn close(ui: &mut Ui, acts: &mut Vec<Action>) {
    ui.add_space(10.0);
    if ui
        .add(egui::Button::new("Close").min_size(egui::vec2(80.0, 30.0)))
        .clicked()
    {
        acts.push(Action::Package(PackageAct::Close));
    }
}
