//! Inert completion schedules: workers are recorded, and only synthetic work runs.
use super::{InsightsView, Kind};
use crate::forge::{Forge, Hosting};
use crate::git::Remote;
use crate::repo::{RepoTab, messages};

pub(super) const TRANSITIONS: [&str; 6] = [
    "forge",
    "forge-aba",
    "other-tab-aba",
    "account",
    "account-aba",
    "signin",
];

pub(super) fn tab(ctx: &egui::Context, kind: Kind) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/insights-context".into(), ctx);
    for (name, owner) in [("origin", "a"), ("fork", "b")] {
        tab.remotes.push(Remote {
            name: name.into(),
            url: format!("https://github.com/{owner}/repo"),
        });
        tab.forge_remotes.push((
            name.into(),
            Hosting {
                forge: Forge::GitHub,
                host: "github.com".into(),
                path: format!("{owner}/repo"),
                web: "https://github.com".into(),
            },
        ));
    }
    tab.take_forge_selection(false, ctx);
    tab.refs.head = "a".repeat(40);
    tab.insights = Some(Box::new(InsightsView::new(kind)));
    tab.insights.as_mut().unwrap().loaded.insert(kind);
    tab.sync_insights_context(ctx);
    tab
}

pub(super) fn transition(tab: &mut RepoTab, ctx: &egui::Context, how: &str) {
    messages::record_jobs(|| match how {
        "forge" => tab.set_primary_forge(Some("fork".into()), ctx),
        "forge-aba" => {
            tab.set_primary_forge(Some("fork".into()), ctx);
            tab.set_primary_forge(Some("origin".into()), ctx);
        }
        "other-tab-aba" => {
            let mut other = self::tab(ctx, Kind::Evolution);
            other.set_primary_forge(Some("fork".into()), ctx);
            other.set_primary_forge(None, ctx);
            tab.poll(ctx);
        }
        "account" | "account-aba" => {
            let original = crate::connections::current();
            let other = crate::connections::Context {
                error: Some("different account/transport".into()),
                ..Default::default()
            };
            other.run(|| tab.notice_connections(ctx));
            if how == "account-aba" {
                original.run(|| tab.notice_connections(ctx));
            }
        }
        "signin" => tab.invalidate_https_challenge(),
        _ => panic!("unknown context transition"),
    });
    tab.sync_insights_context(ctx);
}

/// Exercise the real forge+shared-clock completion wrapper without invoking Git/forge tools.
pub(super) fn reply<T: Send + 'static>(
    tab: &RepoTab,
    ctx: &egui::Context,
    value: T,
    complete: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
) -> Box<dyn FnOnce(&mut RepoTab, &egui::Context)> {
    let sender = tab.forge_sender();
    let (_, mut jobs) = tab
        .connection_context
        .run(|| messages::record_jobs(|| tab.insight_forge_then(ctx, move || value, complete)));
    assert_eq!(jobs.len(), 1);
    let job = jobs.pop().unwrap();
    Box::new(move |tab, ctx| {
        let connection = tab.connection_context.clone();
        connection.run(|| {
            sender.send(job()).unwrap();
            messages::record_jobs(|| tab.poll(ctx));
        });
    })
}

pub(super) fn reload_from_frame(tab: &mut RepoTab, ctx: &egui::Context) {
    crate::theme::install_fonts(ctx);
    let connection = tab.connection_context.clone();
    let (_, jobs) =
        connection.run(|| messages::record_jobs(|| super::tests::draw(tab, ctx, 900.0)));
    assert_eq!(jobs.len(), 1, "visible invalidated page must reload once");
    let view = tab.insights.as_ref().unwrap();
    assert!(view.loaded.contains(&view.kind));
}
