//! The keys of the window's and of a repository tab's main actions: one list that the key
//! handlers, the manual, Settings → Keyboard shortcuts, the toolbar's tooltips, the palette and
//! the menus all read, so none of them can say another key than the one that works.

use egui::{InputState, Key, Modifiers};

/// What a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Command {
    NewTab,
    CloseTab,
    ReopenTab,
    NextTab,
    PreviousTab,
    /// The tab at this place, 1 to 8.
    GoToTab(u8),
    LastTab,
    Palette,
    Settings,
    Reload,
    Manual,
    Fetch,
    Pull,
    Push,
    Stash,
    NewBranch,
    Terminal,
    StageAll,
    Commit,
}

/// Where a key works: anywhere in the window, or in a repository tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Window,
    Repository,
}

impl Command {
    pub(crate) fn scope(self) -> Scope {
        match self {
            Command::NewTab
            | Command::CloseTab
            | Command::ReopenTab
            | Command::NextTab
            | Command::PreviousTab
            | Command::GoToTab(_)
            | Command::LastTab
            | Command::Palette
            | Command::Settings
            | Command::Reload
            | Command::Manual => Scope::Window,
            Command::Fetch
            | Command::Pull
            | Command::Push
            | Command::Stash
            | Command::NewBranch
            | Command::Terminal
            | Command::StageAll
            | Command::Commit => Scope::Repository,
        }
    }

    /// Whether holding the key down does it again: going through the tabs does, an operation
    /// started once doesn't.
    fn repeats(self) -> bool {
        matches!(self, Command::NextTab | Command::PreviousTab)
    }
}

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
];

/// The command `key` with `modifiers` held does, if any.
pub(crate) fn find(key: Key, modifiers: Modifiers) -> Option<Command> {
    BINDINGS
        .iter()
        .find(|b| b.key == key && modifiers.matches_exact(b.modifiers))
        .map(|b| b.command)
}

/// The commands of `scope` whose key was pressed this frame, in the order pressed.
pub(crate) fn pressed(input: &InputState, scope: Scope) -> Vec<Command> {
    input
        .events
        .iter()
        .filter_map(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } => find(*key, *modifiers).filter(|c| !*repeat || c.repeats()),
            _ => None,
        })
        .filter(|c| c.scope() == scope)
        .collect()
}

/// The key of `command`, as the manual, the tooltips and the menus write it ("Ctrl+Shift+F", or
/// "Cmd+Shift+F" on macOS). Empty for a command with no key.
pub(crate) fn label(command: Command) -> String {
    BINDINGS
        .iter()
        .find(|b| b.command == command)
        .map(|b| combo(b.modifiers, b.key))
        .unwrap_or_default()
}

/// `text` followed by the key of `command` in brackets, for a tooltip.
pub(crate) fn tip(text: &str, command: Command) -> String {
    format!("{text} ({})", label(command))
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

/// A row of the manual: the commands it covers and what they do. `range`: the keys of the
/// first and the last command, written "Ctrl+1…8".
pub(crate) struct Row {
    pub commands: &'static [Command],
    pub range: bool,
    pub what: &'static str,
}

const fn row(commands: &'static [Command], what: &'static str) -> Row {
    Row {
        commands,
        range: false,
        what,
    }
}

/// The manual's rows for the window's keys.
pub(crate) const WINDOW_ROWS: &[Row] = &[
    row(&[Command::NewTab], "New tab"),
    row(&[Command::CloseTab], "Close the tab (or middle-click it)"),
    row(&[Command::ReopenTab], "Reopen the tab closed last"),
    row(
        &[Command::NextTab, Command::PreviousTab],
        "Next / previous tab, round from the last to the first",
    ),
    Row {
        commands: &[
            Command::GoToTab(1),
            Command::GoToTab(2),
            Command::GoToTab(3),
            Command::GoToTab(4),
            Command::GoToTab(5),
            Command::GoToTab(6),
            Command::GoToTab(7),
            Command::GoToTab(8),
        ],
        range: true,
        what: "Go to the first to the eighth tab",
    },
    row(&[Command::LastTab], "Go to the last tab"),
    row(
        &[Command::Palette],
        "Command palette: every action and branch",
    ),
    row(&[Command::Settings], "Settings"),
    row(&[Command::Reload], "Reload the repository"),
    row(&[Command::Manual], "This manual"),
];

/// The manual's rows for a repository tab's keys. None works while a text field or the
/// terminal has the keyboard.
pub(crate) const REPOSITORY_ROWS: &[Row] = &[
    row(&[Command::Fetch], "Fetch all remotes"),
    row(&[Command::Pull], "Pull the current branch"),
    row(&[Command::Push], "Push the current branch"),
    row(&[Command::Stash], "Stash all changes"),
    row(&[Command::NewBranch], "Create a branch at HEAD"),
    row(&[Command::Terminal], "Show or hide the terminal"),
    row(&[Command::StageAll], "Stage all changes"),
    row(
        &[Command::Commit],
        "Commit (with no summary yet, go to the message)",
    ),
];

/// The keys of a manual row, as `manual::key_combo` draws them ("Ctrl+Tab / Ctrl+Shift+Tab").
pub(crate) fn row_keys(row: &Row) -> String {
    match (row.range, row.commands.first(), row.commands.last()) {
        (true, Some(&first), Some(&last)) => {
            let last_key = BINDINGS
                .iter()
                .find(|b| b.command == last)
                .map(|b| b.key.symbol_or_name())
                .unwrap_or_default();
            format!("{}…{last_key}", label(first))
        }
        _ => row
            .commands
            .iter()
            .map(|&c| label(c))
            .collect::<Vec<_>>()
            .join(" / "),
    }
}

#[cfg(test)]
mod tests;
