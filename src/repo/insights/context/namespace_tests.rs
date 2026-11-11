use super::*;
use crate::repo::insights::test_support::required;
use crate::repo::insights::{InsightsView, test_support as schedule};
use crate::repo::{Msg, messages};
use std::time::Instant;

type Reply = Box<dyn FnOnce() -> Msg + Send>;

pub(in crate::repo::insights) trait Fixture {
    fn seed(&mut self);
    fn assert_retired(&self);
}

pub(in crate::repo::insights) fn seed_slot<T>(slot: &mut crate::repo::insights::common::Slot<T>) {
    let request = slot.start();
    slot.accept(request, Err("previous repository data".into()));
    slot.start();
}

pub(in crate::repo::insights) fn assert_slot_retired<T>(
    slot: &crate::repo::insights::common::Slot<T>,
) {
    assert!(!slot.loading && slot.value.is_none());
}

fn fixture(view: &mut InsightsView, kind: Kind) -> &mut dyn Fixture {
    match kind {
        Kind::Impact => &mut view.impact,
        Kind::CiPath => &mut view.ci_path,
        Kind::Constellation => &mut view.constellation,
        Kind::Evolution => &mut view.evolution,
        Kind::Coupling => &mut view.coupling,
        Kind::ConflictRadar => &mut view.conflict_radar,
        Kind::ReviewHeat => &mut view.review_heat,
        Kind::Transform => &mut view.transform,
        Kind::Deploy => &mut view.deploy,
        Kind::Ribbons => &mut view.ribbons,
        Kind::Coverage => &mut view.coverage,
        Kind::Churn => &mut view.churn,
    }
}

fn queued(tab: &RepoTab, ctx: &egui::Context, error: bool) -> Reply {
    let (_, mut jobs) = messages::record_jobs(|| {
        tab.insight_then(
            ctx,
            move || {
                if error {
                    Err("old error")
                } else {
                    Ok("old success")
                }
            },
            |tab, result, _| {
                tab.status = Some(crate::repo::Notice::plain((format!("{result:?}"), false)));
            },
        );
    });
    assert_eq!(jobs.len(), 1);
    required(jobs.pop())
}

fn change_namespace(tab: &mut RepoTab, ctx: &egui::Context, how: &str) {
    messages::record_jobs(|| match how {
        "route" | "route-aba" => {
            let original = tab.connection_context.clone();
            let other = crate::connections::Context {
                git_config: Some("/recording-only/other-ssh-route".into()),
                ..Default::default()
            };
            other.run(|| tab.notice_connections(ctx));
            if how == "route-aba" {
                original.run(|| tab.notice_connections(ctx));
            }
        }
        "path" => {
            tab.path = "/recording-only/other-repository".into();
            tab.sync_insights_context(ctx);
        }
        "suspend" => {
            tab.life
                .shown(Instant::now() - crate::repo::lifecycle::IDLE_AFTER);
            tab.background_tick();
            assert!(tab.sleeping());
        }
        "signin" => tab.invalidate_https_challenge(),
        _ => unreachable!(),
    });
}

fn assert_retired(tab: &mut RepoTab, kind: Kind) {
    let view = required(tab.insights.as_mut());
    assert!(view.loaded.is_empty());
    assert_eq!(view.kind, kind);
    assert!(view.table);
    fixture(view, kind).assert_retired();
}

fn deliver(tab: &mut RepoTab, ctx: &egui::Context, job: Reply) {
    let connection = tab.connection_context.clone();
    connection.run(|| {
        messages::record_jobs(|| tab.handle(job(), ctx));
    });
}

#[test]
fn all_insights_retire_repository_slots_without_losing_page_or_safe_preferences() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for kind in Kind::ALL {
        for transition in ["route", "route-aba", "path", "suspend", "signin"] {
            for error in [false, true] {
                let ctx = egui::Context::default();
                let mut tab = schedule::tab(&ctx, kind);
                let view = required(tab.insights.as_mut());
                view.table = true;
                fixture(view, kind).seed();
                let old = queued(&tab, &ctx, error);
                change_namespace(&mut tab, &ctx, transition);
                assert_retired(&mut tab, kind);
                deliver(&mut tab, &ctx, old);
                assert!(tab.status.is_none(), "{kind:?} {transition}");
                assert_retired(&mut tab, kind);
                if transition == "suspend" {
                    messages::record_jobs(|| tab.shown_now(&ctx));
                    assert!(!tab.sleeping());
                }
                // A fresh namespace read still reaches its completion.
                let connection = tab.connection_context.clone();
                let fresh = connection.run(|| queued(&tab, &ctx, false));
                deliver(&mut tab, &ctx, fresh);
                assert!(tab.status.is_some(), "{kind:?} {transition}");
            }
        }
    }
}

#[test]
fn abandoned_first_stages_cannot_start_build_or_layout_work_in_another_namespace() {
    for kind in [Kind::Churn, Kind::Coverage] {
        for complete_first in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = schedule::tab(&ctx, kind);
            let (_, mut first) = messages::record_jobs(|| {
                tab.insight_then(
                    &ctx,
                    || (),
                    |tab, (), ctx| {
                        tab.insight_then(
                            ctx,
                            || (),
                            |tab, (), _| {
                                tab.status =
                                    Some(crate::repo::Notice::plain(("old layout".into(), false)));
                            },
                        );
                    },
                );
            });
            if complete_first {
                let (_, next) = messages::record_jobs(|| tab.handle(required(first.pop())(), &ctx));
                assert_eq!(next.len(), 1);
                first = next;
            }
            change_namespace(&mut tab, &ctx, "route");
            let connection = tab.connection_context.clone();
            let (_, later) = connection
                .run(|| messages::record_jobs(|| tab.handle(required(first.pop())(), &ctx)));
            assert!(
                later.is_empty(),
                "{kind:?} must discard before the second stage"
            );
            assert!(tab.status.is_none());
        }
    }
}

#[test]
fn close_and_reopen_never_reuses_a_repository_read_lifetime() {
    use crate::repo::Action;
    use crate::repo::insights::InsightsAct;
    for kind in Kind::ALL {
        for error in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = schedule::tab(&ctx, kind);
            let old = queued(&tab, &ctx, error);
            messages::record_jobs(|| {
                tab.apply(Action::Insights(InsightsAct::Close), &ctx);
                tab.apply(Action::Insights(InsightsAct::Open(kind)), &ctx);
            });
            deliver(&mut tab, &ctx, old);
            assert!(tab.status.is_none());
            let fresh = queued(&tab, &ctx, false);
            deliver(&mut tab, &ctx, fresh);
            assert!(tab.status.is_some());
        }
    }
}

#[test]
fn forge_only_changes_preserve_local_slots_and_their_valid_completions() {
    for kind in [Kind::Churn, Kind::Coverage, Kind::Evolution, Kind::Coupling] {
        let ctx = egui::Context::default();
        let mut tab = schedule::tab(&ctx, kind);
        let old = queued(&tab, &ctx, false);
        schedule::transition(&mut tab, &ctx, "forge");
        assert!(required(tab.insights.as_ref()).loaded.contains(&kind));
        deliver(&mut tab, &ctx, old);
        assert!(tab.status.is_some());
    }
}
