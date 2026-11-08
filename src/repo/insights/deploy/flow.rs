//! The flow's picture (commit → runs → artifacts → environments, in columns), its legend, the
//! environments and runs as tables, and what the picked node is, with its links.

use egui::{RichText, Ui};

use super::{Act, Picture, Target, act};
use crate::ci::CheckState;
use crate::ci::deployments::DeployState;
use crate::insights::Evidence;
use crate::insights::deploy::{self as analysis, Flow, Freshness, Inclusion};
use crate::repo::insights::common;
use crate::repo::insights::graph::{self, Edge, Graph, Node, NodeShape};
use crate::repo::{Action, short};
use crate::theme::Tokens;

/// So many artifacts go into the picture; the table has them all.
const MAX_ARTIFACT_NODES: usize = 24;

fn run_badge(state: CheckState) -> &'static str {
    match state {
        CheckState::Passed => "ok",
        CheckState::Failed => "!",
        CheckState::Running => "..",
        CheckState::Skipped => "-",
    }
}

fn run_word(state: CheckState) -> &'static str {
    match state {
        CheckState::Passed => "passed",
        CheckState::Failed => "failed",
        CheckState::Running => "running",
        CheckState::Skipped => "skipped or cancelled",
    }
}

fn inclusion_badge(inclusion: &Inclusion) -> &'static str {
    match inclusion {
        Inclusion::Included { .. } => "IN",
        Inclusion::NotIncluded => "NO",
        Inclusion::Unknown(_) => "?",
        Inclusion::NoSuccess | Inclusion::NoRecord => "--",
    }
}

fn freshness_word(f: Freshness) -> &'static str {
    match f {
        Freshness::Recent => "within a day",
        Freshness::Days => "within two weeks",
        Freshness::Old => "older than two weeks",
        Freshness::Unknown => "unknown",
    }
}

/// The flow as a graph in columns, with what each node stands for.
pub(super) fn picture(flow: &Flow, ctx: &egui::Context) -> Picture {
    let t = Tokens::get(ctx);
    let mut graph = Graph::default();
    let mut targets = Vec::new();
    let mut commit = Node::new(format!("commit {}", short(&flow.commit)), t.accent);
    commit.shape = NodeShape::Pill;
    commit.tip = format!("{}\nClick for its links", flow.commit);
    graph.nodes.push(commit);
    targets.push(Target::Commit);
    let mut run_nodes = Vec::new();
    for (r, run) in flow.runs.iter().enumerate() {
        let color = match run.state {
            CheckState::Passed => t.success,
            CheckState::Failed => t.danger,
            CheckState::Running => t.warning,
            CheckState::Skipped => t.text_faint,
        };
        let mut node = Node::new(format!("{} #{}", run.name, run.number), color);
        node.shape = NodeShape::Square;
        node.column = 1;
        node.badge = Some(run_badge(run.state).to_owned());
        node.tip = format!(
            "Run {} ({}): a CI run is not a deployment",
            run_word(run.state),
            run.event
        );
        run_nodes.push(graph.nodes.len());
        let mut edge = Edge::new(0, graph.nodes.len());
        edge.directed = true;
        edge.tip = "A run of this commit, as the forge records it".to_owned();
        graph.edges.push(edge);
        graph.nodes.push(node);
        targets.push(Target::Run(r));
    }
    let mut shown = 0;
    let mut left_out = 0;
    for (r, artifacts) in flow.artifacts.iter().enumerate() {
        let Ok(artifacts) = artifacts else { continue };
        for (a, artifact) in artifacts.iter().enumerate() {
            if shown == MAX_ARTIFACT_NODES {
                left_out += 1;
                continue;
            }
            shown += 1;
            let mut node = Node::new(graph::elide(&artifact.name, 24), t.text_dim);
            node.shape = NodeShape::Diamond;
            node.column = 2;
            node.badge = Some(if artifact.digest.is_some() { "#" } else { "?" }.to_owned());
            node.faded = artifact.expired;
            node.tip = match &artifact.digest {
                Some(d) => format!("Artifact, digest {d}"),
                None => "Artifact, digest not recorded".to_owned(),
            };
            if artifact.expired {
                node.tip.push_str(" (expired)");
            }
            let mut edge = Edge::new(run_nodes[r], graph.nodes.len());
            edge.directed = true;
            edge.tip = "Uploaded by this run".to_owned();
            graph.edges.push(edge);
            graph.nodes.push(node);
            targets.push(Target::Artifact(r, a));
        }
    }
    let mut env_nodes = Vec::new();
    for (e, env) in flow.environments.iter().enumerate() {
        let color = match env.inclusion {
            Inclusion::Included { .. } => t.success,
            Inclusion::NotIncluded => t.text_dim,
            Inclusion::Unknown(_) => t.warning,
            Inclusion::NoSuccess | Inclusion::NoRecord => t.text_faint,
        };
        let mut node = Node::new(env.name.clone(), color);
        node.shape = NodeShape::Pill;
        node.column = 3;
        node.badge = Some(inclusion_badge(&env.inclusion).to_owned());
        node.evidence = env.inclusion.evidence();
        node.faded = matches!(env.inclusion, Inclusion::NoRecord);
        node.tip = env_line(env, flow.now);
        env_nodes.push(graph.nodes.len());
        if let Some(d) = &env.current {
            let mut edge = Edge::new(0, graph.nodes.len());
            edge.directed = true;
            match env.inclusion {
                Inclusion::Included { .. } => {
                    edge.color = Some(t.success);
                    edge.weight = 2.0;
                    edge.tip = format!(
                        "Deployed {}, which is or contains this commit (the forge's record and \
                         Git's history)",
                        short(&d.sha)
                    );
                    graph.edges.push(edge);
                }
                Inclusion::Unknown(_) => {
                    edge.evidence = Evidence::Unknown;
                    edge.tip = format!(
                        "Deployed {}, which isn't in this repository: whether it contains this \
                         commit is unknown",
                        short(&d.sha)
                    );
                    graph.edges.push(edge);
                }
                _ => {}
            }
        }
        graph.nodes.push(node);
        targets.push(Target::Env(e));
    }
    for (r, e, current) in flow.run_links() {
        let mut edge = Edge::new(run_nodes[r], env_nodes[e]);
        edge.directed = true;
        edge.tip = if current {
            "The environment's latest successful deployment names this run".to_owned()
        } else {
            "A queued or running deployment names this run".to_owned()
        };
        if !current {
            edge.color = Some(t.warning);
        }
        graph.edges.push(edge);
    }
    let positions = graph::arrange(&graph, graph::Arrange::Columns);
    Picture {
        graph,
        positions,
        targets,
        left_out,
    }
}

/// An environment in a line: what is known of the commit there and how fresh it is.
fn env_line(env: &analysis::EnvRow, now: i64) -> String {
    let mut text = env.inclusion.label();
    if let Some(d) = &env.current {
        text.push_str(&format!(
            "\nDeployed {} · {} ({})",
            short(&d.sha),
            analysis::reported_label(d.reported, now),
            freshness_word(analysis::freshness(d.reported, now))
        ));
    }
    if !env.in_flight.is_empty() {
        text.push_str(&format!("\n{} queued or under way", env.in_flight.len()));
    }
    if let Some(d) = &env.failed {
        text.push_str(&format!("\nA later deployment of {} failed", short(&d.sha)));
    }
    text
}

pub(super) fn legend(ui: &mut Ui) {
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
pub(super) fn caution(ui: &mut Ui, flow: &Flow) {
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
pub(super) fn environments(
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
pub(super) fn runs(ui: &mut Ui, flow: &Flow, selected: Option<Target>, acts: &mut Vec<Action>) {
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
pub(super) fn detail(ui: &mut Ui, flow: &Flow, selected: Option<Target>, acts: &mut Vec<Action>) {
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
