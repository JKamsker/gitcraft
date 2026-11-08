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
    FocusSidebar,
    FocusGraph,
    FocusDetail,
    FileNext,
    FilePrevious,
    FileToggleStage,
    FileOpen,
    HunkStage,
    HunkUnstage,
    HunkDiscard,
    Undo,
    Redo,
    Timeline,
    Search,
    Back,
    Forward,
    NextChange,
    PreviousChange,
    NextChangeAlt,
    PreviousChangeAlt,
    GraphPrevious,
    GraphNext,
    GraphLeft,
    GraphRight,
    RebasePrevious,
    RebaseNext,
    RebaseMoveUp,
    RebaseMoveDown,
    RebasePick,
    RebaseReword,
    RebaseEdit,
    RebaseSquash,
    RebaseFixup,
    RebaseDrop,
    SettingsSearch,
    SettingsPrevious,
    SettingsNext,
    FindNext,
    FindPrevious,
    SelectionCopy,
    SelectionAll,
    SelectionHistory,
}

/// Where a key works: anywhere in the window, or in a repository tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Scope {
    Window,
    Repository,
    Files,
    Diff,
    Changes,
    Graph,
    Rebase,
    Settings,
    Find,
    Selection,
}

impl Scope {
    fn overlaps(self, other: Self) -> bool {
        match (self, other) {
            (Self::Window, _) | (_, Self::Window) => true,
            (Self::Settings, scope) | (scope, Self::Settings) => scope == Self::Settings,
            (
                Self::Rebase,
                Self::Files
                | Self::Diff
                | Self::Changes
                | Self::Graph
                | Self::Find
                | Self::Selection,
            )
            | (
                Self::Files
                | Self::Diff
                | Self::Changes
                | Self::Graph
                | Self::Find
                | Self::Selection,
                Self::Rebase,
            )
            | (Self::Files, Self::Diff | Self::Graph)
            | (Self::Diff | Self::Graph, Self::Files) => false,
            _ => true,
        }
    }
}

impl Command {
    pub(crate) fn scope(self) -> Scope {
        match self {
            Command::FindNext | Command::FindPrevious => Scope::Find,
            Command::SelectionCopy | Command::SelectionAll | Command::SelectionHistory => {
                Scope::Selection
            }
            Command::GraphPrevious
            | Command::GraphNext
            | Command::GraphLeft
            | Command::GraphRight => Scope::Graph,
            Command::RebasePrevious
            | Command::RebaseNext
            | Command::RebaseMoveUp
            | Command::RebaseMoveDown
            | Command::RebasePick
            | Command::RebaseReword
            | Command::RebaseEdit
            | Command::RebaseSquash
            | Command::RebaseFixup
            | Command::RebaseDrop => Scope::Rebase,
            Command::SettingsSearch | Command::SettingsPrevious | Command::SettingsNext => {
                Scope::Settings
            }
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
            Command::Undo
            | Command::Redo
            | Command::Timeline
            | Command::Search
            | Command::Back
            | Command::Forward
            | Command::Fetch
            | Command::Pull
            | Command::Push
            | Command::Stash
            | Command::NewBranch
            | Command::Terminal
            | Command::StageAll
            | Command::Commit
            | Command::FocusSidebar
            | Command::FocusGraph
            | Command::FocusDetail => Scope::Repository,
            Command::FileNext
            | Command::FilePrevious
            | Command::FileToggleStage
            | Command::FileOpen => Scope::Files,
            Command::HunkStage | Command::HunkUnstage | Command::HunkDiscard => Scope::Diff,
            Command::NextChange
            | Command::PreviousChange
            | Command::NextChangeAlt
            | Command::PreviousChangeAlt => Scope::Changes,
        }
    }

    /// Stable IDs are persisted independently of translated labels.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::FindNext => "find_next",
            Self::FindPrevious => "find_previous",
            Self::SelectionCopy => "selection_copy",
            Self::SelectionAll => "selection_all",
            Self::SelectionHistory => "selection_history",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Timeline => "timeline",
            Self::Search => "search",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::NextChange => "next_change",
            Self::PreviousChange => "previous_change",
            Self::NextChangeAlt => "next_change_alternate",
            Self::PreviousChangeAlt => "previous_change_alternate",
            Self::GraphPrevious => "graph_previous",
            Self::GraphNext => "graph_next",
            Self::GraphLeft => "graph_left",
            Self::GraphRight => "graph_right",
            Self::RebasePrevious => "rebase_previous",
            Self::RebaseNext => "rebase_next",
            Self::RebaseMoveUp => "rebase_move_up",
            Self::RebaseMoveDown => "rebase_move_down",
            Self::RebasePick => "rebase_pick",
            Self::RebaseReword => "rebase_reword",
            Self::RebaseEdit => "rebase_edit",
            Self::RebaseSquash => "rebase_squash",
            Self::RebaseFixup => "rebase_fixup",
            Self::RebaseDrop => "rebase_drop",
            Self::SettingsSearch => "settings_search",
            Self::SettingsPrevious => "settings_previous",
            Self::SettingsNext => "settings_next",
            Self::NewTab => "new_tab",
            Self::CloseTab => "close_tab",
            Self::ReopenTab => "reopen_tab",
            Self::NextTab => "next_tab",
            Self::PreviousTab => "previous_tab",
            Self::LastTab => "last_tab",
            Self::Palette => "palette",
            Self::Settings => "settings",
            Self::Reload => "reload",
            Self::Manual => "manual",
            Self::Fetch => "fetch",
            Self::Pull => "pull",
            Self::Push => "push",
            Self::Stash => "stash",
            Self::NewBranch => "new_branch",
            Self::Terminal => "terminal",
            Self::StageAll => "stage_all",
            Self::Commit => "commit",
            Self::FocusSidebar => "focus_sidebar",
            Self::FocusGraph => "focus_graph",
            Self::FocusDetail => "focus_detail",
            Self::FileNext => "file_next",
            Self::FilePrevious => "file_previous",
            Self::FileToggleStage => "file_toggle_stage",
            Self::FileOpen => "file_open",
            Self::HunkStage => "hunk_stage",
            Self::HunkUnstage => "hunk_unstage",
            Self::HunkDiscard => "hunk_discard",
            Self::GoToTab(1) => "tab_1",
            Self::GoToTab(2) => "tab_2",
            Self::GoToTab(3) => "tab_3",
            Self::GoToTab(4) => "tab_4",
            Self::GoToTab(5) => "tab_5",
            Self::GoToTab(6) => "tab_6",
            Self::GoToTab(7) => "tab_7",
            Self::GoToTab(8) => "tab_8",
            Self::GoToTab(_) => "invalid_tab",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        WINDOW_ROWS
            .iter()
            .chain(REPOSITORY_ROWS)
            .chain(LOCAL_ROWS)
            .find(|r| r.commands.contains(&self))
            .map_or("Action", |r| r.what)
    }

    /// Whether holding the key down does it again: going through the tabs does, an operation
    /// started once doesn't.
    fn repeats(self) -> bool {
        matches!(
            self,
            Command::NextTab
                | Command::PreviousTab
                | Command::FileNext
                | Command::FilePrevious
                | Command::NextChange
                | Command::PreviousChange
                | Command::NextChangeAlt
                | Command::PreviousChangeAlt
                | Command::Back
                | Command::Forward
                | Command::GraphPrevious
                | Command::GraphNext
                | Command::GraphLeft
                | Command::GraphRight
                | Command::RebasePrevious
                | Command::RebaseNext
                | Command::RebaseMoveUp
                | Command::RebaseMoveDown
                | Command::SettingsPrevious
                | Command::SettingsNext
                | Command::FindNext
                | Command::FindPrevious
        )
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
pub(crate) const UNBOUND_COMMANDS: &[Command] = &[Command::SelectionHistory];

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
        "Next / previous tab, wrapping around (Ctrl on macOS too)",
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
        what: "Go to tab 1 to 8",
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
    row(&[Command::Undo], "Undo"),
    row(&[Command::Redo], "Redo"),
    row(&[Command::Timeline], "Timeline and undo tree"),
    row(
        &[Command::Search],
        "Search the shown diff, file, blame or commits",
    ),
    row(&[Command::Back], "Back through recent views"),
    row(&[Command::Forward], "Forward through recent views"),
    row(&[Command::FocusSidebar], "Focus the sidebar"),
    row(&[Command::FocusGraph], "Focus the graph or diff"),
    row(
        &[Command::FocusDetail],
        "Focus the right pane and its files",
    ),
    row(&[Command::Fetch], "Fetch all remotes"),
    row(&[Command::Pull], "Pull the current branch"),
    row(&[Command::Push], "Push the current branch"),
    row(&[Command::Stash], "Stash all changes…"),
    row(&[Command::NewBranch], "Create a branch at HEAD"),
    row(
        &[Command::Terminal],
        "Show or hide the terminal (to hide it from inside, click outside it first)",
    ),
    row(&[Command::StageAll], "Stage all changes"),
    row(
        &[Command::Commit],
        "Commit; with nothing staged, everything; with no summary yet, go to the message. In a note editor, save the note",
    ),
];

pub(crate) const LOCAL_ROWS: &[Row] = &[
    row(&[Command::FindNext], "Find: next match"),
    row(&[Command::FindPrevious], "Find: previous match"),
    row(&[Command::SelectionCopy], "Copy selected lines"),
    row(&[Command::SelectionAll], "Select all shown lines"),
    row(&[Command::SelectionHistory], "History of selected lines"),
    row(&[Command::GraphPrevious], "Graph: previous commit"),
    row(&[Command::GraphNext], "Graph: next commit"),
    row(&[Command::GraphLeft], "Graph lane walk: jump to tip"),
    row(
        &[Command::GraphRight],
        "Graph lane walk: cycle merge parent",
    ),
    row(&[Command::RebasePrevious], "Rebase: previous row"),
    row(&[Command::RebaseNext], "Rebase: next row"),
    row(&[Command::RebaseMoveUp], "Rebase: move commit up"),
    row(&[Command::RebaseMoveDown], "Rebase: move commit down"),
    row(&[Command::RebasePick], "Rebase: pick"),
    row(&[Command::RebaseReword], "Rebase: reword"),
    row(&[Command::RebaseEdit], "Rebase: edit"),
    row(&[Command::RebaseSquash], "Rebase: squash"),
    row(&[Command::RebaseFixup], "Rebase: fix up"),
    row(&[Command::RebaseDrop], "Rebase: drop"),
    row(&[Command::SettingsSearch], "Settings: focus search"),
    row(&[Command::SettingsPrevious], "Settings: previous page"),
    row(&[Command::SettingsNext], "Settings: next page"),
    row(&[Command::NextChange], "Next change"),
    row(&[Command::PreviousChange], "Previous change"),
    row(&[Command::NextChangeAlt], "Next change (alternate binding)"),
    row(
        &[Command::PreviousChangeAlt],
        "Previous change (alternate binding)",
    ),
    row(&[Command::FileNext], "Next file"),
    row(&[Command::FilePrevious], "Previous file"),
    row(
        &[Command::FileToggleStage],
        "Stage or unstage selected file",
    ),
    row(&[Command::FileOpen], "Open selected file diff"),
    row(&[Command::HunkStage], "Stage current hunk"),
    row(&[Command::HunkUnstage], "Unstage current hunk"),
    row(
        &[Command::HunkDiscard],
        "Discard current hunk (confirmation)",
    ),
];

/// The keys of a manual row, as `manual::key_combo` draws them ("Ctrl+Tab / Ctrl+Shift+Tab").
pub(crate) fn row_keys(ctx: &egui::Context, row: &Row) -> String {
    match (row.range, row.commands.first(), row.commands.last()) {
        (true, Some(&first), Some(&last))
            if row.commands.iter().all(|c| {
                !crate::settings::Settings::get(ctx)
                    .key_bindings
                    .contains_key(c.id())
            }) =>
        {
            let last_key = BINDINGS
                .iter()
                .find(|b| b.command == last)
                .map(|b| b.key.symbol_or_name())
                .unwrap_or_default();
            format!("{}…{last_key}", label(ctx, first))
        }
        _ => row
            .commands
            .iter()
            .map(|&c| label(ctx, c))
            .collect::<Vec<_>>()
            .join(" / "),
    }
}

#[cfg(test)]
mod tests;
