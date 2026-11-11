//! Command identities, scopes, labels, and repeat policy.
use super::*;

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
    GraphCopyHash,
    GraphCopyMessage,
    GraphCopyBranch,
    FileHistory,
    FileBlame,
    FileContents,
    FileCopyPath,
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
    pub(super) fn overlaps(self, other: Self) -> bool {
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
            Command::GraphCopyHash
            | Command::GraphCopyMessage
            | Command::GraphCopyBranch
            | Command::GraphPrevious
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
            Command::FileHistory
            | Command::FileBlame
            | Command::FileContents
            | Command::FileCopyPath
            | Command::FileNext
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
            Self::GraphCopyHash => "graph_copy_hash",
            Self::GraphCopyMessage => "graph_copy_message",
            Self::GraphCopyBranch => "graph_copy_branch",
            Self::FileHistory => "file_history",
            Self::FileBlame => "file_blame",
            Self::FileContents => "file_contents",
            Self::FileCopyPath => "file_copy_path",
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
            .map_or_else(
                || crate::i18n::text("interaction-key-action"),
                |r| crate::i18n::text(r.what),
            )
    }

    /// Whether holding the key down does it again: going through the tabs does, an operation
    /// started once doesn't.
    pub(super) fn repeats(self) -> bool {
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
