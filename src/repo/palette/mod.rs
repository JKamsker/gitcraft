//! What the command palette offers for a repository: its commands, what can be done with the
//! selected commit or file, and verbs (merge, rebase, check out…) whose branch is picked in a
//! second step, so that a repository with thousands of branches doesn't get an entry for each
//! combination.

mod picks;
mod selection;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use super::pull::PullMode;
use super::toolbar::reveal_label;
use super::{Action, Dialog, DiffKey, Refresh, RepoTab, ops};

/// One palette entry.
pub(crate) struct PaletteItem {
    pub label: String,
    /// What kind of entry it is (`remote`, `branch`…), on the right.
    pub hint: String,
    /// The keys that do the same, drawn on the right as a keycap; `Ctrl` stands for Cmd on
    /// macOS.
    pub shortcut: Option<String>,
    /// What the palette remembers it by when it is run: the same entry across reloads even
    /// when its label names something that changes (the checked-out branch).
    pub key: String,
    /// It is about the selected commit or file: it comes first.
    pub selection: bool,
    pub run: PaletteRun,
}

/// What choosing an entry does.
pub(crate) enum PaletteRun {
    Action(Action),
    /// Lists what to do it with, to pick from.
    Pick(Pick),
}

/// A verb whose object is picked in a second step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    Checkout,
    Interaction,
    Merge,
    Rebase,
    InteractiveRebase,
    /// What a branch changed since it left the checked-out one.
    ChangesAdded,
    Compare,
    /// Push a branch other than the checked-out one.
    Push,
    /// Reset the checked-out branch to this commit, picking how.
    Reset(String),
}

impl Pick {
    /// The prompt of the second step's field.
    pub(crate) fn placeholder(&self) -> &'static str {
        match self {
            Pick::Reset(_) => "How to reset…",
            Pick::Push => "Branch to push…",
            _ => "Branch…",
        }
    }

    /// What its choices are remembered under.
    fn key(&self) -> &'static str {
        match self {
            Pick::Checkout => "checkout",
            Pick::Interaction => "interaction",
            Pick::Merge => "merge",
            Pick::Rebase => "rebase",
            Pick::InteractiveRebase => "interactive-rebase",
            Pick::ChangesAdded => "changes-added",
            Pick::Compare => "compare",
            Pick::Push => "push",
            Pick::Reset(_) => "reset",
        }
    }
}

impl PaletteItem {
    /// An entry remembered by its label.
    pub(crate) fn new(label: impl Into<String>, hint: &str, run: PaletteRun) -> Self {
        let label = label.into();
        Self {
            key: label.clone(),
            label,
            hint: hint.to_owned(),
            shortcut: None,
            selection: false,
            run,
        }
    }

    pub(crate) fn action(label: impl Into<String>, hint: &str, action: Action) -> Self {
        Self::new(label, hint, PaletteRun::Action(action))
    }

    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    pub(crate) fn shortcut(mut self, keys: &str) -> Self {
        self.shortcut = Some(keys.to_owned());
        self
    }

    fn selection(mut self) -> Self {
        self.selection = true;
        self
    }
}

/// What the palette's entries for a repository were made from: when it changes, they are
/// made again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaletteGeneration {
    history: u64,
    submodules: u64,
    selected: Option<usize>,
    file: Option<DiffKey>,
    busy: bool,
}

impl RepoTab {
    /// Changes whenever what [`Self::palette_items`] is made from (refs, remotes, submodules,
    /// the selected commit and the open file) is read again or changes, and whether an operation is running.
    pub(crate) fn palette_generation(&self) -> PaletteGeneration {
        PaletteGeneration {
            history: self.history_gen,
            submodules: self.submodules_read,
            selected: self.selected,
            file: self.diff.as_ref().map(|d| d.key.clone()),
            busy: self.busy_op.is_some(),
        }
    }

    /// What the command palette offers for this repository: what can be done with the
    /// selection first, then the commands, then the verbs that pick a branch next.
    pub(crate) fn palette_items(&self, ctx: &egui::Context) -> Vec<PaletteItem> {
        let mut items = self.selection_items();
        items.extend(self.command_items(ctx));
        items.extend(self.pick_items());
        for command in crate::keymap::LOCAL_ROWS
            .iter()
            .chain(&crate::keymap::REPOSITORY_ROWS[..3])
            .flat_map(|r| r.commands.iter().copied())
        {
            // A registered widget key is not automatically an executable palette action.
            // Selection/find/rebase primitives stay with their view unless key_action owns
            // an eligible target. Never publish an Action::Key that returns Ok(None).
            if matches!(self.key_action(command), Ok(Some(_))) {
                items.push(
                    PaletteItem::action(
                        command.title(),
                        "keyboard selection",
                        Action::Key(command),
                    )
                    .shortcut(&crate::keymap::label(ctx, command)),
                );
            }
        }
        let mut labels: std::collections::HashSet<String> = items
            .iter()
            .map(|i| i.label.trim_end_matches('…').to_owned())
            .collect();
        for (label, hint, action) in self.additional_palette_items(ctx) {
            if labels.insert(label.trim_end_matches('…').to_owned()) {
                let item = if crate::keymap::is_keys(&hint) {
                    PaletteItem::action(label, "command", action).shortcut(&hint)
                } else {
                    PaletteItem::action(label, &hint, action)
                };
                items.push(item);
            }
        }
        let mut commands = std::collections::HashSet::new();
        items.retain_mut(|item| {
            let PaletteRun::Action(action) = &item.run else {
                return true;
            };
            let Some(command) = action.key_command() else {
                return true;
            };
            if !commands.insert(command) {
                return false;
            }
            let keys = crate::keymap::label(ctx, command);
            item.shortcut = (!keys.is_empty()).then_some(keys);
            item.run = PaletteRun::Action(Action::Key(command));
            true
        });
        items
    }

    /// The repository's commands, whatever is selected.
    fn command_items(&self, ctx: &egui::Context) -> Vec<PaletteItem> {
        let mut items = vec![
            PaletteItem::action("Fetch", "remote", Action::Fetch),
            PaletteItem::action("Pull", "remote", Action::Pull),
            PaletteItem::action(
                if self.gerrit.is_some() {
                    "Push for review"
                } else {
                    "Push"
                },
                "remote",
                Action::Push,
            ),
        ];
        items.extend(
            PullMode::ALL.map(|m| PaletteItem::action(m.label(), "remote", Action::PullWith(m))),
        );
        items.extend([
            PaletteItem::action("Sync (pull, then push)", "remote", Action::Sync),
            PaletteItem::action(
                "Reflog",
                "recovery",
                Action::Reflog(super::reflog::ReflogAct::Show(super::reflog::Source::head())),
            ),
            PaletteItem::action(
                "Find lost commits",
                "recovery",
                Action::Reflog(super::reflog::ReflogAct::FindLost),
            ),
            PaletteItem::action(
                "Stage all changes",
                "changes",
                Action::Key(crate::keymap::Command::StageAll),
            ),
            PaletteItem::action(
                "Commit",
                "changes",
                Action::Key(crate::keymap::Command::Commit),
            ),
            PaletteItem::action("Apply patch…", "patch", Action::ApplyPatch),
            PaletteItem::action("Undo", "history", Action::Undo),
            PaletteItem::action("Redo", "history", Action::Redo),
            PaletteItem::action("Stash changes", "stash", Action::Dialog(Dialog::stash())),
            PaletteItem::action("Search", "graph or visible lines", Action::FocusSearch),
            PaletteItem::action("Toggle terminal", "terminal", Action::ToggleTerminal),
            PaletteItem::action(
                "New terminal",
                "terminal",
                Action::NewTerminal {
                    shell: None,
                    cwd: None,
                },
            ),
            PaletteItem::action("Reload", "repository", Action::Reload).shortcut("F5"),
            PaletteItem::action(reveal_label(), "folder", Action::OpenPath(PathBuf::new())),
            PaletteItem::action(
                "Open in editor",
                "folder",
                Action::OpenInEditor(PathBuf::new()),
            ),
        ]);
        if self.gerrit.is_some() {
            items.retain(|item| !matches!(item.run, PaletteRun::Action(Action::Sync)));
        }
        if let Some(url) = self.web_page() {
            items.push(PaletteItem::action(
                "Open in browser",
                "remote",
                Action::OpenUrl(url),
            ));
        }
        if !self.submodules.is_empty() {
            items.push(PaletteItem::action(
                "Update submodules",
                "submodule",
                super::keep_submodules::update_all(),
            ));
        }
        if self.lfs {
            for (label, args) in [
                ("LFS: Fetch", ["lfs", "fetch"]),
                ("LFS: Pull", ["lfs", "pull"]),
            ] {
                items.push(PaletteItem::action(
                    label,
                    "remote",
                    Action::Ops {
                        label: label.into(),
                        ops: ops(&[&args]),
                        refresh: Refresh::All,
                    },
                ));
            }
        }
        items.push(PaletteItem::action(
            "Run Actions locally…",
            "act",
            Action::LocalCi(super::local_ci::Act::Open),
        ));
        if !self.refs.head.is_empty() && self.ci_project().is_some() {
            items.push(PaletteItem::action(
                "Show CI for HEAD",
                "builds",
                Action::OpenCi {
                    sha: self.refs.head.clone(),
                    run: None,
                },
            ));
        }
        if !self.refs.head.is_empty() {
            items.push(PaletteItem::action(
                "Create branch…",
                "branch",
                Action::Dialog(Dialog::create_branch(&self.refs.head, "HEAD")),
            ));
            items.push(PaletteItem::action(
                "Create tag…",
                "tag",
                Action::Dialog(Dialog::tag(&self.refs.head, "HEAD", self.default_remote())),
            ));
        }
        for item in &mut items {
            let command = match &item.run {
                PaletteRun::Action(action) => action.key_command(),
                _ => None,
            };
            if let Some(command) = command {
                item.shortcut = Some(crate::keymap::label(ctx, command));
                // The palette and keyboard now share eligibility and operation dispatch.
                item.run = PaletteRun::Action(Action::Key(command));
            }
        }
        items
    }
}
