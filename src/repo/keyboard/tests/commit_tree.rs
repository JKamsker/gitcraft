//! Opposite preferences must control both Remerge rendering and keyboard ordering.
use egui_kittest::{Harness, kittest::Queryable};

use crate::git::ChangedFile;
use crate::repo::{DiffKey, RepoTab};
use crate::settings::Settings;

#[test]
fn remerge_uses_commit_tree_preference_for_rendering_and_navigation() {
    for commit_tree in [false, true] {
        let ctx = egui::Context::default();
        let tab = RepoTab::new_deferred("/224/remerge-setting".into(), &ctx);
        let mut styled = false;
        let files = ["z.rs", "nested/a.rs"].map(|path| ChangedFile {
            path: path.into(),
            old_path: None,
            status: 'M',
            added: Some(1),
            removed: Some(0),
        });
        let mut h = Harness::builder().build_ui_state(
            move |ui, tab: &mut RepoTab| {
                if !styled {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    Settings {
                        files_as_tree: !commit_tree,
                        commit_files_as_tree: commit_tree,
                        ..Default::default()
                    }
                    .set(ui.ctx());
                    styled = true;
                    return;
                }
                let mut keyboard = tab.keyboard.borrow_mut();
                keyboard.path = tab.path.clone();
                keyboard.pane = Some(crate::repo::keyboard::Pane::Detail);
                keyboard.step = 1;
                keyboard.cursor = None;
                drop(keyboard);
                tab.file_list(
                    ui,
                    &mut Vec::new(),
                    "remerge-setting",
                    &files,
                    Some("hash"),
                    |path| DiffKey::Remerge {
                        hash: "hash".into(),
                        path: path.into(),
                        old_path: None,
                        conflicts: vec![],
                    },
                );
            },
            tab,
        );
        h.step();
        h.step();
        let expected = if commit_tree { "nested/a.rs" } else { "z.rs" };
        assert_eq!(
            h.state()
                .keyboard
                .borrow()
                .cursor
                .as_ref()
                .map(|(_, k)| k.path()),
            Some(expected)
        );
        // The nested folder is its own clickable tree node only in tree presentation.
        assert_eq!(
            h.query_by_label_contains("folder nested").is_some(),
            commit_tree
        );
    }
}
