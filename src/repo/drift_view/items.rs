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
        "No task to go by: write one above (or link an issue) to tell the folders apart.".to_owned()
    } else {
        format!("Asked: {}", task.lines().next().unwrap_or_default())
    };
    ui.label(RichText::new(lead).size(12.0).color(t.text_dim));
    ui.label(RichText::new(report.headline()).font(theme::semibold(13.0)));
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
            &format!("Folders the task doesn't mention ({})", unrelated.len()),
            |ui| {
                for folder in unrelated.iter().take(LISTED) {
                    let name = if folder.path.is_empty() {
                        "(top level)"
                    } else {
                        folder.path.as_str()
                    };
                    let label = format!(
                        "{name}  ({} file{})",
                        folder.files.len(),
                        if folder.files.len() == 1 { "" } else { "s" }
                    );
                    item(ui, view, report, &label, &folder.files, acts);
                }
            },
        );
    }
    section(
        ui,
        &format!("New dependencies ({})", report.dependencies.len()),
        |ui| {
            for dep in report.dependencies.iter().take(LISTED) {
                let label = format!("{}  in {}", dep.name, dep.manifest);
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
        &format!("Public signatures changed ({})", report.signatures.len()),
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
    let title = format!(
        "Formatting only: {}% of {} changed lines ({} file{} only that)",
        report.formatting_percent(),
        report.changed_lines,
        report.formatting_files.len(),
        if report.formatting_files.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    section(ui, &title, |ui| {
        for file in report.formatting_files.iter().take(LISTED) {
            item(ui, view, report, file, std::slice::from_ref(file), acts);
        }
    });
    section(
        ui,
        &format!("Every file changed ({})", report.files.len()),
        |ui| {
            for file in report.files.iter().take(LISTED * 5) {
                item(ui, view, report, file, std::slice::from_ref(file), acts);
            }
        },
    );
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).font(theme::medium(12.5)))
        .id_salt(title.split(" (").next().unwrap_or(title))
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
        "Not committed in that worktree: opens it, where its changes are".to_owned()
    } else if files.len() > 1 {
        let mut tip: Vec<&str> = files.iter().map(String::as_str).take(12).collect();
        if files.len() > 12 {
            tip.push("…");
        }
        format!("Opens:\n{}", tip.join("\n"))
    } else {
        "Opens its diff".to_owned()
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
            since_fork: false,
        });
        if let Some(key) = view.diff_key(report, first) {
            acts.push(Action::OpenDiff(key));
        }
    }
}
