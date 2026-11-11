//! Workflow/run/attempt and comparison selection controls.
use super::*;

pub(super) fn header(ui: &mut Ui, state: &State, runs: &Runs, acts: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        if let Some((name, sha)) = &state.subject {
            ui.label(format!("{name}:"));
            if ui
                .link(short(sha))
                .on_hover_text("Select this commit in the graph")
                .clicked()
            {
                acts.push(Action::SelectHash(sha.clone()));
            }
        }
        run_picker(ui, state, runs, acts);
        let Some(explorer) = state.explorer.ready() else {
            return;
        };
        attempt_picker(ui, explorer, acts);
        baseline_picker(ui, state, explorer, acts);
        if ui
            .button("Open in CI view")
            .on_hover_text("The run's jobs, logs and actions (latest attempt)")
            .clicked()
        {
            acts.push(Action::OpenCi {
                sha: explorer.run.info.sha.clone(),
                run: Some(explorer.run.info.id),
            });
        }
        ui.hyperlink_to("Exact attempt", explorer.run.info.attempt_url());
    });
}

fn run_picker(ui: &mut Ui, state: &State, runs: &Runs, acts: &mut Vec<Action>) {
    let shown = runs
        .list
        .iter()
        .position(|(project, run)| state.run.as_ref() == Some(&(project.clone(), run.id)));
    let label = |(project, run): &(Project, Run)| {
        format!(
            "{} #{} · attempt {} · {} · {}",
            run.name,
            run.number,
            run.attempt.max(1),
            state_word(run.state),
            project.path
        )
    };
    egui::ComboBox::from_id_salt("ci-path-run")
        .selected_text(shown.map_or("Pick a run".into(), |i| label(&runs.list[i])))
        .width(260.0)
        .show_ui(ui, |ui| {
            for (index, entry) in runs.list.iter().enumerate() {
                if ui
                    .selectable_label(Some(index) == shown, label(entry))
                    .clicked()
                {
                    acts.push(act(Act::PickRun(index)));
                }
            }
        });
}

fn attempt_picker(ui: &mut Ui, explorer: &Explorer, acts: &mut Vec<Action>) {
    if explorer.project.forge != crate::forge::Forge::GitHub {
        return;
    }
    let current = RunKey::of(&explorer.run.info);
    egui::ComboBox::from_id_salt("ci-path-attempt")
        .selected_text(format!("Pinned attempt {}", current.attempt))
        .show_ui(ui, |ui| {
            for run in &explorer.history.runs {
                let key = RunKey::of(&run.info);
                if key.run == current.run
                    && ui
                        .selectable_label(
                            key == current,
                            format!("Attempt {} · {}", key.attempt, state_word(run.info.state)),
                        )
                        .clicked()
                {
                    acts.push(act(Act::PickAttempt(key)));
                }
            }
        });
}

fn baseline_picker(ui: &mut Ui, state: &State, explorer: &Explorer, acts: &mut Vec<Action>) {
    ui.label("against");
    let picked = explorer.compared.as_ref().map_or_else(
        || "No comparison".into(),
        |compared| {
            format!(
                "#{} attempt {} · {}",
                compared.run.info.number,
                compared.run.info.attempt,
                short(&compared.run.info.sha)
            )
        },
    );
    egui::ComboBox::from_id_salt("ci-path-baseline")
        .selected_text(picked)
        .width(260.0)
        .show_ui(ui, |ui| {
            let mut shortcuts = vec![
                (Baseline::Latest, "Last green"),
                (Baseline::None, "Nothing"),
            ];
            if explorer.project.forge == crate::forge::Forge::GitHub {
                shortcuts.extend([
                    (Baseline::Previous, "Previous run"),
                    (
                        Baseline::PreviousAttempt,
                        "Previous attempt at the same SHA",
                    ),
                ]);
            }
            for (baseline, text) in shortcuts {
                if ui
                    .selectable_label(state.baseline == baseline, text)
                    .clicked()
                {
                    acts.push(act(Act::PickBaseline(baseline)));
                }
            }
            baseline_runs(ui, state, explorer, acts);
        });
}

fn baseline_runs(ui: &mut Ui, state: &State, explorer: &Explorer, acts: &mut Vec<Action>) {
    let mut shown = HashSet::new();
    for run in explorer.baselines.iter().chain(&explorer.recent) {
        let key = RunKey::of(run);
        if key == RunKey::of(&explorer.run.info) || !shown.insert(key) {
            continue;
        }
        let on =
            state.baseline == Baseline::Attempt(key) || state.baseline == Baseline::Run(run.id);
        let text = format!(
            "#{} attempt {} · {} · {}",
            run.number,
            key.attempt,
            short(&run.sha),
            state_word(run.state)
        );
        if ui.selectable_label(on, text).clicked() {
            acts.push(act(Act::PickBaseline(Baseline::Attempt(key))));
        }
    }
}
