//! The keys of the window's and of a repository tab's main actions: one list that the key
//! handlers, the manual, Settings → Keyboard shortcuts, the toolbar's tooltips, the palette and
//! the menus all read, so none of them can say another key than the one that works.

mod config;
pub(crate) mod editor;
mod resolver;
mod selection;
pub(crate) use selection::copy_requested;

pub(crate) use config::{Overrides, Stroke};
use egui::{Key, Modifiers};
pub(crate) use resolver::{
    active, cancel, owner, pressed, queued, restrict, retain_allowed, take, which_key,
};

mod commands;
pub(crate) use commands::*;

/// A key and the modifiers held with it. `Modifiers::COMMAND` is Ctrl, or Cmd on macOS;
/// `Modifiers::CTRL` is Ctrl everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub command: Command,
    pub modifiers: Modifiers,
    pub key: Key,
}

const fn bind(command: Command, modifiers: Modifiers, key: Key) -> Binding {
    Binding {
        command,
        modifiers,
        key,
    }
}

const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);
const CTRL: Modifiers = Modifiers::CTRL;
const CTRL_SHIFT: Modifiers = Modifiers::CTRL.plus(Modifiers::SHIFT);
const NONE: Modifiers = Modifiers::NONE;

/// Every key: one per command.
pub(crate) const BINDINGS: &[Binding] = &[
    bind(Command::NewTab, CMD, Key::T),
    bind(Command::CloseTab, CMD, Key::W),
    bind(Command::ReopenTab, CMD_SHIFT, Key::T),
    // Ctrl, also on macOS, as browsers and editors switch tabs there.
    bind(Command::NextTab, CTRL, Key::Tab),
    bind(Command::PreviousTab, CTRL_SHIFT, Key::Tab),
    bind(Command::GoToTab(1), CMD, Key::Num1),
    bind(Command::GoToTab(2), CMD, Key::Num2),
    bind(Command::GoToTab(3), CMD, Key::Num3),
    bind(Command::GoToTab(4), CMD, Key::Num4),
    bind(Command::GoToTab(5), CMD, Key::Num5),
    bind(Command::GoToTab(6), CMD, Key::Num6),
    bind(Command::GoToTab(7), CMD, Key::Num7),
    bind(Command::GoToTab(8), CMD, Key::Num8),
    bind(Command::LastTab, CMD, Key::Num9),
    bind(Command::Palette, CMD, Key::P),
    bind(Command::Settings, CMD, Key::Comma),
    bind(Command::Reload, NONE, Key::F5),
    bind(Command::Manual, NONE, Key::F1),
    bind(Command::Fetch, CMD_SHIFT, Key::F),
    bind(Command::Pull, CMD_SHIFT, Key::L),
    bind(Command::Push, CMD_SHIFT, Key::K),
    bind(Command::Stash, CMD_SHIFT, Key::S),
    bind(Command::NewBranch, CMD, Key::B),
    bind(Command::Terminal, CMD, Key::J),
    bind(Command::StageAll, CMD_SHIFT, Key::A),
    bind(Command::Commit, CMD, Key::Enter),
    bind(Command::FocusSidebar, CMD.plus(Modifiers::ALT), Key::Num1),
    bind(Command::FocusGraph, CMD.plus(Modifiers::ALT), Key::Num2),
    bind(Command::FocusDetail, CMD.plus(Modifiers::ALT), Key::Num3),
    bind(Command::FileNext, NONE, Key::J),
    bind(Command::FilePrevious, NONE, Key::K),
    bind(Command::FileToggleStage, NONE, Key::Space),
    bind(Command::FileOpen, NONE, Key::Enter),
    bind(Command::HunkStage, NONE, Key::S),
    bind(Command::HunkUnstage, NONE, Key::U),
    bind(Command::HunkDiscard, NONE, Key::D),
    bind(Command::Undo, CMD, Key::Z),
    bind(Command::Redo, CMD, Key::Y),
    bind(Command::Timeline, CMD_SHIFT, Key::Z),
    bind(Command::Search, CMD, Key::F),
    bind(Command::Back, Modifiers::ALT, Key::ArrowLeft),
    bind(Command::Forward, Modifiers::ALT, Key::ArrowRight),
    bind(Command::NextChange, NONE, Key::F7),
    bind(Command::PreviousChange, Modifiers::SHIFT, Key::F7),
    bind(Command::NextChangeAlt, Modifiers::ALT, Key::ArrowDown),
    bind(Command::PreviousChangeAlt, Modifiers::ALT, Key::ArrowUp),
    bind(Command::GraphPrevious, NONE, Key::ArrowUp),
    bind(Command::GraphNext, NONE, Key::ArrowDown),
    bind(Command::GraphLeft, NONE, Key::ArrowLeft),
    bind(Command::GraphRight, NONE, Key::ArrowRight),
    bind(Command::RebasePrevious, NONE, Key::ArrowUp),
    bind(Command::RebaseNext, NONE, Key::ArrowDown),
    bind(Command::RebaseMoveUp, Modifiers::ALT, Key::ArrowUp),
    bind(Command::RebaseMoveDown, Modifiers::ALT, Key::ArrowDown),
    bind(Command::RebasePick, NONE, Key::P),
    bind(Command::RebaseReword, NONE, Key::R),
    bind(Command::RebaseEdit, NONE, Key::E),
    bind(Command::RebaseSquash, NONE, Key::S),
    bind(Command::RebaseFixup, NONE, Key::F),
    bind(Command::RebaseDrop, NONE, Key::D),
    bind(Command::SettingsSearch, CMD, Key::F),
    bind(Command::SettingsPrevious, Modifiers::ALT, Key::ArrowUp),
    bind(Command::SettingsNext, Modifiers::ALT, Key::ArrowDown),
    bind(Command::FindNext, NONE, Key::F3),
    bind(Command::FindPrevious, Modifiers::SHIFT, Key::F3),
    bind(Command::SelectionCopy, CMD, Key::C),
    bind(Command::SelectionAll, CMD, Key::A),
];

/// Optional actions appear in the same editor/manual registry and start unbound.
pub(crate) const UNBOUND_COMMANDS: &[Command] = &[
    Command::SelectionHistory,
    Command::GraphCopyHash,
    Command::GraphCopyMessage,
    Command::GraphCopyBranch,
    Command::FileHistory,
    Command::FileBlame,
    Command::FileContents,
    Command::FileCopyPath,
];

/// The command `key` with `modifiers` held does, if any.
#[cfg(test)]
pub(crate) fn find(key: Key, modifiers: Modifiers) -> Option<Command> {
    BINDINGS
        .iter()
        .find(|b| {
            matches!(b.command.scope(), Scope::Window | Scope::Repository)
                && b.key == key
                && modifiers.matches_exact(b.modifiers)
        })
        .map(|b| b.command)
}

/// The key of `command`, as the manual, the tooltips and the menus write it ("Ctrl+Shift+F", or
/// "Cmd+Shift+F" on macOS). Empty for a command with no key.
pub(crate) fn label(ctx: &egui::Context, command: Command) -> String {
    config::effective(
        &config::usable(&crate::settings::Settings::get(ctx).key_bindings),
        command,
    )
    .iter()
    .map(Stroke::label)
    .collect::<Vec<_>>()
    .join(" → ")
}

/// `text` followed by the key of `command` in brackets, for a tooltip.
pub(crate) fn tip(ctx: &egui::Context, text: &str, command: Command) -> String {
    {
        let keys = label(ctx, command);
        if keys.is_empty() {
            text.to_owned()
        } else {
            format!("{text} ({keys})")
        }
    }
}

/// Keys written as the manual writes them ("Cmd+Z / Cmd+Y"), with Ctrl outside macOS.
pub(crate) fn written(keys: &str) -> String {
    keys.replace("Cmd", crate::manual::command())
}

/// Whether a palette hint is keys rather than a category ("Ctrl+B", "F5", not "branch").
pub(crate) fn is_keys(hint: &str) -> bool {
    let mut parts = hint.split('+');
    let key = parts.next_back().unwrap_or_default();
    let modifiers_ok = parts.all(|m| ["Ctrl", "Cmd", "Shift", "Alt"].contains(&m));
    let single_key = hint.contains('+') && !key.is_empty();
    let function_key = key.len() <= 3
        && key.starts_with('F')
        && key.len() > 1
        && key[1..].chars().all(|c| c.is_ascii_digit());
    modifiers_ok && (single_key || function_key)
}

fn combo(modifiers: Modifiers, key: Key) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if modifiers.command {
        parts.push(crate::manual::command());
    } else if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    parts.push(key.symbol_or_name());
    parts.join("+")
}

mod reference;
pub(crate) use reference::*;

#[cfg(test)]
mod tests;
