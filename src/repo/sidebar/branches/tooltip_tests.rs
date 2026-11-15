use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn tracking_tooltip_remains_open_when_the_pointer_moves_from_row_to_action() {
    let mut tab = RepoTab::new_deferred(
        "/nonexistent/tracking-tooltip".into(),
        &egui::Context::default(),
    );
    tab.refs.local.push(Branch {
        name: "topic".into(),
        target: "a".repeat(40),
        is_head: true,
        upstream: String::new(),
        track: String::new(),
    });
    let mut h = Harness::builder().with_size([600.0, 400.0]).build_ui_state(
        |ui, state: &mut (bool, RepoTab, Vec<Action>)| {
            if !std::mem::replace(&mut state.0, true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            ui.style_mut().interaction.tooltip_delay = 0.0;
            ui.style_mut().interaction.show_tooltips_only_when_still = false;
            let b = &state.1.refs.local[0];
            state.1.local_branch_row(
                ui,
                &mut state.2,
                b,
                LeafRow {
                    name: &b.name,
                    indent: 0.0,
                },
                true,
                30,
                false,
            );
        },
        (false, tab, Vec::new()),
    );
    h.run();
    h.get_by_label(&labels::branch("topic", true, 0, 0)).hover();
    h.run_steps(5);
    let set = crate::i18n::text("tracking-set");
    h.get_by_label(set).hover();
    h.run_steps(5);
    h.get_by_label(set).click();
    h.step();
    assert!(matches!(
        h.state().2.as_slice(),
        [Action::Dialog(Dialog::Tracking(_))]
    ));
}
