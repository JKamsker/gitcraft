//! Deployment evidence legend, environment/run tables, and target details.

use super::*;

pub(in crate::repo::insights::deploy) fn legend(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    common::legend(
        ui,
        &[
            (common::Mark::Glyph("IN", t.success), "known included"),
            (
                common::Mark::Glyph("NO", t.text_dim),
                "not in the deployed commit",
            ),
            (common::Mark::Glyph("?", t.warning), "unknown"),
            (
                common::Mark::Glyph("--", t.text_faint),
                "no deployment recorded",
            ),
            (common::Mark::Glyph("#", t.text_dim), "artifact with digest"),
            (
                common::evidence_line(Evidence::Observed, t.text_dim),
                "recorded by the forge or Git",
            ),
            (
                common::evidence_line(Evidence::Unknown, t.text_dim),
                "not known",
            ),
        ],
    );
}

/// What the view always says: a run or a tag is not a deployment; and how it was read.
pub(in crate::repo::insights::deploy) fn caution(ui: &mut Ui, flow: &Flow) {
    let t = Tokens::get(ui.ctx());
    ui.add(
        egui::Label::new(
            RichText::new(
                "Only deployments the forge recorded count: a successful run or a tag is not \
                 taken as a deployment.",
            )
            .size(11.5)
            .color(t.text_faint),
        )
        .wrap(),
    );
    for note in &flow.notes {
        ui.label(RichText::new(note).size(11.5).color(t.warning));
    }
}

/// The environments as a table, always shown: what the picture says, reachable by keyboard.
pub(in crate::repo::insights::deploy) fn environments(
    ui: &mut Ui,
    flow: &Flow,
    selected: Option<Target>,
    acts: &mut Vec<Action>,
) {
    common::heading(ui, "Environments");
    if flow.environments.is_empty() {
        common::empty(
            ui,
            "The forge lists no environments and records no deployments for this project.",
        );
        return;
    }
    let widths = [140.0, 260.0, 90.0, 150.0, 120.0];
    let header = [
        "Environment",
        "This commit",
        "Deployed",
        "Reported",
        "Evidence age",
        "Queued or under way",
    ]
    .map(str::to_owned);
    common::table_row(ui, &widths, &header, true, false);
    for (e, env) in flow.environments.iter().enumerate() {
        let current = env.current.as_ref();
        let in_flight: Vec<String> = env
            .in_flight
            .iter()
            .map(|(d, contains)| {
                let has = match contains {
                    Some(true) => ", contains it",
                    Some(false) => ", doesn't contain it",
                    None => "",
                };
                format!("{} {}{has}", short(&d.sha), d.state().label())
            })
            .collect();
        let cells = [
            env.name.clone(),
            env.inclusion.label(),
            current.map_or("—".to_owned(), |d| short(&d.sha).to_owned()),
            current.map_or(String::new(), |d| {
                analysis::reported_label(d.reported, flow.now)
            }),
            current.map_or(String::new(), |d| {
                freshness_word(analysis::freshness(d.reported, flow.now)).to_owned()
            }),
            in_flight.join(", "),
        ];
        let on = selected == Some(Target::Env(e));
        if common::table_row(ui, &widths, &cells, false, on).clicked() {
            acts.push(act(Act::Select((!on).then_some(Target::Env(e)))));
        }
    }
}

/// The runs and their artifacts as a table.
pub(in crate::repo::insights::deploy) fn runs(
    ui: &mut Ui,
    flow: &Flow,
    selected: Option<Target>,
    acts: &mut Vec<Action>,
) {
    common::heading(ui, "Runs of the commit and their artifacts");
    if flow.runs.is_empty() {
        common::empty(ui, "No CI runs of this commit were found.");
        return;
    }
    let widths = [240.0, 90.0, 110.0, 200.0];
    let header = ["Run or artifact", "State", "Event", "Digest"].map(str::to_owned);
    common::table_row(ui, &widths, &header, true, false);
    for (r, run) in flow.runs.iter().enumerate() {
        let cells = [
            format!("{} #{}", run.name, run.number),
            run_word(run.state).to_owned(),
            run.event.clone(),
            String::new(),
        ];
        let on = selected == Some(Target::Run(r));
        if common::table_row(ui, &widths, &cells, false, on).clicked() {
            acts.push(act(Act::Select((!on).then_some(Target::Run(r)))));
        }
        match flow.artifacts.get(r) {
            Some(Ok(list)) => {
                for (a, artifact) in list.iter().enumerate() {
                    let cells = [
                        format!("    {}", artifact.name),
                        if artifact.expired { "expired" } else { "" }.to_owned(),
                        "artifact".to_owned(),
                        artifact
                            .digest
                            .clone()
                            .unwrap_or_else(|| "digest not recorded".to_owned()),
                    ];
                    let on = selected == Some(Target::Artifact(r, a));
                    if common::table_row(ui, &widths, &cells, false, on).clicked() {
                        acts.push(act(Act::Select((!on).then_some(Target::Artifact(r, a)))));
                    }
                }
            }
            Some(Err(e)) => {
                let t = Tokens::get(ui.ctx());
                ui.label(
                    RichText::new(format!("    Artifacts not read: {e}"))
                        .size(11.5)
                        .color(t.warning),
                );
            }
            None => {}
        }
    }
}

/// What the picked node is, and its links.
pub(in crate::repo::insights::deploy) fn detail(
    ui: &mut Ui,
    flow: &Flow,
    selected: Option<Target>,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let Some(target) = selected else {
        ui.add_space(6.0);
        common::empty(
            ui,
            "Pick a node or a row (Tab to it and press Enter) for its run, deployment or commit.",
        );
        return;
    };
    match target {
        Target::Commit => {
            common::heading(ui, &format!("Commit {}", short(&flow.commit)));
            if ui.button("Select the commit").clicked() {
                acts.push(Action::SelectHash(flow.commit.clone()));
            }
        }
        Target::Run(r) => {
            let Some(run) = flow.runs.get(r) else { return };
            common::heading(ui, &format!("{} #{}", run.name, run.number));
            ui.label(
                RichText::new(format!(
                    "{} · {} · a run is not a deployment",
                    run_word(run.state),
                    run.event
                ))
                .color(t.text_dim),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open in CI view").clicked() {
                    acts.push(Action::OpenCi {
                        sha: flow.commit.clone(),
                        run: Some(run.id),
                    });
                }
                if !run.url.is_empty() && ui.button("Run on the forge").clicked() {
                    acts.push(Action::OpenUrl(run.url.clone()));
                }
            });
        }
        Target::Artifact(r, a) => {
            let Some(artifact) = flow
                .artifacts
                .get(r)
                .and_then(|l| l.as_ref().ok())
                .and_then(|l| l.get(a))
            else {
                return;
            };
            common::heading(ui, &artifact.name);
            let size = artifact
                .size
                .map_or(String::new(), |s| format!(" · {s} bytes"));
            ui.label(
                RichText::new(format!(
                    "{}{size}{}",
                    artifact.digest.as_deref().unwrap_or("digest not recorded"),
                    if artifact.expired { " · expired" } else { "" }
                ))
                .color(t.text_dim),
            );
            if ui.button("Artifact on the forge").clicked() {
                acts.push(Action::OpenUrl(artifact.url.clone()));
            }
        }
        Target::Env(e) => {
            let Some(env) = flow.environments.get(e) else {
                return;
            };
            common::heading(ui, &env.name);
            ui.add(
                egui::Label::new(RichText::new(env_line(env, flow.now)).color(t.text_dim)).wrap(),
            );
            ui.horizontal_wrapped(|ui| {
                if !env.url.is_empty() && ui.button("Environment on the forge").clicked() {
                    acts.push(Action::OpenUrl(env.url.clone()));
                }
                if let Some(d) = &env.current {
                    if ui
                        .button("Deployment on the forge")
                        .on_hover_text(&d.url)
                        .clicked()
                    {
                        acts.push(Action::OpenUrl(d.url.clone()));
                    }
                    // Only a commit the repository has can be selected.
                    let local = matches!(
                        env.inclusion,
                        Inclusion::Included { .. } | Inclusion::NotIncluded
                    );
                    if local
                        && ui
                            .button(format!("Select deployed {}", short(&d.sha)))
                            .clicked()
                    {
                        acts.push(Action::SelectHash(d.sha.clone()));
                    }
                    if let Some(run) = d.run
                        && ui.button("Its run in the CI view").clicked()
                    {
                        acts.push(Action::OpenCi {
                            sha: d.sha.clone(),
                            run: Some(run),
                        });
                    }
                    if !d.environment_url.is_empty()
                        && ui
                            .button("Deployed site")
                            .on_hover_text(&d.environment_url)
                            .clicked()
                    {
                        acts.push(Action::OpenUrl(d.environment_url.clone()));
                    }
                }
            });
            for (d, _) in &env.in_flight {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} {} · {}",
                            short(&d.sha),
                            d.state().label(),
                            analysis::reported_label(d.reported.or(d.created), flow.now)
                        ))
                        .color(t.text_dim),
                    );
                    if ui.small_button("open").clicked() {
                        acts.push(Action::OpenUrl(d.url.clone()));
                    }
                });
            }
            if env
                .current
                .as_ref()
                .is_some_and(|d| d.state() == DeployState::Success)
                && matches!(env.inclusion, Inclusion::NotIncluded)
            {
                ui.label(
                    RichText::new(
                        "The deployed commit is in this repository and doesn't contain this one.",
                    )
                    .size(11.5)
                    .color(t.text_faint),
                );
            }
        }
    }
}
