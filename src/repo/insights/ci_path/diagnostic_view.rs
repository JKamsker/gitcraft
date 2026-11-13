//! Failure transitions and observed same-SHA history alongside the existing timing view.

use super::*;
use crate::insights::ci_path::diagnostics;

pub(super) fn summary(ui: &mut Ui, explorer: &Explorer) {
    if explorer.project.forge != crate::forge::Forge::GitHub {
        return;
    }
    ui.separator();
    ui.label("Failure status changes (separate from duration changes)");
    if explorer.compared.is_none() {
        ui.label("No baseline status evidence selected.");
    }
    for job in &explorer.diagnostics {
        let name = job
            .pair
            .now
            .and_then(|i| explorer.run.jobs.get(i))
            .or_else(|| {
                job.pair
                    .before
                    .and_then(|i| explorer.compared.as_ref()?.run.jobs.get(i))
            })
            .map_or("unknown job", |job| job.name.as_str());
        ui.label(format!(
            "{name}: {} · identity {}",
            job.transition.label(),
            job.pair.evidence.label()
        ));
    }
    ui.label(diagnostics::UNKNOWN_EVIDENCE);
    ui.label(format!(
        "Workflow revision: {} at {}",
        explorer.run.info.path, explorer.run.info.sha
    ));
    if let Some(before) = &explorer.compared {
        ui.label(format!(
            "Baseline workflow revision: {} at {}",
            before.run.info.path, before.run.info.sha
        ));
    }
    for note in &explorer.history.notes {
        ui.label(note);
    }
    if explorer.history.more {
        ui.label("History is a bounded sample; missing runs/attempts are unknown.");
    }
}

pub(super) fn job(ui: &mut Ui, explorer: &Explorer, index: usize) {
    if explorer.project.forge != crate::forge::Forge::GitHub {
        return;
    }
    let Some(job) = explorer.run.jobs.get(index) else {
        return;
    };
    ui.separator();
    let observations = diagnostics::observations(&explorer.run, index, None, &explorer.history);
    let label = if diagnostics::intermittent(&observations) {
        "Observed job outcomes vary at this SHA (intermittent); individual test flakiness is unknown."
    } else {
        "Observed job history at this workflow/branch/SHA; no claim about individual test flakiness."
    };
    ui.label(label);
    history_links(ui, &observations);
    if job.runner.is_empty() {
        ui.label("Runner metadata: unknown (not recorded).");
    }
    let diagnostic = explorer
        .diagnostics
        .iter()
        .find(|entry| entry.pair.now == Some(index));
    for (step_index, step) in job.steps.iter().enumerate() {
        let transition = diagnostic
            .and_then(|job| {
                job.steps
                    .iter()
                    .find(|step| step.pair.now == Some(step_index))
            })
            .map_or(diagnostics::Transition::Unknown, |step| step.transition);
        egui::CollapsingHeader::new(format!("{} · {}", step.name, transition.label()))
            .id_salt((index, step_index, "ci-step-history"))
            .show(ui, |ui| {
                let observations = diagnostics::observations(
                    &explorer.run,
                    index,
                    Some(step_index),
                    &explorer.history,
                );
                if diagnostics::intermittent(&observations) {
                    ui.label(
                        "Observed step outcomes vary at this SHA; test-report evidence is unknown.",
                    );
                }
                history_links(ui, &observations);
            });
    }
}

pub(super) fn history_links(ui: &mut Ui, observations: &[diagnostics::Observation]) {
    if observations.is_empty() {
        ui.label("History unknown: no recorded attempts available.");
    }
    ui.horizontal_wrapped(|ui| {
        for observation in observations {
            let state = observation
                .state
                .map_or("unknown / not aligned", state_word);
            ui.hyperlink_to(
                format!(
                    "Run {} attempt {}: {state}",
                    observation.key.run, observation.key.attempt
                ),
                &observation.url,
            );
        }
    });
}
