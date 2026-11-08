//! The figures of a drift report, each item opening its files.

use egui::{RichText, Ui};

use super::DriftView;
use crate::drift::Report;
use crate::repo::Action;
use crate::theme::{self, Tokens};

/// Most rows a section lists.
const LISTED: usize = 40;

pub(super) fn report(ui: &mut Ui, view: &DriftView, report: &Report, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let task = view.task_text();
    let lead = if task.trim().is_empty() {
        crate::i18n::text("repo-workflows-drift_view-report-57f4cf0a").to_owned()
    } else {
        crate::i18n::message(
            "repo-workflows-drift_view-report-3ecc0ca7",
            &[(
                "task-lines-next-unwrap-or-defaul-1",
                (task.lines().next().unwrap_or_default()).into(),
            )],
        )
    };
    ui.label(RichText::new(lead).size(12.0).color(t.text_dim));
    ui.label(RichText::new(report.display_headline()).font(theme::semibold(13.0)));
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt("drift-items")
        .max_height(340.0)
        .show(ui, |ui| sections(ui, view, report, acts));
}

fn sections(ui: &mut Ui, view: &DriftView, report: &Report, acts: &mut Vec<Action>) {
    let unrelated: Vec<_> = report.unrelated().collect();
    if report.judged {
        section(
            ui,
            "unrelated",
            &crate::i18n::message(
                "repo-workflows-drift_view-sections-1bb434ba",
                &[("unrelated-len-1", (unrelated.len()).into())],
            ),
            |ui| {
                for folder in unrelated.iter().take(LISTED) {
                    let name = if folder.path.is_empty() {
                        crate::i18n::text("repo-workflows-drift_view-sections-4cf1b0e4")
                    } else {
                        folder.path.as_str()
                    };
                    let label = crate::i18n::message(
                        "repo-workflows-drift-folder-count",
                        &[
                            ("name", (name).into()),
                            ("count", (folder.files.len()).into()),
                        ],
                    );
                    item(ui, view, report, &label, &folder.files, acts);
                }
            },
        );
    }
    section(
        ui,
        "dependencies",
        &crate::i18n::message(
            "repo-workflows-drift_view-sections-14063185",
            &[(
                "report-dependencies-len-1",
                (report.dependencies.len()).into(),
            )],
        ),
        |ui| {
            for dep in report.dependencies.iter().take(LISTED) {
                let label = crate::i18n::message(
                    "repo-workflows-drift_view-sections-819e9567",
                    &[
                        ("dep-name-1", (dep.name).clone().into()),
                        ("dep-manifest-2", (dep.manifest).clone().into()),
                    ],
                );
                item(
                    ui,
                    view,
                    report,
                    &label,
                    std::slice::from_ref(&dep.manifest),
                    acts,
                );
            }
        },
    );
    section(
        ui,
        "signatures",
        &crate::i18n::message(
            "repo-workflows-drift_view-sections-a8299dbd",
            &[("report-signatures-len-1", (report.signatures.len()).into())],
        ),
        |ui| {
            for sig in report.signatures.iter().take(LISTED) {
                let label = format!("{}:{}  {}", sig.path, sig.line, sig.text);
                item(
                    ui,
                    view,
                    report,
                    &label,
                    std::slice::from_ref(&sig.path),
                    acts,
                );
            }
        },
    );
    let title = crate::i18n::message(
        "repo-workflows-drift-formatting-summary",
        &[
            ("percent", (report.formatting_percent()).into()),
            ("lines", (report.changed_lines).into()),
            ("files", (report.formatting_files.len()).into()),
        ],
    );
    section(ui, "formatting", &title, |ui| {
        for file in report.formatting_files.iter().take(LISTED) {
            item(ui, view, report, file, std::slice::from_ref(file), acts);
        }
    });
    section(
        ui,
        "files",
        &crate::i18n::message(
            "repo-workflows-drift_view-sections-619bb740",
            &[("report-files-len-1", (report.files.len()).into())],
        ),
        |ui| {
            for file in report.files.iter().take(LISTED * 5) {
                item(ui, view, report, file, std::slice::from_ref(file), acts);
            }
        },
    );
}

fn section(ui: &mut Ui, id: &str, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).font(theme::medium(12.5)))
        .id_salt(id)
        .default_open(false)
        .show(ui, add);
}

/// A row that opens `files`: the comparison of the worktree's changes, the first file's diff.
fn item(
    ui: &mut Ui,
    view: &DriftView,
    report: &Report,
    label: &str,
    files: &[String],
    acts: &mut Vec<Action>,
) {
    let elsewhere = files
        .first()
        .is_some_and(|f| view.diff_key(report, f).is_none());
    let tip = if elsewhere {
        crate::i18n::text("repo-workflows-drift_view-item-518baa09").to_owned()
    } else if files.len() > 1 {
        let mut tip: Vec<&str> = files.iter().map(String::as_str).take(12).collect();
        if files.len() > 12 {
            tip.push("…");
        }
        crate::i18n::message(
            "repo-workflows-drift_view-item-d91938be",
            &[("tip-join-n-1", (tip.join("\n")).clone().into())],
        )
    } else {
        crate::i18n::text("repo-workflows-drift_view-item-bccfdf69").to_owned()
    };
    if ui
        .selectable_label(false, label)
        .on_hover_text(tip)
        .clicked()
        && let Some(first) = files.first()
    {
        acts.push(Action::Compare {
            base: report.fork.clone(),
            base_label: crate::repo::short(&report.fork).to_owned(),
            target: (!view.worktree.current).then(|| report.head.clone()),
            target_label: view
                .worktree
                .branch
                .clone()
                .unwrap_or_else(|| view.worktree.name()),
            captions: [None, None],
            since_fork: false,
        });
        if let Some(key) = view.diff_key(report, first) {
            acts.push(Action::OpenDiff(key));
        }
    }
}
#[cfg(test)]
mod fluent_section_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_drift_count_header_keeps_expansion_across_language_and_count() {
        crate::i18n::set_language("en-US");
        let mut initialized = false;
        let mut h = Harness::new_ui_state(
            |ui, count: &mut usize| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    initialized = true;
                    ui.ctx().request_discard("install supplied fonts");
                    return;
                }
                let title = crate::i18n::message(
                    "repo-workflows-drift_view-sections-14063185",
                    &[("report-dependencies-len-1", (*count).into())],
                );
                section(ui, "dependencies", &title, |ui| {
                    ui.label("Supplied dependency body");
                });
            },
            1usize,
        );
        let title = crate::i18n::message(
            "repo-workflows-drift_view-sections-14063185",
            &[("report-dependencies-len-1", 1.into())],
        );
        assert!(h.query_by_label("Supplied dependency body").is_none());
        h.get_by_label(&title).click();
        h.run();
        h.get_by_label("Supplied dependency body");
        crate::i18n::set_language("de");
        *h.state_mut() = 1234;
        h.run();
        h.get_by_label("Supplied dependency body");
        let title = crate::i18n::message(
            "repo-workflows-drift_view-sections-14063185",
            &[("report-dependencies-len-1", 1234.into())],
        );
        h.get_by_label(&title).click();
        h.run();
        assert!(h.query_by_label("Supplied dependency body").is_none());
        crate::i18n::set_language("en-US");
        *h.state_mut() = 2;
        h.run();
        assert!(h.query_by_label("Supplied dependency body").is_none());
    }
}
