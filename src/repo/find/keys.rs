//! Which search Ctrl+F is for: the find bar while a diff, a file or a blame is shown, the
//! commit search otherwise; and the find bar's keys outside its text field.

use egui::{Key, Modifiers};

use super::bar::FIELD_ID;
use crate::repo::diff::rendered;
use crate::repo::file_view::FileTab;
use crate::repo::{DiffKey, RepoTab};

/// What the central pane shows that the find bar can search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FindIn {
    Diff,
    /// A diff switched to the whole file, or the file view's File tab.
    File,
    Blame,
}

impl FindIn {
    /// What the bar's hint calls it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            FindIn::Diff => "the diff",
            FindIn::File => "the file",
            FindIn::Blame => "the blame",
        }
    }
}

impl RepoTab {
    /// What the find bar would search in the central pane now (as `layout.rs` picks the view
    /// to show); `None` for the graph and the views without lines to search.
    pub(in crate::repo) fn find_in(&self) -> Option<FindIn> {
        if self.conflict.is_some() || self.rebase.is_some() {
            return None;
        }
        // A diff shows whenever there is one: on its own, or beside its file's history.
        if let Some(diff) = &self.diff {
            let path = match &diff.key {
                DiffKey::Commit { path, .. } | DiffKey::Range { path, .. } => path,
                DiffKey::Work(file) => &file.path,
            };
            return if rendered::is_markdown(path) && self.diff_rendered {
                None
            } else if self.diff_as_file {
                Some(FindIn::File)
            } else {
                Some(FindIn::Diff)
            };
        }
        match self.file_view.as_ref()?.tab {
            FileTab::Blame => Some(FindIn::Blame),
            FileTab::File => Some(FindIn::File),
            FileTab::History => None,
        }
    }

    /// The find bar's keys, before the tab's own: Ctrl+F opens it where there's something to
    /// search (and is then not the commit search's), F3 / Shift+F3 and, with nothing else
    /// taking the keyboard, Enter / Shift+Enter step, Esc closes it. `free`: no dialog, no text
    /// field has the keyboard. While another field has it (the terminal), its keys are its own.
    pub(in crate::repo) fn find_keys(&mut self, ctx: &egui::Context, free: bool) {
        if self.dialog.is_some() || self.keys_blocked {
            return;
        }
        let Some(_) = self.find_in() else {
            // What it searched is gone: the graph's Ctrl+F is the commit search again.
            if self.find.open {
                self.find.close();
            }
            return;
        };
        let open = self.find.open;
        // Typing in the find bar itself; any other field (the terminal, the commit message)
        // keeps its keys.
        let in_bar = ctx.memory(|m| m.focused()) == Some(egui::Id::new(FIELD_ID));
        if !free && !in_bar {
            return;
        }
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND, Key::F) {
                self.find.show();
            }
            if !open {
                return;
            }
            if i.consume_key(Modifiers::SHIFT, Key::F3) {
                self.find.step(-1);
            } else if i.consume_key(Modifiers::NONE, Key::F3) {
                self.find.step(1);
            }
            if !free {
                return;
            }
            if i.consume_key(Modifiers::SHIFT, Key::Enter) {
                self.find.step(-1);
            } else if i.consume_key(Modifiers::NONE, Key::Enter) {
                self.find.step(1);
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                self.find.close();
            }
        });
    }
}
