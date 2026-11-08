//! Which search Ctrl+F is for: the find bar while a diff, a file or a blame is shown, the
//! commit search otherwise; and the find bar's keys outside its text field.

use egui::{Key, Modifiers};

use super::bar::FIELD_ID;
use crate::repo::RepoTab;
use crate::repo::file_view::FileTab;

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
            FindIn::Diff => crate::i18n::text("repo-core-2510"),
            FindIn::File => crate::i18n::text("repo-core-2511"),
            FindIn::Blame => crate::i18n::text("repo-core-2512"),
        }
    }
}

impl RepoTab {
    pub(crate) fn search_shortcut_exception(&self, ctx: &egui::Context) -> bool {
        self.search.open || ctx.memory(|m| m.focused()) == Some(egui::Id::new(FIELD_ID))
    }

    pub(crate) fn find_shortcut_exception(&self, ctx: &egui::Context) -> bool {
        self.find.open && ctx.memory(|m| m.focused()) == Some(egui::Id::new(FIELD_ID))
    }

    /// What the find bar would search in the central pane now (as `layout.rs` picks the view
    /// to show); `None` for the graph and the views without lines to search.
    pub(in crate::repo) fn find_in(&self) -> Option<FindIn> {
        if self.conflict.is_some() || self.rebase.is_some() {
            return None;
        }
        // A diff shows whenever there is one: on its own, or beside its file's history.
        if self.diff.is_some() {
            return Some(if self.diff_as_file {
                FindIn::File
            } else {
                FindIn::Diff
            });
        }
        match self.file_view.as_ref()?.tab {
            FileTab::Blame => Some(FindIn::Blame),
            FileTab::File => Some(FindIn::File),
            FileTab::History => None,
        }
    }

    /// The palette and key command search the same visible lines, or the commit graph.
    pub(in crate::repo) fn focus_search(&mut self, ctx: &egui::Context) {
        let panel = crate::repo::popout::Panel::Diff;
        if self.find_in().is_some()
            && (self.main_keys(ctx, panel)
                || ctx.viewport_id() == crate::repo::popout::viewport_id(&self.path, panel))
        {
            self.find.show();
        } else {
            self.search.open = true;
            self.search.focus = true;
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
        if crate::keymap::take(ctx, crate::keymap::Command::Search) {
            self.focus_search(ctx);
        }
        if open {
            for command in crate::keymap::pressed(ctx, crate::keymap::Scope::Find) {
                self.find
                    .step(if command == crate::keymap::Command::FindPrevious {
                        -1
                    } else {
                        1
                    });
            }
        }
        ctx.input_mut(|i| {
            if !open {
                return;
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

#[cfg(test)]
mod registry_tests {
    use super::*;
    use crate::keymap::{self, Command, Scope, Stroke};
    use crate::repo::DiffKey;

    #[test]
    fn configured_find_steps_actual_matches_with_find_field_focus() {
        use crate::repo::find::{Layout, Source, SourceKey};
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recorded/find-steps".into(), &ctx);
        tab.diff = Some(crate::repo::DiffView::new(DiffKey::Commit {
            hash: "recorded".into(),
            path: "file".into(),
        }));
        tab.find.show();
        tab.find.query.text = "hit".into();
        let mut settings = crate::settings::Settings::default();
        settings.key_bindings.insert(
            Command::FindNext.id().into(),
            vec![Stroke::new(Key::F8, Modifiers::NONE)],
        );
        settings.key_bindings.insert(
            Command::FindPrevious.id().into(),
            vec![Stroke::new(Key::F9, Modifiers::NONE)],
        );
        settings.put_for_test(&ctx);
        for (key, expected) in [
            (None, 0),
            (Some(Key::F8), 1),
            (Some(Key::F3), 1),
            (Some(Key::F9), 0),
        ] {
            ctx.run_ui(
                egui::RawInput {
                    events: key
                        .into_iter()
                        .flat_map(|key| {
                            [true, false].map(|pressed| egui::Event::Key {
                                key,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            })
                        })
                        .collect(),
                    ..Default::default()
                },
                |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut String::new()).id(egui::Id::new(FIELD_ID)),
                    )
                    .request_focus();
                    keymap::active(&ctx, tab.key_scopes(&ctx));
                    keymap::restrict(
                        &ctx,
                        Some(vec![
                            Command::Search,
                            Command::FindNext,
                            Command::FindPrevious,
                        ]),
                    );
                    tab.find.update(
                        &ctx,
                        SourceKey {
                            what: Source::Diff("recorded".into()),
                            at: 1,
                            len: 3,
                        },
                        3,
                        |_| Some("hit"),
                        Layout::Lines,
                    );
                    assert!(tab.find_shortcut_exception(&ctx));
                    tab.find_keys(&ctx, false);
                    assert_eq!(tab.find.current().map(|h| h.line), Some(expected));
                    assert!(!keymap::label(&ctx, Command::FindNext).contains("F3"));
                },
            )
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn configured_search_targets_actual_find_bar_before_commit_search() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recorded/find-keys".into(), &ctx);
        tab.diff = Some(crate::repo::DiffView::new(DiffKey::Commit {
            hash: "recorded".into(),
            path: "file".into(),
        }));
        let mut settings = crate::settings::Settings::default();
        settings.key_bindings.insert(
            Command::Search.id().into(),
            vec![Stroke::new(Key::F8, Modifiers::NONE)],
        );
        settings.put_for_test(&ctx);
        for (key, opens) in [(Key::F8, true), (Key::F, false)] {
            tab.find.close();
            ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: true,
                            repeat: false,
                            modifiers: if key == Key::F {
                                Modifiers::COMMAND
                            } else {
                                Modifiers::NONE
                            },
                        },
                        egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: false,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |_| {
                    keymap::active(&ctx, vec![Scope::Window, Scope::Repository]);
                    let mut acts = Vec::new();
                    tab.shortcuts(&ctx, &mut acts);
                    assert!(
                        acts.is_empty(),
                        "find must own its command before commit search"
                    );
                },
            )
            .drop_without_applying_deltas();
            assert_eq!(tab.find.open, opens);
            assert!(!tab.search.open);
        }
    }
}
