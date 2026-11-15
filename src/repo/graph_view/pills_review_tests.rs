use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn another_remotes_same_name_pill_checks_out_from_primary_and_overflow() {
    for overflow in [false, true] {
        let refs = Refs {
            current: Some("topic".into()),
            local: vec![Branch {
                name: "topic".into(),
                target: "tip".into(),
                is_head: true,
                upstream: "origin/topic".into(),
                track: "[behind 2]".into(),
            }],
            ..Default::default()
        };
        let mut h = Harness::builder().with_step_dt(0.01).build_ui_state(
            move |ui, (tab, acts): &mut (RepoTab, Vec<Action>)| {
                let resp = ui.button("alice/topic");
                let badge = remote_pill("topic", "alice");
                if overflow {
                    tab.stack_slot_clicks(resp, &badge, 0, acts);
                } else {
                    let rect = resp.rect;
                    tab.pill_clicks(ui, &resp, rect, &badge, "tip", acts);
                }
            },
            (tab(refs), Vec::new()),
        );
        h.run();
        h.get_by_label_contains("alice/topic").click();
        h.step();
        h.get_by_label_contains("alice/topic").click();
        h.step();
        assert!(
            h.state()
                .1
                .iter()
                .any(|a| matches!(a, Action::CheckoutRemote(name) if name == "alice/topic")),
            "overflow={overflow}"
        );
        assert!(!h.state().1.iter().any(|a| matches!(a, Action::Pull)));
    }
}
