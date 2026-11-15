//! Location edits are drafts until committed; slow usage reads stay out of the field.
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn location_applies_on_enter_and_focus_loss() {
    for enter in [true, false] {
        let mut h = Harness::builder().build_ui_state(
            |ui, location: &mut String| {
                super::location_field(ui, location);
                let _ = ui.button("Elsewhere");
            },
            String::new(),
        );
        h.get_by_role(egui::accesskit::Role::TextInput).focus();
        h.step();
        h.get_by_role(egui::accesskit::Role::TextInput)
            .type_text("/archive/draft");
        h.step();
        assert!(h.state().is_empty());
        if enter {
            h.key_press(egui::Key::Enter);
        } else {
            h.get_by_label("Elsewhere").click();
        }
        h.step();
        assert_eq!(h.state(), "/archive/draft");
    }
}
