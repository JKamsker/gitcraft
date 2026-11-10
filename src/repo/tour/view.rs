//! Repository tour presentation and navigation actions.

use super::*;

pub(super) fn contents(
    ui: &mut egui::Ui,
    view: &mut TourView,
    busy: bool,
    acts: &mut Vec<Action>,
) -> (bool, bool) {
    let mut close = false;
    let mut refresh = false;
    ui.horizontal(|ui| {
        ui.heading(crate::i18n::text("repo-workflows-tour-tour-view-3174faca"));
        refresh = ui
            .button(crate::i18n::text("repo-workflows-tour-tour-view-f98bf467"))
            .clicked();
        close = ui
            .button(if view.loading {
                crate::i18n::text("repo-workflows-tour-tour-view-712320ba")
            } else {
                crate::i18n::text("repo-workflows-tour-tour-view-2cf3aea7")
            })
            .clicked();
    });
    ui.label(crate::i18n::text("repo-workflows-tour-tour-view-49fa26c7"));
    if view.loading {
        ui.add(crate::spinner::Spinner::new());
        ui.label(crate::i18n::text("repo-workflows-tour-tour-view-aaf81fb7"));
    }
    if let Some(e) = &view.error {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).danger,
            tour::presentation::diagnostic(e),
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("repository_tour")
        .show(ui, |ui| {
            let Some(report) = &view.report else {
                return;
            };
            let h = &report.history;
            ui.label(crate::i18n::message(
                "repo-workflows-tour-scan-summary",
                &[
                    ("count", (h.commits.len()).into()),
                    ("partial", (if h.more { "yes" } else { "no" }).into()),
                    ("shallow", (if h.shallow { "yes" } else { "no" }).into()),
                    (
                        "head",
                        (report.head.get(..12).unwrap_or(&report.head)).into(),
                    ),
                ],
            ));
            ui.label(crate::i18n::text("repo-workflows-tour-tour-view-55b8885f"));
            if h.bytes_limited {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-af4145bc"));
            }
            ui.separator();
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-17cc361f"));
            for document in &report.documents {
                let readme = document.path.to_ascii_lowercase().starts_with("readme");
                egui::CollapsingHeader::new(&document.path)
                    .default_open(readme || view.show_help)
                    .show(ui, |ui| {
                        ui.label(&document.text);
                        if document.truncated {
                            ui.label(crate::i18n::text("repo-workflows-tour-tour-view-ddb4a38f"));
                        }
                    });
            }
            for unavailable in &report.documents_unavailable {
                ui.label(tour::presentation::document_unavailable(unavailable));
            }
            if report.documents.is_empty() {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-0715e061"));
            }
            ui.separator();
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-8e490f8b"));
            let w = &report.workflow;
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-75d74f36",
                &[
                    ("w-inspected-1", (w.inspected).into()),
                    ("w-merges-2", (w.merges).into()),
                    ("w-conventional-3", (w.conventional).into()),
                    ("w-signature-headers-4", (w.signature_headers).into()),
                    ("w-signoffs-5", (w.signoffs).into()),
                ],
            ));
            ui.label(if w.merges == 0 {
                crate::i18n::text("repo-workflows-tour-tour-view-f462deb1")
            } else {
                crate::i18n::text("repo-workflows-tour-tour-view-7ba62dbe")
            });
            if let Some(e) = &w.incomplete {
                ui.label(tour::presentation::diagnostic(e));
            }
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-f2c602f9"));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-17f176ca",
                &[
                    ("report-branches-len-1", (report.branches.len()).into()),
                    ("report-tags-len-2", (report.tags.len()).into()),
                ],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-b71999b4",
                &[(
                    "report-branches-iter-take-30-clo-1",
                    (report
                        .branches
                        .iter()
                        .take(30)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "))
                    .clone()
                    .into(),
                )],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-dfbf1d23",
                &[(
                    "report-tags-iter-take-30-cloned--1",
                    (report
                        .tags
                        .iter()
                        .take(30)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "))
                    .clone()
                    .into(),
                )],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-152f32cc",
                &[
                    (
                        "report-branches-iter-filter-s-s--1",
                        (report
                            .branches
                            .iter()
                            .filter(|s| s.contains("/release/"))
                            .count())
                        .into(),
                    ),
                    (
                        "report-tags-iter-filter-s-s-star-2",
                        (report.tags.iter().filter(|s| s.starts_with('v')).count()).into(),
                    ),
                ],
            ));
            ui.separator();
            ui.strong(crate::i18n::message(
                "repo-workflows-tour-tour-view-325dcc8e",
                &[
                    ("report-since-1", report.since.display().into()),
                    ("report-through-2", report.through.display().into()),
                ],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-e78ace8e",
                &[("report-recent-commits-1", (report.recent_commits).into())],
            ));
            for folder in &report.folders {
                ui.label(format!(
                    "{}: {}",
                    folder.display_path(),
                    folder
                        .authors
                        .iter()
                        .map(|(n, e, c)| crate::i18n::message(
                            "repo-workflows-tour-tour-view-009719be",
                            &[
                                ("n", (n).clone().into()),
                                ("e", (e).clone().into()),
                                ("c", (*(c)).into())
                            ]
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-d00db6bc"));
            for (path, n) in &report.hot_files {
                ui.label(crate::i18n::message(
                    "repo-workflows-tour-tour-view-6a2e97ea",
                    &[
                        ("n", (*(n)).into()),
                        (
                            "string-from-utf8-lossy-path-1",
                            (String::from_utf8_lossy(path)).clone().into(),
                        ),
                    ],
                ));
            }
            ui.separator();
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-0e4924f9"));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-939f2714",
                &[(
                    "if-report-needs-is-empty-no-list-1",
                    (if report.needs.is_empty() {
                        crate::i18n::text("repo-workflows-tour-tour-view-78ab5e15").into()
                    } else {
                        report
                            .needs
                            .iter()
                            .map(|raw| tour::presentation::need(raw))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .clone()
                    .into(),
                )],
            ));
            ui.label(crate::i18n::text("repo-workflows-tour-tour-view-96c17e04"));
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-dcd1d522"));
                ui.text_edit_singleline(&mut view.name);
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-969ccbd3"));
                ui.text_edit_singleline(&mut view.email);
            });
            let valid = !view.name.trim().is_empty()
                && !view.email.trim().is_empty()
                && !view.name.chars().any(char::is_control)
                && !view.email.chars().any(char::is_control)
                && !busy;
            if ui
                .add_enabled(
                    valid,
                    egui::Button::new(crate::i18n::text("repo-workflows-tour-tour-view-095444dc")),
                )
                .clicked()
            {
                acts.push(Action::SetIdentity(view.name.clone(), view.email.clone()));
            }
            if report.needs.contains("Git LFS") {
                ui.hyperlink_to(
                    crate::i18n::text("repo-workflows-tour-tour-view-4015deeb"),
                    "https://git-lfs.com/",
                );
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-tour-tour-view-f0a6629b",
                        )),
                    )
                    .clicked()
                {
                    acts.push(Action::Ops {
                        label: "Install local Git LFS hooks".into(),
                        ops: ops(&[&["lfs", "install", "--local"]]),
                        refresh: Refresh::All,
                    });
                }
            }
            if report.needs.contains("Submodules")
                && ui
                    .button(crate::i18n::text("repo-workflows-tour-tour-view-2189fda2"))
                    .clicked()
            {
                acts.push(Action::Dialog(crate::repo::Dialog::confirm(
                    "Initialize submodules",
                    crate::i18n::text("repo-workflows-tour-tour-view-e64cb6f2").into(),
                    crate::i18n::text("repo-workflows-tour-tour-view-7bfe85f2"),
                    ops(&[&["submodule", "update", "--init", "--recursive"]]),
                    None,
                )));
            }
            if ui
                .button(crate::i18n::text("repo-workflows-tour-tour-view-d65a30c1"))
                .clicked()
            {
                view.show_help = !view.show_help;
            }
            if view.show_help {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-6f54c5e0"));
            }
            if valid
                && ui
                    .button(crate::i18n::text("repo-workflows-tour-tour-view-5b0a8af2"))
                    .clicked()
            {
                ui.ctx()
                    .copy_text(format!("Signed-off-by: {} <{}>", view.name, view.email));
            }
        });

    (close, refresh)
}
