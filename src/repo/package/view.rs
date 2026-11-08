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
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-package-contents-b60559a9",
        ))
        .font(theme::semibold(15.0)),
    );
    let list = match &view.packages {
        None => {
            ui.add(crate::spinner::Spinner::new());
            buttons(ui, acts, false);
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(
                t.danger,
                crate::i18n::message(
                    "repo-workflows-package-contents-e237505e",
                    &[("e", (e).clone().into())],
                ),
            );
            buttons(ui, acts, true);
            return;
        }
        Some(Ok(list)) if list.is_empty() => {
            ui.label(
                RichText::new(crate::i18n::text(
                    "repo-workflows-package-contents-8ed27415",
                ))
                .color(t.text_dim),
            );
            buttons(ui, acts, true);
            return;
        }
        Some(Ok(list)) => list,
    };
    let pkg = &list[view.selected.min(list.len() - 1)];
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-package-contents-59de121d",
            ))
            .color(t.text_dim),
        );
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
        RichText::new(crate::i18n::message(
            "repo-workflows-package-contents-a2e9d64e",
            &[
                (
                    "pkg-ecosystem-tool-1",
                    (pkg.ecosystem.tool()).clone().into(),
                ),
                (
                    "if-pkg-dir-is-empty-the-reposito-2",
                    (if pkg.dir.is_empty() {
                        crate::i18n::text("repo-workflows-package-contents-d5728cc3")
                    } else {
                        &pkg.dir
                    })
                    .clone()
                    .into(),
                ),
            ],
        ))
        .color(t.text_faint)
        .small(),
    );
    ui.add_space(6.0);
    match &view.report {
        None => {
            ui.add(crate::spinner::Spinner::new());
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-package-contents-b32a9827",
                    &[(
                        "pkg-ecosystem-tool-1",
                        (pkg.ecosystem.tool()).clone().into(),
                    )],
                ))
                .color(t.text_faint)
                .small(),
            );
        }
        Some(Err(e)) => {
            ui.colored_label(
                t.danger,
                crate::i18n::message(
                    "repo-workflows-package-contents-89bee303",
                    &[("e", (e).clone().into())],
                ),
            );
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
        crate::i18n::text("repo-workflows-package-report-ui-fefc6039"),
        &report.accidental,
        t.warning,
    );
    notes(
        ui,
        crate::i18n::text("repo-workflows-package-report-ui-fa5b38db"),
        &report.missing,
        t.warning,
    );
    heading(
        ui,
        &crate::i18n::message(
            "repo-workflows-package-report-ui-b2667be4",
            &[(
                "registry-to-uppercase-1",
                (registry.to_uppercase()).clone().into(),
            )],
        ),
    );
    match published {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                faint(
                    ui,
                    &crate::i18n::message(
                        "repo-workflows-package-report-ui-b413d4db",
                        &[("registry", (registry).clone().into())],
                    ),
                );
            });
        }
        Some(Ok(None)) => faint(
            ui,
            &crate::i18n::message(
                "repo-workflows-package-report-ui-b47c6e02",
                &[("registry", (registry).clone().into())],
            ),
        ),
        Some(Err(e)) => faint(
            ui,
            &crate::i18n::message(
                "repo-workflows-package-report-ui-74f3a93b",
                &[("e", (e).clone().into())],
            ),
        ),
        Some(Ok(Some(p))) => {
            if p.added.is_empty() && p.gone.is_empty() {
                faint(
                    ui,
                    &crate::i18n::message(
                        "repo-workflows-package-report-ui-ef333981",
                        &[
                            ("p-version-1", (p.version).clone().into()),
                            ("registry", (registry).clone().into()),
                        ],
                    ),
                );
            }
            for f in p.added.iter().take(SHOWN) {
                let tip = crate::i18n::message(
                    "repo-workflows-package-report-ui-6c125757",
                    &[("p-version-1", (p.version).clone().into())],
                );
                line(ui, &format!("+ {f}"), t.success, &tip);
            }
            more(ui, p.added.len());
            for f in p.gone.iter().take(SHOWN) {
                let tip = crate::i18n::message(
                    "repo-workflows-package-report-ui-47ec594b",
                    &[("p-version-1", (p.version).clone().into())],
                );
                line(ui, &format!("− {f}"), t.danger, &tip);
            }
            more(ui, p.gone.len());
        }
    }
    heading(
        ui,
        &crate::i18n::message(
            "repo-workflows-package-report-ui-d4826304",
            &[("report-files-len-1", (report.files.len()).clone().into())],
        ),
    );
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
        faint(
            ui,
            crate::i18n::text("repo-workflows-package-notes-adea78e6"),
        );
    }
    for n in notes.iter().take(SHOWN) {
        line(ui, &format!("{} — {}", n.path, n.why), color, "");
    }
    more(ui, notes.len());
}

/// The count of the rows of a list of `len` not shown.
fn more(ui: &mut Ui, len: usize) {
    if len > SHOWN {
        faint(
            ui,
            &crate::i18n::message(
                "repo-workflows-package-more-683768ab",
                &[("len-shown-1", (len - SHOWN).clone().into())],
            ),
        );
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
                egui::Button::new(crate::i18n::text("repo-workflows-package-buttons-0e916101"))
                    .min_size(egui::vec2(80.0, 30.0)),
            )
            .on_hover_text(crate::i18n::text("repo-workflows-package-buttons-8a24ec7c"))
            .clicked()
        {
            acts.push(Action::Package(PackageAct::Refresh));
        }
        if ui
            .add(
                egui::Button::new(crate::i18n::text("repo-workflows-package-buttons-7d9eb7ac"))
                    .min_size(egui::vec2(80.0, 30.0)),
            )
            .on_hover_text("Esc")
            .clicked()
        {
            acts.push(Action::Package(PackageAct::Close));
        }
    });
}
