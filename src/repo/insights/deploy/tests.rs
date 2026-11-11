//! The view in a tab: unavailable without a forge, and a flow read before drawn as a picture
//! and as tables, with its picks and Esc.

use super::*;
use crate::ci::deployments::{Artifact, DeployState, Deployment, Deployments, Environment};
use crate::ci::{CheckState, Project, Run};
use crate::insights::Provenance;
use crate::insights::deploy::environments;
use crate::repo::insights::{InsightsAct, Kind};

fn draw(tab: &mut RepoTab, ctx: &egui::Context) -> Vec<Action> {
    let mut acts = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.insights_ui(ui, &mut acts));
    })
    .drop_without_applying_deltas();
    acts
}

fn flow() -> Flow {
    let commit = "a".repeat(40);
    let read = Deployments {
        environments: ["staging", "production", "preview"]
            .map(|n| Environment {
                name: n.to_owned(),
                url: format!("https://github.com/o/r/deployments/{n}"),
            })
            .to_vec(),
        deployments: vec![
            Deployment {
                id: 2,
                environment: "production".to_owned(),
                sha: "b".repeat(40),
                state: Some(DeployState::Queued),
                run: Some(7),
                ..Deployment::default()
            },
            Deployment {
                id: 1,
                environment: "staging".to_owned(),
                sha: commit.clone(),
                state: Some(DeployState::Success),
                reported: Some(100),
                run: Some(7),
                url: "https://github.com/o/r/actions/runs/7/job/1".to_owned(),
                ..Deployment::default()
            },
        ],
        partial: Vec::new(),
    };
    let environments = environments(&read, &commit, |_| None);
    Flow {
        commit: commit.clone(),
        project: Project {
            forge: crate::forge::Forge::GitHub,
            host: "github.com".to_owned(),
            path: "o/r".to_owned(),
        },
        runs: vec![Run {
            id: 7,
            name: "CI".to_owned(),
            number: 3,
            state: CheckState::Passed,
            sha: commit,
            ..Run::default()
        }],
        artifacts: vec![Ok(vec![
            Artifact {
                id: 1,
                name: "dist".to_owned(),
                run: 7,
                digest: Some("sha256:00ff".to_owned()),
                ..Artifact::default()
            },
            Artifact {
                id: 2,
                name: "logs".to_owned(),
                run: 7,
                ..Artifact::default()
            },
        ])],
        environments,
        notes: vec!["A note".to_owned()],
        now: 1000,
        provenance: Provenance::new("GitHub Deployments API (gh)", "o/r").tip("HEAD", "x"),
    }
}

#[test]
fn without_a_forge_the_view_says_what_it_needs() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/deploy-view".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Deploy)), &ctx);
    assert!(
        tab.insights
            .as_ref()
            .is_some_and(|v| v.deploy.unavailable.is_some())
    );
    draw(&mut tab, &ctx);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx);
}

#[test]
fn a_flow_draws_as_a_picture_and_as_tables_and_esc_unpicks_first() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/deploy-flow".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::Deploy)), &ctx);
    if let Some(state) = tab.insights.as_mut().map(|v| &mut v.deploy) {
        state.unavailable = None;
        state.subject = Some(("HEAD".to_owned(), "x".to_owned()));
        let request = state.flow.start();
        state.flow.accept(request, Ok(flow()));
        state.picture = state.flow.ready().map(|f| flow::picture(f, &ctx));
    }
    let picture = tab
        .insights
        .as_ref()
        .and_then(|v| v.deploy.picture.as_ref());
    // Commit, run, two artifacts, three environments.
    assert_eq!(picture.map(|p| p.graph.nodes.len()), Some(7));
    // Commit → run, run → artifacts, commit → staging (included), run → staging and
    // production: no edge to an environment from the run's success alone.
    assert_eq!(picture.map(|p| p.graph.edges.len()), Some(6));
    for target in [
        Target::Env(0),
        Target::Env(1),
        Target::Run(0),
        Target::Artifact(0, 0),
        Target::Commit,
    ] {
        tab.apply(
            Action::Insights(InsightsAct::Deploy(Act::Select(Some(target)))),
            &ctx,
        );
        draw(&mut tab, &ctx);
    }
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx);
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(
        tab.insights
            .as_ref()
            .is_some_and(|v| v.deploy.selected.is_none())
    );
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_none());
}

#[test]
fn only_current_deployment_completion_changes_availability_and_picture() {
    for error in [
        "gh isn't installed",
        "gh isn't signed in",
        "run gh auth login",
    ] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/deploy-completions".into(), &ctx);
        tab.insights = Some(Box::new(super::super::InsightsView::new(Kind::Deploy)));
        let initial = tab.insights.as_mut().unwrap().deploy.flow.start();
        tab.deploy_read(initial, Ok(flow()), &ctx);
        let (old, current) = {
            let state = &mut tab.insights.as_mut().unwrap().deploy;
            state.selected = Some(Target::Run(0));
            (state.flow.start(), state.flow.start())
        };
        // A rejected error preserves loading, data and the current presentation.
        tab.deploy_read(old, Err(error.into()), &ctx);
        let state = &tab.insights.as_ref().unwrap().deploy;
        assert!(state.flow.loading);
        assert!(state.flow.ready().is_some());
        assert!(state.unavailable.is_none());
        assert_eq!(state.selected, Some(Target::Run(0)));
        assert_eq!(state.picture.as_ref().map(|p| p.graph.nodes.len()), Some(7));

        tab.deploy_read(current, Ok(flow()), &ctx);
        tab.deploy_read(old, Err(error.into()), &ctx);
        let state = &tab.insights.as_ref().unwrap().deploy;
        assert!(!state.flow.loading);
        assert!(state.flow.ready().is_some());
        assert!(state.unavailable.is_none());
        assert_eq!(state.selected, Some(Target::Run(0)));
        assert_eq!(state.picture.as_ref().map(|p| p.graph.nodes.len()), Some(7));

        // A stale success must also preserve a current tool error.
        let current = tab.insights.as_mut().unwrap().deploy.flow.start();
        tab.deploy_read(current, Err(error.into()), &ctx);
        tab.deploy_read(old, Ok(flow()), &ctx);
        let state = &tab.insights.as_ref().unwrap().deploy;
        assert!(!state.flow.loading);
        assert_eq!(state.flow.error(), Some(error));
        assert_eq!(
            state.unavailable.as_ref().map(|v| v.1.as_str()),
            Some(error)
        );

        let current = tab.insights.as_mut().unwrap().deploy.flow.start();
        tab.deploy_read(current, Ok(flow()), &ctx);
        let state = &tab.insights.as_ref().unwrap().deploy;
        assert!(state.unavailable.is_none());
        assert!(state.flow.ready().is_some());
        assert_eq!(state.picture.as_ref().map(|p| p.graph.nodes.len()), Some(7));
    }
}
