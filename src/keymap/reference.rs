//! Shortcut reference rows and their effective configured chords.
use super::*;

/// A row of the manual: the commands it covers and what they do. `range`: the keys of the
/// first and the last command, written "Ctrl+1…8".
pub(crate) struct Row {
    pub commands: &'static [Command],
    pub range: bool,
    pub what: &'static str,
}

pub(super) const fn row(commands: &'static [Command], what: &'static str) -> Row {
    Row {
        commands,
        range: false,
        what,
    }
}

/// The manual's rows for the window's keys.
pub(crate) const WINDOW_ROWS: &[Row] = &[
    row(&[Command::NewTab], "interaction-key-row-new-tab"),
    row(
        &[Command::CloseTab],
        "interaction-key-row-close-the-tab-or-middle-click-it",
    ),
    row(
        &[Command::ReopenTab],
        "interaction-key-row-reopen-the-tab-closed-last",
    ),
    row(
        &[Command::NextTab, Command::PreviousTab],
        "interaction-key-row-next-previous-tab-wrapping-around-ctrl-on-macos-too",
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
        what: "interaction-key-row-go-to-tab-1-to-8",
    },
    row(
        &[Command::LastTab],
        "interaction-key-row-go-to-the-last-tab",
    ),
    row(
        &[Command::Palette],
        "interaction-key-row-command-palette-every-action-and-branch",
    ),
    row(&[Command::Settings], "interaction-key-row-settings"),
    row(
        &[Command::Reload],
        "interaction-key-row-reload-the-repository",
    ),
    row(&[Command::Manual], "interaction-key-row-this-manual"),
];

/// The manual's rows for a repository tab's keys. None works while a text field or the
/// terminal has the keyboard.
pub(crate) const REPOSITORY_ROWS: &[Row] = &[
    row(&[Command::Undo], "interaction-key-row-undo"),
    row(&[Command::Redo], "interaction-key-row-redo"),
    row(
        &[Command::Timeline],
        "interaction-key-row-timeline-and-undo-tree",
    ),
    row(
        &[Command::Search],
        "interaction-key-row-search-the-shown-diff-file-blame-or-commits",
    ),
    row(
        &[Command::Back],
        "interaction-key-row-back-through-recent-views",
    ),
    row(
        &[Command::Forward],
        "interaction-key-row-forward-through-recent-views",
    ),
    row(
        &[Command::FocusSidebar],
        "interaction-key-row-focus-the-sidebar",
    ),
    row(
        &[Command::FocusGraph],
        "interaction-key-row-focus-the-graph-or-diff",
    ),
    row(
        &[Command::FocusDetail],
        "interaction-key-row-focus-the-right-pane-and-its-files",
    ),
    row(&[Command::Fetch], "interaction-key-row-fetch-all-remotes"),
    row(
        &[Command::Pull],
        "interaction-key-row-pull-the-current-branch",
    ),
    row(
        &[Command::Push],
        "interaction-key-row-push-the-current-branch",
    ),
    row(&[Command::Stash], "interaction-key-row-stash-all-changes"),
    row(
        &[Command::NewBranch],
        "interaction-key-row-create-a-branch-at-head",
    ),
    row(
        &[Command::Terminal],
        "interaction-key-row-show-or-hide-the-terminal-to-hide-it-from-inside-click-outside-it-first",
    ),
    row(
        &[Command::StageAll],
        "interaction-key-row-stage-all-changes",
    ),
    row(
        &[Command::Commit],
        "interaction-key-row-commit-with-nothing-staged-everything-with-no-summary-yet-go-to-the-message",
    ),
];

pub(crate) const LOCAL_ROWS: &[Row] = &[
    row(
        &[Command::GraphCopyHash],
        "interaction-key-row-graph-copy-selected-commit-hash",
    ),
    row(
        &[Command::GraphCopyMessage],
        "interaction-key-row-graph-copy-selected-commit-message",
    ),
    row(
        &[Command::GraphCopyBranch],
        "interaction-key-row-graph-copy-selected-commit-s-first-branch",
    ),
    row(
        &[Command::FileHistory],
        "interaction-key-row-files-history-of-selected-file",
    ),
    row(
        &[Command::FileBlame],
        "interaction-key-row-files-blame-selected-file",
    ),
    row(
        &[Command::FileContents],
        "interaction-key-row-files-view-selected-file",
    ),
    row(
        &[Command::FileCopyPath],
        "interaction-key-row-files-copy-selected-path",
    ),
    row(&[Command::FindNext], "interaction-key-row-find-next-match"),
    row(
        &[Command::FindPrevious],
        "interaction-key-row-find-previous-match",
    ),
    row(
        &[Command::SelectionCopy],
        "interaction-key-row-copy-selected-lines",
    ),
    row(
        &[Command::SelectionAll],
        "interaction-key-row-select-all-shown-lines",
    ),
    row(
        &[Command::SelectionHistory],
        "interaction-key-row-history-of-selected-lines",
    ),
    row(
        &[Command::GraphPrevious],
        "interaction-key-row-graph-previous-commit",
    ),
    row(
        &[Command::GraphNext],
        "interaction-key-row-graph-next-commit",
    ),
    row(
        &[Command::GraphLeft],
        "interaction-key-row-graph-lane-walk-jump-to-tip",
    ),
    row(
        &[Command::GraphRight],
        "interaction-key-row-graph-lane-walk-cycle-merge-parent",
    ),
    row(
        &[Command::RebasePrevious],
        "interaction-key-row-rebase-previous-row",
    ),
    row(
        &[Command::RebaseNext],
        "interaction-key-row-rebase-next-row",
    ),
    row(
        &[Command::RebaseMoveUp],
        "interaction-key-row-rebase-move-commit-up",
    ),
    row(
        &[Command::RebaseMoveDown],
        "interaction-key-row-rebase-move-commit-down",
    ),
    row(&[Command::RebasePick], "interaction-key-row-rebase-pick"),
    row(
        &[Command::RebaseReword],
        "interaction-key-row-rebase-reword",
    ),
    row(&[Command::RebaseEdit], "interaction-key-row-rebase-edit"),
    row(
        &[Command::RebaseSquash],
        "interaction-key-row-rebase-squash",
    ),
    row(&[Command::RebaseFixup], "interaction-key-row-rebase-fix-up"),
    row(&[Command::RebaseDrop], "interaction-key-row-rebase-drop"),
    row(
        &[Command::SettingsSearch],
        "interaction-key-row-settings-focus-search",
    ),
    row(
        &[Command::SettingsPrevious],
        "interaction-key-row-settings-previous-page",
    ),
    row(
        &[Command::SettingsNext],
        "interaction-key-row-settings-next-page",
    ),
    row(&[Command::NextChange], "interaction-key-row-next-change"),
    row(
        &[Command::PreviousChange],
        "interaction-key-row-previous-change",
    ),
    row(
        &[Command::NextChangeAlt],
        "interaction-key-row-next-change-alternate-binding",
    ),
    row(
        &[Command::PreviousChangeAlt],
        "interaction-key-row-previous-change-alternate-binding",
    ),
    row(&[Command::FileNext], "interaction-key-row-next-file"),
    row(
        &[Command::FilePrevious],
        "interaction-key-row-previous-file",
    ),
    row(
        &[Command::FileToggleStage],
        "interaction-key-row-stage-or-unstage-selected-file",
    ),
    row(
        &[Command::FileOpen],
        "interaction-key-row-open-selected-file-diff",
    ),
    row(
        &[Command::HunkStage],
        "interaction-key-row-stage-current-hunk",
    ),
    row(
        &[Command::HunkUnstage],
        "interaction-key-row-unstage-current-hunk",
    ),
    row(
        &[Command::HunkDiscard],
        "interaction-key-row-discard-current-hunk-confirmation",
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
