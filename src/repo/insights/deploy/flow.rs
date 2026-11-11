//! The flow's picture (commit → runs → artifacts → environments, in columns), its legend, the
//! environments and runs as tables, and what the picked node is, with its links.

mod presentation;
pub(super) use presentation::{caution, detail, environments, legend, runs};

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
