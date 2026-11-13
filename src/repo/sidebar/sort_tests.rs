//! The branch sort orders in the sidebar header (#213).

/// The sort orders are icons (#213): each named for screen readers, the one in use selected,
/// and clicking the other switches and keeps it.
#[test]
fn the_branch_sort_icons_are_named_selected_and_kept() {
    use egui_kittest::kittest::{NodeT, Queryable};
    let ctx = egui::Context::default();
    let tab = crate::repo::RepoTab::new_deferred("/nonexistent/sort-icons".into(), &ctx);
    let mut styled = false;
    let mut h = egui_kittest::Harness::builder()
        .with_size([320.0, 200.0])
        .build_ui_state(
            move |ui, (tab, acts): &mut (crate::repo::RepoTab, Vec<crate::repo::Action>)| {
                if !styled {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    styled = true;
                    return;
                }
                egui::CentralPanel::default().show(ui, |ui| tab.branch_header_buttons(ui, acts));
            },
            (tab, Vec::new()),
        );
    h.step();
    h.step();
    let selected = |h: &egui_kittest::Harness<'_, _>, name: &str| {
        h.get_by_label(name).accesskit_node().data().toggled()
            == Some(egui::accesskit::Toggled::True)
    };
    assert!(selected(&h, "Name") && !selected(&h, "Recent"));
    // Not the words any more: icons of a usable size, side by side.
    let (name, recent) = (
        h.get_by_label("Name").rect(),
        h.get_by_label("Recent").rect(),
    );
    assert!((20.0..=24.0).contains(&name.width()) && !name.intersects(recent));
    h.get_by_label("Recent").click();
    h.step();
    h.step();
    assert!(selected(&h, "Recent") && !selected(&h, "Name"));
    let key = h.state().0.repo_key().to_path_buf();
    assert!(crate::repo::branch_order::BranchOrder::load(&h.ctx, &key).by_date);
}
