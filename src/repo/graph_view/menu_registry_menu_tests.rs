use super::*;
use crate::keymap::{Command, Stroke};
use egui::{Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn graph_copy_menu_hints_match_selected_commit_and_first_branch_only() {
    let mut h = Harness::new_ui_state(
        |ui, state: &mut (bool, bool, bool)| {
            if !std::mem::replace(&mut state.0, true) {
                egui_extras::install_image_loaders(ui.ctx());
                let mut settings = crate::settings::Settings::default();
                for (command, key) in [
                    (Command::GraphCopyHash, Key::F8),
                    (Command::GraphCopyMessage, Key::F9),
                    (Command::GraphCopyBranch, Key::F10),
                ] {
                    settings
                        .key_bindings
                        .insert(command.id().into(), vec![Stroke::new(key, Modifiers::NONE)]);
                }
                settings.put_for_test(ui.ctx());
            }
            copy_items(ui, "recorded", "message", Some("main"), state.1, state.2);
        },
        (false, true, true),
    );
    h.run();
    h.get_by_label("Copy commit hash F8");
    h.get_by_label("Copy message F9");
    h.get_by_label("Copy branch name (main) F10");
    h.state_mut().2 = false;
    h.run();
    h.get_by_label("Copy branch name (main)");
    h.get_by_label("Copy commit hash F8");
    h.state_mut().1 = false;
    h.run();
    h.get_by_label("Copy commit hash");
    h.get_by_label("Copy message");
}
