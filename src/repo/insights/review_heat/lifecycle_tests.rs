use super::*;
use crate::repo::insights::test_support::required;
use crate::repo::insights::{Kind, test_support as schedule};

#[test]
fn context_changes_forget_the_previous_forges_review_and_opened_pr() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for transition in schedule::TRANSITIONS {
        for error in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = schedule::tab(&ctx, Kind::ReviewHeat);
            let old = {
                let view = required(tab.insights.as_mut());
                view.pr = Some(7);
                let state = &mut view.review_heat;
                state.number = Some(7);
                state.selected = Some((0, 0));
                state.expanded.insert(0);
                let current = state.data.start();
                state.data.accept(current, Ok(Heat::default()));
                state.data.start()
            };
            let result = if error {
                Err("old review error".into())
            } else {
                Ok(Heat::default())
            };
            let old = schedule::reply(&tab, &ctx, result, move |tab, result, _| {
                tab.review_heat_reply(old, result);
            });
            schedule::transition(&mut tab, &ctx, transition);
            old(&mut tab, &ctx);
            let view = required(tab.insights.as_ref());
            assert!(!view.loaded.contains(&Kind::ReviewHeat));
            assert!(view.pr.is_none());
            let state = &view.review_heat;
            assert!(state.number.is_none() && state.selected.is_none());
            assert!(state.expanded.is_empty());
            assert!(!state.data.loading && state.data.value.is_none());
            // The new context's same-number PR can start and finish independently.
            let current = required(tab.insights.as_mut()).review_heat.data.start();
            let fresh = schedule::reply(&tab, &ctx, Ok(Heat::default()), move |tab, result, _| {
                tab.review_heat_reply(current, result);
            });
            fresh(&mut tab, &ctx);
            assert!(
                required(tab.insights.as_ref())
                    .review_heat
                    .data
                    .ready()
                    .is_some()
            );
        }
    }
}
