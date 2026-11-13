//! What the exact logs of the picked job record: per-test status changes against the
//! baseline, same-SHA test history with links to each attempt's job, and cache, toolchain and
//! error-annotation differences. Read on request; nothing is inferred from elapsed time.

use super::*;
use crate::insights::ci_path::log_evidence::Report;
use crate::insights::ci_path::test_history::{self, JobEvidence, Read};

pub(super) fn job(
    ui: &mut Ui,
    state: &State,
    explorer: &Explorer,
    index: usize,
    acts: &mut Vec<Action>,
) {
    if explorer.project.forge != crate::forge::Forge::GitHub {
        return;
    }
    let Some(plan) = test_history::plan(explorer, index) else {
        return;
    };
    ui.separator();
    let reads = std::iter::once(&plan.now)
        .chain(&plan.before)
        .chain(&plan.history)
        .map(|target| target.job)
        .collect::<HashSet<_>>()
        .len();
    let current = state.logs.ready().filter(|evidence| evidence.plan == plan);
    if current.is_none() && !state.logs.loading {
        if let Some(error) = state.logs.error() {
            ui.label(format!("The job logs couldn't be read: {error}"));
        }
        if ui
            .button(format!("Read job logs ({reads})"))
            .on_hover_text(
                "Test outcomes, cache hits, toolchain versions and errors from this job's log, \
                 its baseline job's and each same-SHA attempt's",
            )
            .clicked()
        {
            acts.push(act(Act::ReadLogs(index)));
        }
        return;
    }
    let Some(evidence) = current else {
        ui.label(format!("Reading {reads} job logs…"));
        return;
    };
    show(ui, evidence);
}

fn report(read: Option<&Read>) -> String {
    match read {
        None => "no aligned baseline job".to_owned(),
        Some(read) => match (&read.log, read.report()) {
            (Err(error), _) => format!("log unreadable ({error})"),
            (_, Some(report)) => report.label().to_owned(),
            (_, None) => Report::Missing.label().to_owned(),
        },
    }
}

fn show(ui: &mut Ui, evidence: &JobEvidence) {
    ui.label("Evidence from the job logs (separate from durations and job status)");
    ui.label(format!("This run: {}", report(Some(&evidence.now))));
    if evidence.plan.baseline.is_some() {
        ui.label(format!("Baseline: {}", report(evidence.before.as_ref())));
    }
    for read in evidence.history.iter().filter(|read| read.log.is_err()) {
        ui.label(format!(
            "Run {} attempt {}: {}",
            read.target.key.run,
            read.target.key.attempt,
            report(Some(read))
        ));
    }
    for key in &evidence.plan.unaligned {
        ui.label(format!(
            "Run {} attempt {}: no unambiguous matching job; its test outcomes are unknown.",
            key.run, key.attempt
        ));
    }
    for row in &evidence.rows {
        let flaky = if row.intermittent {
            " · outcomes vary at this SHA (flaky evidence)"
        } else {
            ""
        };
        egui::CollapsingHeader::new(format!(
            "{} › {}: {}{flaky}",
            row.test.0,
            row.test.1,
            row.transition.label()
        ))
        .id_salt(("ci-test-history", &row.test))
        .show(ui, |ui| diagnostic_view::history_links(ui, &row.history));
    }
    if evidence.more > 0 {
        ui.label(format!("…and {} more tests with a signal.", evidence.more));
    }
    ui.label(format!(
        "{} tests passed in both; {} without a definite outcome on both sides (absent, ignored, \
         ambiguous or unreported: unknown, not passed).",
        evidence.passed_both, evidence.unknown
    ));
    if let Ok(log) = &evidence.now.log {
        ui.label(format!("Cache (this run): {}", test_history::caches(log)));
        ui.label(format!(
            "Toolchain (this run): {}",
            test_history::toolchains(log)
        ));
    }
    for difference in &evidence.differences {
        ui.label(format!(
            "{}: {} → {}",
            difference.what, difference.before, difference.now
        ));
    }
    for error in &evidence.new_errors {
        ui.label(format!("Error new in this run's log: {error}"));
    }
    ui.label("Recorded differences are observations, not causes.");
}
