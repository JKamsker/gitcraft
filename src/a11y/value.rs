//! Values and assistive-technology actions for painted scalar controls.
use egui::{
    Response,
    accesskit::{Action, ActionData, Orientation},
};

/// Publish the same bounded value that the control uses, and handle AccessKit's
/// increment/decrement/set requests through its ordinary caller-owned state.
pub fn slider(
    response: &Response,
    label: impl super::Label,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    vertical: bool,
) -> Option<f32> {
    let min = *range.start();
    let max = *range.end();
    let mut next = value.clamp(min, max);
    if response.enabled() {
        response.ctx.input(|input| {
            next += (input.num_accesskit_action_requests(response.id, Action::Increment) as f32
                - input.num_accesskit_action_requests(response.id, Action::Decrement) as f32)
                * step;
            for request in input.accesskit_action_requests(response.id, Action::SetValue) {
                if let Some(ActionData::NumericValue(value)) = request.data
                    && value.is_finite()
                {
                    next = value as f32;
                }
            }
        });
        next = next.clamp(min, max);
    }
    super::a11y(response, egui::WidgetType::Slider, label);
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_numeric_value(f64::from(next));
        node.set_min_numeric_value(f64::from(min));
        node.set_max_numeric_value(f64::from(max));
        node.set_numeric_value_step(f64::from(step));
        node.set_orientation(if vertical {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        });
        if response.enabled() && max > min {
            node.add_action(Action::SetValue);
            if next < max {
                node.add_action(Action::Increment);
            }
            if next > min {
                node.add_action(Action::Decrement);
            }
        }
    });
    (response.enabled() && next != value).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    struct State {
        value: f32,
        id: egui::Id,
        enabled: bool,
    }

    #[test]
    fn painted_range_exposes_state_and_responds_to_real_accesskit_requests() {
        let mut h = Harness::new_ui_state(
            |ui, state: &mut State| {
                ui.add_enabled_ui(state.enabled, |ui| {
                    let response = ui
                        .allocate_response(egui::vec2(30.0, 180.0), egui::Sense::click_and_drag());
                    state.id = response.id;
                    if let Some(value) = slider(
                        &response,
                        "History viewport position",
                        state.value,
                        0.0..=100.0,
                        10.0,
                        true,
                    ) {
                        state.value = value;
                    }
                });
                super::super::end_frame(ui.ctx());
            },
            State {
                value: 40.0,
                id: egui::Id::NULL,
                enabled: true,
            },
        );
        h.run();
        let node =
            h.get_by_role_and_label(egui::accesskit::Role::Slider, "History viewport position");
        let data = node.accesskit_node().data();
        assert_eq!(data.numeric_value(), Some(40.0));
        assert_eq!(data.min_numeric_value(), Some(0.0));
        assert_eq!(data.max_numeric_value(), Some(100.0));
        assert_eq!(data.numeric_value_step(), Some(10.0));
        assert_eq!(data.orientation(), Some(Orientation::Vertical));
        assert!(data.supports_action(Action::Increment));
        assert!(data.supports_action(Action::Decrement));
        let request = |action, data| {
            egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: h.state().id.accesskit_id(),
                data,
            })
        };
        h.event(request(Action::Increment, None));
        h.run();
        assert_eq!(h.state().value, 50.0);
        h.event(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: Action::SetValue,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: h.state().id.accesskit_id(),
                data: Some(ActionData::NumericValue(200.0)),
            },
        ));
        h.run();
        assert_eq!(h.state().value, 100.0);
        h.state_mut().enabled = false;
        h.event(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: Action::Decrement,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: h.state().id.accesskit_id(),
                data: None,
            },
        ));
        h.run();
        assert_eq!(h.state().value, 100.0);
        assert!(
            h.get_by_label("History viewport position")
                .accesskit_node()
                .is_disabled()
        );
    }
}
