use super::*;
use crate::repo::insights::{Kind, test_support as schedule};

fn assert_retired(tab: &RepoTab) {
    let view = tab.insights.as_ref().unwrap();
    let state = &view.deploy;
    assert!(!view.loaded.contains(&Kind::Deploy));
    assert!(state.subject.is_none() && state.unavailable.is_none());
    assert!(!state.flow.loading && state.flow.value.is_none());
    assert!(state.picture.is_none() && state.selected.is_none());
}

#[test]
fn context_switches_retire_deployment_data_and_reload_the_visible_page() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for transition in schedule::TRANSITIONS {
        for error in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = schedule::tab(&ctx, Kind::Deploy);
            let initial = tab.insights.as_mut().unwrap().deploy.flow.start();
            tab.deploy_read(initial, Ok(tests::flow()), &ctx);
            let old = tab.insights.as_mut().unwrap().deploy.flow.start();
            let result = if error {
                Err("gh auth login".into())
            } else {
                Ok(tests::flow())
            };
            let old = schedule::reply(&tab, &ctx, result, move |tab, result, ctx| {
                tab.deploy_read(old, result, ctx);
            });
            schedule::transition(&mut tab, &ctx, transition);
            assert_retired(&tab);
            old(&mut tab, &ctx);
            assert_retired(&tab);
            schedule::reload_from_frame(&mut tab, &ctx);
            assert!(tab.insights.as_ref().unwrap().deploy.flow.loading);
            let current = tab.insights.as_mut().unwrap().deploy.flow.start();
            let fresh = schedule::reply(&tab, &ctx, Ok(tests::flow()), move |tab, result, ctx| {
                tab.deploy_read(current, result, ctx);
            });
            fresh(&mut tab, &ctx);
            let state = &tab.insights.as_ref().unwrap().deploy;
            assert!(state.flow.ready().is_some() && !state.flow.loading);
            assert!(state.picture.is_some());
        }
    }
}

#[test]
fn unavailable_deployment_subjects_retire_old_flow_and_picture() {
    for missing in ["commit", "forge", "project"] {
        let ctx = egui::Context::default();
        let mut tab = schedule::tab(&ctx, Kind::Deploy);
        let initial = tab.insights.as_mut().unwrap().deploy.flow.start();
        tab.deploy_read(initial, Ok(tests::flow()), &ctx);
        let old = tab.insights.as_mut().unwrap().deploy.flow.start();
        match missing {
            "commit" => tab.refs.head.clear(),
            "forge" => tab.hosting = None,
            "project" => tab.remotes.clear(),
            _ => unreachable!(),
        }
        crate::repo::messages::record_jobs(|| tab.deploy_load(&ctx));
        tab.deploy_read(old, Ok(tests::flow()), &ctx);
        let state = &tab.insights.as_ref().unwrap().deploy;
        assert!(state.unavailable.is_some());
        assert!(!state.flow.loading && state.flow.value.is_none());
        assert!(state.picture.is_none() && state.selected.is_none());
    }
}
