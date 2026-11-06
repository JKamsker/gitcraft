//! Back and forward through the tab's recent views: the graph (with its selected commit), a
//! diff, a file's history, blame or contents, a pull request, a commit's builds. What the big
//! area shows is noted after each frame; going back or forward shows a place again through the
//! same actions that opened it.

use egui::{Key, PointerButton, Ui};

use super::nav_history::NavHistory;
use super::{Action, DiffKey, RepoTab, WIP_HASH, file_view, short};
use crate::git;
use crate::icons;

/// A place the big area showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum View {
    /// The graph, with the commit selected in it (its hash; the changes row is `WIP_HASH`).
    Graph {
        selected: Option<String>,
    },
    Diff(DiffKey),
    /// A file's history, blame or contents (at `rev`, the working tree when `None`).
    File {
        path: String,
        rev: Option<String>,
        tab: file_view::FileTab,
    },
    /// A pull request, by its number.
    Pr(u64),
    /// The builds of a commit.
    Ci(String),
}

impl View {
    /// Another visit to the same kind of place that doesn't need a step of its own: selecting
    /// commits one after the other in the graph replaces the graph's place.
    fn refines(&self, current: &View) -> bool {
        matches!((self, current), (View::Graph { .. }, View::Graph { .. }))
    }

    /// What the back and forward buttons say they go to.
    pub(super) fn label(&self) -> String {
        match self {
            View::Graph { .. } => "the graph".to_owned(),
            View::Diff(key) => format!("the diff of {}", file_name(key.path())),
            View::File { path, tab, .. } => {
                let what = match tab {
                    file_view::FileTab::History => "the history",
                    file_view::FileTab::Blame => "the blame",
                    file_view::FileTab::File => "the contents",
                };
                format!("{what} of {}", file_name(path))
            }
            View::Pr(number) => format!("pull request #{number}"),
            View::Ci(sha) => format!("the builds of {}", short(sha)),
        }
    }
}

/// The last part of a path, as the tooltips name a file.
fn file_name(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(path)
}

/// Notes `view` as what is shown: a new step, or the graph's place refined.
pub(super) fn record(history: &mut NavHistory<View>, view: View) {
    match history.current() {
        Some(current) if view.refines(current) => history.replace_current(view),
        _ => history.visit(view),
    }
}

/// Alt+← or the mouse's back button (`-1`), Alt+→ or its forward button (`1`). `keys`: the
/// keyboard is free (no text field has it), else only the mouse buttons count, as Alt+← and
/// Alt+→ move by words in a text field.
pub(super) fn nav_step(i: &egui::InputState, keys: bool) -> Option<i32> {
    let alt = keys && i.modifiers.alt && !i.modifiers.command && !i.modifiers.shift;
    if (alt && i.key_pressed(Key::ArrowLeft)) || i.pointer.button_pressed(PointerButton::Extra1) {
        Some(-1)
    } else if (alt && i.key_pressed(Key::ArrowRight))
        || i.pointer.button_pressed(PointerButton::Extra2)
    {
        Some(1)
    } else {
        None
    }
}

impl RepoTab {
    /// What the big area shows now, as a place to come back to. `None` while an editor (the
    /// conflict editor, the interactive rebase) takes it: those are not places of the history.
    pub(super) fn current_view(&self) -> Option<View> {
        if self.conflict.is_some() || self.rebase.is_some() {
            return None;
        }
        if let Some(diff) = self.diff.as_ref().filter(|_| !self.history_beside_diff()) {
            if let DiffKey::Work(file) = &diff.key
                && file.area == git::Area::Conflicted
            {
                // Its editor is still being read.
                return None;
            }
            // A file of the pull request shown belongs to the pull request.
            if let Some(pr) = &self.pr_view {
                return Some(View::Pr(pr.number));
            }
            return Some(View::Diff(diff.key.clone()));
        }
        if let Some(fv) = &self.file_view {
            return Some(View::File {
                path: fv.path.clone(),
                rev: fv.rev.clone(),
                tab: fv.tab,
            });
        }
        if let Some(ci) = &self.ci_view {
            return Some(View::Ci(ci.sha.clone()));
        }
        if let Some(pr) = &self.pr_view {
            return Some(View::Pr(pr.number));
        }
        Some(View::Graph {
            selected: self.selected_hash(),
        })
    }

    /// Notes what the big area shows, after the frame's actions.
    pub(super) fn record_view(&mut self) {
        if let Some(view) = self.current_view() {
            record(&mut self.nav, view);
        }
    }

    /// Goes back (`-1`) or forward (`1`) through the recent views.
    pub(super) fn navigate(&mut self, step: i32, ctx: &egui::Context) {
        if let Some(editor) = self.editor_open() {
            self.status = Some((format!("Finish or close the {editor} first"), true));
            return;
        }
        let Some(view) = self.nav.go(step).cloned() else {
            return;
        };
        self.restore(&view, ctx);
        // What could really be shown (a commit gone from the graph leaves none selected) is
        // the place now, without dropping the places forward of it.
        if let Some(shown) = self.current_view() {
            self.nav.replace_current(shown);
        }
    }

    /// The editor taking the big area, which going elsewhere would throw away.
    fn editor_open(&self) -> Option<&'static str> {
        if self.conflict.is_some() {
            Some("conflict editor")
        } else if self.rebase.is_some() {
            Some("interactive rebase")
        } else {
            None
        }
    }

    /// Shows `view` again through the actions that open it.
    fn restore(&mut self, view: &View, ctx: &egui::Context) {
        match view {
            View::Graph { selected } => {
                for close in [
                    Action::CloseDiff,
                    Action::CloseFileView,
                    Action::CloseCi,
                    Action::ClosePr,
                ] {
                    self.apply(close, ctx);
                }
                if let Some(hash) = selected {
                    self.reselect(hash, ctx);
                }
            }
            View::Diff(key) => {
                self.apply(Action::ClosePr, ctx);
                self.apply(Action::CloseCi, ctx);
                self.apply(Action::CloseFileView, ctx);
                if let Some(gone) = self.vanished_diff(key) {
                    self.apply(Action::CloseDiff, ctx);
                    self.status = Some((gone, true));
                } else if self.diff.as_ref().is_none_or(|d| &d.key != key) {
                    self.apply(Action::OpenDiff(key.clone()), ctx);
                }
            }
            View::File { path, rev, tab } => {
                self.apply(Action::ClosePr, ctx);
                self.apply(Action::CloseCi, ctx);
                if rev.is_none() && self.missing_here(path) {
                    self.status = Some((format!("{path} is no longer in the working tree"), true));
                }
                self.apply(
                    Action::FileView {
                        path: path.clone(),
                        rev: rev.clone(),
                        tab: *tab,
                    },
                    ctx,
                );
            }
            View::Pr(number) => {
                self.apply(Action::CloseCi, ctx);
                if self.pr_view.as_ref().is_some_and(|p| p.number == *number) {
                    self.apply(Action::CloseDiff, ctx);
                    self.apply(Action::CloseFileView, ctx);
                } else {
                    self.apply(Action::OpenPr(*number), ctx);
                }
            }
            View::Ci(sha) => {
                self.apply(Action::CloseDiff, ctx);
                self.apply(Action::CloseFileView, ctx);
                if self.ci_view.as_ref().is_none_or(|c| &c.sha != sha) {
                    self.apply(
                        Action::OpenCi {
                            sha: sha.clone(),
                            run: None,
                        },
                        ctx,
                    );
                }
            }
        }
    }

    /// Selects the commit a graph place had, or says it is gone.
    fn reselect(&mut self, hash: &str, ctx: &egui::Context) {
        match self.index.get(hash).copied() {
            Some(row) => {
                self.apply(Action::Select(row), ctx);
                self.scroll_to = Some(row);
            }
            // The changes row goes once everything is committed: nothing to say.
            None if hash == WIP_HASH => {}
            None => {
                self.status = Some((
                    format!("Commit {} is no longer in the graph", short(hash)),
                    true,
                ))
            }
        }
    }

    /// Why a diff can't be shown again: a change of the working tree that is gone.
    fn vanished_diff(&self, key: &DiffKey) -> Option<String> {
        let DiffKey::Work(file) = key else {
            return None;
        };
        let work = &self.work;
        let still = [&work.staged, &work.unstaged, &work.conflicted]
            .into_iter()
            .flatten()
            .any(|f| f.path == file.path && f.area == file.area);
        (!still).then(|| format!("{} has no such changes any more", file.path))
    }

    /// A file of the working tree that isn't there any more (only known for a repository on
    /// this machine: another one's files aren't looked at from the window's thread).
    fn missing_here(&self, path: &str) -> bool {
        !crate::remote::is_remote(&self.path) && !self.path.join(path).exists()
    }

    /// Back / forward from the keys and the mouse's buttons, unless a dialog has them.
    pub(super) fn nav_shortcuts(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if self.dialog.is_some() || self.keys_blocked {
            return;
        }
        let keys = !ctx.egui_wants_keyboard_input();
        if let Some(step) = ctx.input(|i| nav_step(i, keys)) {
            acts.push(Action::Navigate(step));
        }
    }

    /// The toolbar's back and forward buttons, each naming where it goes.
    pub(super) fn nav_buttons(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        for (step, icon, name, keys) in [
            (-1, "arrow-left", "Back", "Alt+←"),
            (1, "arrow-right", "Forward", "Alt+→"),
        ] {
            let target = self.nav.peek(step);
            let way = name.to_lowercase();
            let tip = match target {
                Some(view) => format!(
                    "{name} to {} ({keys} or the mouse's {way} button)",
                    view.label()
                ),
                None => format!("Nothing to go {way} to ({keys})"),
            };
            if icons::tool_button(ui, icon, "", target.is_some())
                .on_hover_text(&tip)
                .on_disabled_hover_text(&tip)
                .clicked()
            {
                acts.push(Action::Navigate(step));
            }
        }
    }
}

#[cfg(test)]
mod tests;
