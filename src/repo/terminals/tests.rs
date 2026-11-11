//! Inert transition tests exercise the production collection logic without PTYs.

use super::{Terminals, command_items, new_palette_item, toggle_palette_item};
use crate::repo::palette::PaletteRun;
use crate::repo::{Action, terminals::TerminalAct};
use std::cell::Cell;
use std::rc::Rc;

fn opened() -> Terminals<&'static str> {
    let mut panel = Terminals::default();
    for shell in ["first", "second", "third"] {
        panel.opened(Ok(shell));
    }
    panel
}

#[test]
fn closing_preserves_the_selected_shell_or_selects_its_neighbour() {
    let mut panel = opened();
    panel.close(0);
    assert_eq!(panel.shells(), ["second", "third"]);
    assert_eq!(panel.active(), Some(&"third"));
    panel.close(1);
    assert_eq!(panel.active(), Some(&"second"));
    assert!(panel.is_open());
    panel.close(0);
    assert!(panel.is_empty());
    assert_eq!(panel.active(), None);
    assert!(!panel.is_open());

    let mut panel = opened();
    panel.select(0);
    panel.close(2);
    assert_eq!(panel.active(), Some(&"first"));
    panel.close(0);
    assert_eq!(panel.active(), Some(&"second"));
}

#[test]
fn stale_indices_do_not_break_selection_or_drop_a_shell() {
    let mut panel = opened();
    panel.select(1);
    panel.select(usize::MAX);
    panel.close(usize::MAX);
    assert_eq!(panel.active(), Some(&"second"));
    assert_eq!(panel.shells(), ["first", "second", "third"]);
    let mut empty = Terminals::<()>::default();
    empty.select(usize::MAX);
    empty.close(usize::MAX);
    assert_eq!(empty.active(), None);
    assert!(!empty.is_open());
}

#[test]
fn failed_launch_keeps_existing_shells_and_success_clears_the_error() {
    let mut panel = opened();
    panel.select(0);
    panel.hide();
    panel.opened(Err("launch refused".into()));
    assert!(panel.is_open());
    assert_eq!(panel.error(), Some("launch refused"));
    assert_eq!(panel.active(), Some(&"first"));
    assert_eq!(panel.len(), 3);
    panel.opened(Ok("fourth"));
    assert_eq!(panel.active(), Some(&"fourth"));
    assert_eq!(panel.error(), None);

    let mut empty = Terminals::<()>::default();
    empty.opened(Err("no shell installed".into()));
    assert!(empty.is_open());
    assert_eq!(empty.active(), None);
    assert_eq!(empty.error(), Some("no shell installed"));
}

#[test]
fn hiding_and_failed_launches_preserve_owned_sessions_until_close_or_drop() {
    struct Session(Rc<Cell<usize>>);
    impl Drop for Session {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let ended = Rc::new(Cell::new(0));
    {
        let mut panel = Terminals::default();
        panel.opened(Ok(Session(ended.clone())));
        panel.opened(Ok(Session(ended.clone())));
        panel.hide();
        panel.opened(Err("failed".into()));
        assert_eq!(ended.get(), 0);
        panel.close(0);
        assert_eq!(ended.get(), 1);
        assert_eq!(panel.len(), 1);
    }
    assert_eq!(ended.get(), 2);
}

#[test]
fn repeated_hide_actions_do_not_reopen_or_start_a_shell() {
    let ctx = egui::Context::default();
    let mut panel = Terminals::default();
    panel.opened(Err("no shell installed".into()));
    for _ in 0..2 {
        panel.apply(TerminalAct::Hide, std::path::Path::new("/unused"), &ctx);
        assert!(!panel.is_open());
        assert!(panel.is_empty());
        assert_eq!(panel.error(), Some("no shell installed"));
    }
}

#[test]
fn terminal_palette_labels_hints_and_routes_stay_compatible() {
    let ctx = egui::Context::default();
    let (label, hint, action) = toggle_palette_item(&ctx);
    assert_eq!(label, "Toggle terminal");
    assert_eq!(
        hint,
        crate::keymap::label(&ctx, crate::keymap::Command::Terminal)
    );
    assert!(matches!(action, Action::Terminal(TerminalAct::Toggle)));
    let (label, hint, action) = new_palette_item();
    assert_eq!(
        (label.as_str(), hint.as_str()),
        ("New terminal", "terminal")
    );
    assert!(matches!(
        action,
        Action::Terminal(TerminalAct::New {
            shell: None,
            cwd: None
        })
    ));
    let [toggle, new] = command_items();
    assert_eq!(toggle.label, "Toggle terminal");
    assert!(matches!(
        toggle.run,
        PaletteRun::Action(Action::Terminal(TerminalAct::Toggle))
    ));
    assert_eq!(new.label, "New terminal");
    assert!(matches!(
        new.run,
        PaletteRun::Action(Action::Terminal(TerminalAct::New {
            shell: None,
            cwd: None
        }))
    ));
}
