//! Independently checked default, chord, disabled and invalid shortcut inputs.
use crate::keymap::{BINDINGS, Command, Stroke, label};

#[derive(Clone, Copy, Debug)]
pub(super) enum ShortcutContext {
    Default,
    Custom,
    Unbound,
    Conflicting,
}

impl ShortcutContext {
    pub(super) const ALL: [Self; 4] = [
        Self::Default,
        Self::Custom,
        Self::Unbound,
        Self::Conflicting,
    ];
}

const COMMANDS: [(Command, egui::Key); 7] = [
    (Command::Fetch, egui::Key::F6),
    (Command::Pull, egui::Key::F7),
    (Command::Push, egui::Key::F8),
    (Command::Undo, egui::Key::F9),
    (Command::Redo, egui::Key::F10),
    (Command::Timeline, egui::Key::F11),
    (Command::Stash, egui::Key::F12),
];

pub(super) fn expected(kind: ShortcutContext, command: Command) -> String {
    let strokes: Vec<_> = match kind {
        ShortcutContext::Custom => {
            let mut strokes = vec![Stroke::new(egui::Key::F6, egui::Modifiers::NONE)];
            for (found, key) in COMMANDS {
                if found == command {
                    strokes.push(Stroke::new(key, egui::Modifiers::NONE));
                }
            }
            assert_eq!(
                strokes.len(),
                2,
                "fixture must contain exactly one custom binding for {command:?}"
            );
            strokes
        }
        ShortcutContext::Unbound => Vec::new(),
        ShortcutContext::Default | ShortcutContext::Conflicting => BINDINGS
            .iter()
            .filter(|binding| binding.command == command)
            .map(|binding| Stroke::new(binding.key, binding.modifiers))
            .collect(),
    };
    strokes
        .iter()
        .map(Stroke::label)
        .collect::<Vec<_>>()
        .join(" \u{2192} ")
}

fn install(ctx: &egui::Context, kind: ShortcutContext) {
    let mut settings = crate::settings::Settings::default();
    for (command, key) in COMMANDS {
        let sequence = match kind {
            ShortcutContext::Default => continue,
            ShortcutContext::Custom => vec![
                Stroke::new(egui::Key::F6, egui::Modifiers::NONE),
                Stroke::new(key, egui::Modifiers::NONE),
            ],
            ShortcutContext::Unbound => Vec::new(),
            // Pull's plain F7 deliberately conflicts with NextChange. The entire
            // invalid map must fall back, including otherwise-valid overrides.
            ShortcutContext::Conflicting => vec![Stroke::new(key, egui::Modifiers::NONE)],
        };
        settings.key_bindings.insert(command.id().into(), sequence);
    }
    settings.put_for_test(ctx);
    for (command, _) in COMMANDS {
        assert_eq!(
            label(ctx, command),
            expected(kind, command),
            "{kind:?}: {command:?}"
        );
    }
}

pub(super) fn context(kind: ShortcutContext) -> egui::Context {
    let ctx = egui::Context::default();
    install(&ctx, kind);
    ctx
}

#[test]
fn context_changes_preserve_valid_disabled_and_fallback_shortcuts() {
    let ctx = egui::Context::default();
    for kind in [
        ShortcutContext::Custom,
        ShortcutContext::Conflicting,
        ShortcutContext::Unbound,
        ShortcutContext::Default,
        ShortcutContext::Conflicting,
        ShortcutContext::Custom,
        ShortcutContext::Default,
    ] {
        install(&ctx, kind);
    }
}
