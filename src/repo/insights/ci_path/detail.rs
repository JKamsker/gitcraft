//! What the explorer says in words: the critical path and the comparison above the picture, the
//! jobs as a table, and the picked job with its steps, what it waited for, what the forge
//! recorded differently of it, and its links.

use egui::{RichText, Ui};

use super::{Act, act, state_word, waterfall};
use crate::ci::timing::TimedJob;
use crate::ci::{self, CheckState};
use crate::insights::Evidence;
use crate::insights::ci_path::Explorer;
use crate::repo::insights::common;
use crate::repo::{Action, short};
use crate::theme::Tokens;

/// `1m 2s`, or `?` when it isn't known.
pub(super) fn seconds(value: Option<i64>) -> String {
    value.map_or_else(|| "?".to_owned(), ci::duration_label)
}

/// `+1m 2s`, `-5s`, `±0s`.
pub(super) fn signed(delta: i64) -> String {
    match delta {
        0 => "±0s".to_owned(),
        d if d > 0 => format!("+{}", ci::duration_label(d)),
        d => format!("-{}", ci::duration_label(-d)),
    }
}

/// Seconds a job ran, queueing aside, when both ends are known.
pub(super) fn ran(job: &TimedJob) -> Option<i64> {
    Some((job.finished? - job.started?).max(0))
}

/// Seconds it waited for a runner.
pub(super) fn queued(job: &TimedJob) -> Option<i64> {
    Some((job.started? - job.ready()?).max(0))
}

fn faint(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).size(11.5).color(t.text_faint)).wrap());
}

/// The critical path, the dependencies' source and the comparison, in a few lines.
pub(super) fn summary(ui: &mut Ui, explorer: &Explorer) {
    let t = Tokens::get(ui.ctx());
    let jobs = &explorer.run.jobs;
    let critical = &explorer.critical;
    if critical.jobs.is_empty() {
        ui.label(
            RichText::new("No job has finished: there is no critical path yet.").color(t.text_dim),
        );
    } else {
        let names: Vec<&str> = critical
            .jobs
            .iter()
            .map(|&i| jobs[i].name.as_str())
            .collect();
        let evidence = critical.evidence.unwrap_or(Evidence::Unknown).label();
        ui.label(
            RichText::new(format!(
                "Critical path ({evidence}): {} · {}",
                names.join(" → "),
                seconds(critical.span)
            ))
            .color(t.text),
        );
    }
    for note in &explorer.deps.notes {
        faint(ui, note);
    }
    if let Some(e) = &explorer.baseline_error {
        faint(ui, format!("The earlier run couldn't be read: {e}"));
    }
    let Some(compared) = &explorer.compared else {
        if explorer.baselines.is_empty() && explorer.baseline_error.is_none() {
            faint(
                ui,
                format!(
                    "No earlier successful run of {} on {} to compare with.",
                    explorer.run.info.name, explorer.run.info.branch
                ),
            );
        }
        return;
    };
    let c = &compared.comparison;
    ui.label(
        RichText::new(format!(
            "Against #{}: first start to last end {} (was {}); the jobs ran {} together (was {}); \
             critical path {} (was {}).",
            c.baseline.number,
            seconds(c.span_now),
            seconds(c.span_before),
            ci::duration_label(c.work_now),
            ci::duration_label(c.work_before),
            seconds(explorer.critical.span),
            seconds(compared.critical.span),
        ))
        .color(t.text_dim),
    );
    if c.lost_parallelism {
        ui.label(
            RichText::new(
                "Lost parallelism: the run took markedly longer end to end while its jobs ran \
                 about as long together, so fewer of them ran at once.",
            )
            .color(t.warning),
        );
    }
    let slower: Vec<String> = c
        .slower()
        .map(|j| {
            format!(
                "{} {} (was {})",
                j.name,
                j.delta().map_or("?".to_owned(), signed),
                seconds(j.took_before)
            )
        })
        .collect();
    if !slower.is_empty() {
        ui.add(
            egui::Label::new(
                RichText::new(format!("Newly slow: {}", slower.join(", "))).color(t.warning),
            )
            .wrap(),
        );
    }
}

/// The jobs as a table: the picture's fallback.
pub(super) fn table(
    ui: &mut Ui,
    explorer: &Explorer,
    selected: Option<usize>,
    acts: &mut Vec<Action>,
) {
    let widths = [26.0, 220.0, 70.0, 70.0, 70.0, 80.0, 80.0, 80.0];
    let header = [
        "",
        "Job",
        "State",
        "Queued",
        "Ran",
        "Starts at",
        "Before",
        "Change",
        "Waits for",
    ]
    .map(str::to_owned);
    common::table_row(ui, &widths, &header, true, false);
    let start = waterfall::origin(explorer);
    let compared = explorer.compared.as_ref().map(|c| &c.comparison);
    for (i, job) in explorer.run.jobs.iter().enumerate() {
        let delta = compared.and_then(|c| c.jobs.iter().find(|d| d.now == Some(i)));
        let waits: Vec<String> = explorer
            .deps
            .before(i)
            .map(|d| {
                format!(
                    "{} ({})",
                    explorer.run.jobs[d.from].name,
                    d.evidence.label()
                )
            })
            .collect();
        let cells = [
            if explorer.critical.contains(i) {
                "C"
            } else {
                ""
            }
            .to_owned(),
            job.name.clone(),
            state_word(job.state).to_owned(),
            seconds(queued(job)),
            seconds(ran(job)),
            job.started
                .or(job.ready())
                .zip(start)
                .map_or("?".to_owned(), |(s, o)| {
                    format!("+{}", ci::duration_label(s - o))
                }),
            delta.map_or(String::new(), |d| seconds(d.took_before)),
            delta
                .and_then(|d| d.delta().map(|x| (x, d.slower)))
                .map_or(String::new(), |(x, slow)| {
                    format!("{}{}", signed(x), if slow { " slower" } else { "" })
                }),
            if waits.is_empty() {
                "nothing known".to_owned()
            } else {
                waits.join(", ")
            },
        ];
        let resp = common::table_row(ui, &widths, &cells, false, selected == Some(i));
        if resp.clicked() {
            let pick = if selected == Some(i) { None } else { Some(i) };
            acts.push(act(Act::Select(pick)));
        }
    }
    if let Some(c) = compared {
        let gone: Vec<&str> = c
            .jobs
            .iter()
            .filter(|d| d.now.is_none())
            .map(|d| d.name.as_str())
            .collect();
        if !gone.is_empty() {
            faint(ui, format!("Only in the earlier run: {}", gone.join(", ")));
        }
    }
}

/// The picked job: its times, what it waited for, the comparison, its steps and links.
pub(super) fn job(
    ui: &mut Ui,
    explorer: &Explorer,
    index: usize,
    expanded: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let Some(job) = explorer.run.jobs.get(index) else {
        return;
    };
    common::heading(ui, &job.name);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open in CI view").clicked() {
            acts.push(Action::OpenCi {
                sha: explorer.run.info.sha.clone(),
                run: Some(explorer.run.info.id),
            });
        }
        if !job.url.is_empty()
            && ui
                .button("Job log on the forge")
                .on_hover_text(&job.url)
                .clicked()
        {
            acts.push(Action::OpenUrl(job.url.clone()));
        }
        if ui
            .button(format!("Select commit {}", short(&explorer.run.info.sha)))
            .clicked()
        {
            acts.push(Action::SelectHash(explorer.run.info.sha.clone()));
        }
        if !job.steps.is_empty() {
            let text = if expanded {
                "Hide steps in the picture"
            } else {
                "Show steps in the picture"
            };
            if ui.button(text).clicked() {
                acts.push(act(Act::Expand(index)));
            }
        }
        if ui.button("Unpick").on_hover_text("Esc").clicked() {
            acts.push(act(Act::Select(None)));
        }
    });
    let mut facts = vec![
        format!("{}", state_word(job.state)),
        format!("queued {}", seconds(queued(job))),
        format!("ran {}", seconds(ran(job))),
    ];
    if job.state == CheckState::Running {
        facts.push("still running when read".to_owned());
    }
    if !job.stage.is_empty() {
        facts.push(format!("stage {}", job.stage));
    }
    match explorer.deps.matched.get(index).and_then(Option::as_ref) {
        Some((id, evidence)) => {
            facts.push(format!("workflow job `{id}` ({})", evidence.label()));
        }
        None if explorer.deps.source == crate::insights::ci_path::DepSource::File => {
            facts.push("not matched to the workflow file".to_owned());
        }
        None => {}
    }
    ui.label(RichText::new(facts.join(" · ")).color(t.text_dim));
    let waits: Vec<String> = explorer
        .deps
        .before(index)
        .map(|d| {
            let on = if explorer.critical.link(d.from, index) {
                ", on the critical path"
            } else {
                ""
            };
            format!(
                "{} ({}{on})",
                explorer.run.jobs[d.from].name,
                d.evidence.label()
            )
        })
        .collect();
    faint(
        ui,
        if waits.is_empty() {
            format!(
                "Waits for: nothing known (dependencies from {})",
                explorer.deps.source.label()
            )
        } else {
            format!("Waits for: {}", waits.join(", "))
        },
    );
    let delta = explorer
        .compared
        .as_ref()
        .and_then(|c| c.comparison.jobs.iter().find(|d| d.now == Some(index)));
    if let Some(d) = delta {
        if d.before.is_none() {
            faint(ui, "Not in the earlier run.");
        } else {
            faint(
                ui,
                format!(
                    "Earlier run: ran {}, queued {}; now {}{}",
                    seconds(d.took_before),
                    seconds(d.queued_before),
                    d.delta().map_or("?".to_owned(), signed),
                    if d.slower { ", newly slow" } else { "" }
                ),
            );
        }
        if !d.differences.is_empty() {
            ui.label(
                RichText::new("Recorded differences (what the forge recorded; not causes):")
                    .color(t.text_dim),
            );
            for diff in &d.differences {
                faint(
                    ui,
                    format!("  {}: {} → {}", diff.what, diff.before, diff.now),
                );
            }
        }
    }
    if !job.runner.is_empty() {
        let runner: Vec<String> = job
            .runner
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        faint(ui, format!("Recorded: {}", runner.join(" · ")));
    }
    if job.steps.is_empty() {
        faint(
            ui,
            "The forge lists no steps for this job (GitLab doesn't list steps).",
        );
        return;
    }
    ui.add_space(4.0);
    let widths = [260.0, 70.0, 70.0, 80.0, 90.0];
    let header = ["Step", "State", "Ran", "Before", "Change", "Log"].map(str::to_owned);
    common::table_row(ui, &widths, &header, true, false);
    for (n, step) in job.steps.iter().enumerate() {
        let sd = delta.and_then(|d| {
            d.steps.iter().filter(|s| s.name == step.name).nth(
                job.steps[..n]
                    .iter()
                    .filter(|s| s.name == step.name)
                    .count(),
            )
        });
        let took = step.started.zip(step.finished).map(|(s, f)| (f - s).max(0));
        let cells = [
            step.name.clone(),
            state_word(step.state).to_owned(),
            seconds(took),
            sd.map_or(String::new(), |s| seconds(s.before)),
            sd.and_then(|s| s.now.zip(s.before).map(|(a, b)| (a - b, s.slower)))
                .map_or(String::new(), |(x, slow)| {
                    format!("{}{}", signed(x), if slow { " slower" } else { "" })
                }),
            "open log".to_owned(),
        ];
        let resp = common::table_row(ui, &widths, &cells, false, false)
            .on_hover_text("Open this step's log on the forge");
        if resp.clicked() {
            acts.push(Action::OpenUrl(job.step_url(step)));
        }
    }
}
