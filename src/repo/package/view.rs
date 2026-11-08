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
                    &[("e", e.display().into())],
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
                    &[("e", e.display().into())],
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
    published: Option<&Result<Option<Published>, crate::package::Failure>>,
    registry: &str,
) {
    let t = Tokens::get(ui.ctx());
    if let Some(hint) = report.display_hint() {
        ui.add(egui::Label::new(RichText::new(hint).color(t.warning).small()).wrap());
    }
    notes(
        ui,
        report,
        crate::i18n::text("repo-workflows-package-report-ui-fefc6039"),
        &report.accidental,
        t.warning,
    );
    notes(
        ui,
        report,
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
                &[("e", e.display().into())],
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

fn notes(ui: &mut Ui, report: &Report, title: &str, notes: &[Note], color: egui::Color32) {
    heading(
        ui,
        &format!("{title} ({})", crate::i18n::number(notes.len())),
    );
    if notes.is_empty() {
        faint(
            ui,
            crate::i18n::text("repo-workflows-package-notes-adea78e6"),
        );
    }
    for n in notes.iter().take(SHOWN) {
        line(
            ui,
            &format!("{} — {}", n.path, report.display_why(n)),
            color,
            "",
        );
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

#[cfg(test)]
mod package_report_paint_tests {
    use super::*;

    #[test]
    fn retained_package_report_repaints_known_reasons_and_hint_without_changing_raw_data() {
        let report = Report {
            files: vec![("opaque/on, auf/12345.env".into(), Some(1536 * 1024))],
            accidental: vec![Note {
                path: "opaque/on, auf/12345.env".into(),
                why: "big for a package (1.5 MB)".into(),
            }, Note {
                path: "unknown/12345".into(),
                why: "provider text; package.json has a prepack script".into(),
            }],
            missing: vec![],
            hint: Some("package.json has a prepack script, which npm publish runs first and this listing doesn't: files it builds may show as left out or gone until it has run".into()),
        };
        let original = report.clone();
        let ctx = egui::Context::default();
        for (locale, reason) in [
            ("en-US", "big for a package (1.5 MB)"),
            ("de", "groß für ein Paket (1,5 MB)"),
            ("en-US", "big for a package (1.5 MB)"),
        ] {
            crate::i18n::set_language(locale);
            let shown = crate::repo::push_preflight::core_guard_paint_tests::paint(&ctx, |ui| {
                report_ui(ui, &report, Some(&Ok(None)), "registry-12345");
            });
            assert!(shown.contains(reason), "{shown}");
            let hint = if locale == "de" {
                "package.json enthält ein prepack-Skript"
            } else {
                "package.json has a prepack script"
            };
            assert!(shown.contains(hint), "{shown}");
            assert!(shown.contains("opaque/on, auf/12345.env"), "{shown}");
            assert!(
                shown.contains("provider text; package.json has a prepack script"),
                "{shown}"
            );
            assert_eq!(report, original);
        }
        crate::i18n::set_language("en-US");
    }
}

#[cfg(test)]
mod fluent_package_witness_tests {
    use super::*;
    use crate::package::{Ecosystem, Failure, Package};
    use egui_kittest::{Harness, kittest::Queryable};

    fn package() -> Package {
        Package {
            ecosystem: Ecosystem::Npm,
            name: "raw; {$name}".into(),
            version: "1234.5".into(),
            dir: "opaque/path\nmain".into(),
            registry: None,
        }
    }

    #[test]
    fn fluent_partition_actual_package_three_cached_failure_slots_repaint_without_request_or_action_changes()
     {
        let detail = "provider: No Cargo.toml or package.json at the top of the repository; ;\n{$raw} 日本語";
        let failures = [
            Failure::NoManifest {
                raw: "No Cargo.toml or package.json at the top of the repository".into(),
            },
            Failure::NpmPackAnswer {
                raw: format!("npm pack answered: {detail}"),
                detail: detail.into(),
            },
            Failure::AskRegistry {
                raw: format!("registry lookup: {detail}"),
                url: "https://example/1234;raw".into(),
                detail: detail.into(),
            },
        ];
        let report = Report { files: vec![("raw.env".into(), Some(1536 * 1024))], accidental: vec![Note { path: "raw.env".into(), why: "big for a package (1.5 MB)".into() }], missing: vec![], hint: Some("package.json has a prepack script, which npm publish runs first and this listing doesn't: files it builds may show as left out or gone until it has run".into()) };
        let original = report.clone();
        for (slot, failure) in failures.into_iter().enumerate() {
            let mut installed = false;
            let view = PackageView {
                packages: Some(if slot == 0 {
                    Err(failure.clone())
                } else {
                    Ok(vec![package()])
                }),
                report: Some(if slot == 1 {
                    Err(failure.clone())
                } else {
                    Ok(report.clone())
                }),
                published: Some(Err(failure.clone())),
                selected: 0,
                request: 1234,
            };
            let mut h = Harness::new_ui_state(
                |ui, state: &mut (PackageView, Vec<Action>)| {
                    if !installed {
                        crate::theme::install_fonts(ui.ctx());
                        installed = true;
                        ui.ctx().request_discard("supplied fonts");
                        return;
                    }
                    contents(ui, &state.0, 1400.0, &mut state.1);
                },
                (view, Vec::new()),
            );
            for locale in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(locale);
                h.run();
                let id = [
                    "repo-workflows-package-contents-e237505e",
                    "repo-workflows-package-contents-89bee303",
                    "repo-workflows-package-report-ui-74f3a93b",
                ][slot];
                let expected = crate::i18n::message(id, &[("e", failure.display().into())]);
                h.get_by_label(&expected);
                if locale == "de" {
                    assert_ne!(failure.display(), failure.raw());
                }
                assert_eq!(h.state().0.request, 1234);
                assert_eq!(h.state().0.selected, 0);
                assert!(h.state().1.is_empty());
                let retained = match slot {
                    0 => h.state().0.packages.as_ref().unwrap().as_ref().unwrap_err(),
                    1 => h.state().0.report.as_ref().unwrap().as_ref().unwrap_err(),
                    _ => h
                        .state()
                        .0
                        .published
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unwrap_err(),
                };
                assert_eq!(retained, &failure);
                assert_eq!(retained.clone().into_raw(), failure.raw());
                if slot == 2 {
                    let r = h.state().0.report.as_ref().unwrap().as_ref().unwrap();
                    h.get_by_label(&format!("raw.env — {}", r.display_why(&r.accidental[0])));
                    h.get_by_label(&r.display_hint().unwrap());
                    assert_eq!(r, &original);
                }
            }
            h.get_by_label(crate::i18n::text("repo-workflows-package-buttons-0e916101"))
                .click();
            h.run();
            assert!(matches!(
                h.state().1.as_slice(),
                [Action::Package(PackageAct::Refresh)]
            ));
            assert_eq!(h.state().0.request, 1234);
        }
        let opaque =
            Failure::Opaque("No Cargo.toml or package.json at the top of the repository".into());
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            assert_eq!(opaque.display(), opaque.raw());
        }
        crate::i18n::set_language("en-US");
    }
}
