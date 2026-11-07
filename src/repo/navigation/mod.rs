//! Back and forward through the tab's recent views: the graph (with its selected commit), a
//! diff, a file's history, blame or contents, a pull request, a commit's builds. What the big
//! area shows is noted after each frame; going back or forward shows a place again through the
//! same actions that opened it.

use egui::{Key, PointerButton, Ui};

use super::nav_history::NavHistory;
use super::{Action, DiffKey, RepoTab, WIP_HASH, file_view, short};
use crate::git::{self, Area};
use crate::icons;

/// A place the big area showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum View {
    /// The graph, with the commit selected in it (its hash; the changes row is `WIP_HASH`).
    Graph { selected: Option<String> },
    /// A diff, with the commit selected in the graph meanwhile (the commit pane shows it).
    Diff {
        key: DiffKey,
        selected: Option<String>,
    },
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
    /// Another visit to the same place that doesn't need a step of its own: another commit
    /// selected in the graph, or in the graph above the same diff.
    fn refines(&self, current: &View) -> bool {
        match (self, current) {
            (View::Graph { .. }, View::Graph { .. }) => true,
            (View::Diff { key, .. }, View::Diff { key: shown, .. }) => key == shown,
            _ => false,
        }
    }

    /// What the back and forward buttons say they go to. `graph_shown`: the graph is in view
    /// already (above a diff), so going to it means closing the diff.
    pub(super) fn label(&self, graph_shown: bool) -> String {
        match self {
            View::Graph { .. } if graph_shown => "the graph without the diff".to_owned(),
            View::Graph {
                selected: Some(hash),
            } if hash == WIP_HASH => "the graph at the changes".to_owned(),
            View::Graph {
                selected: Some(hash),
            } => format!("the graph at {}", short(hash)),
            View::Graph { selected: None } => "the graph".to_owned(),
            View::Diff { key, .. } => diff_label(key),
            View::File { path, rev, tab } => {
                let what = match tab {
                    file_view::FileTab::History => "the history",
                    file_view::FileTab::Blame => "the blame",
                    file_view::FileTab::File => "the contents",
                };
                let at = rev
                    .as_deref()
                    .map(|r| format!(" at {}", short(r)))
                    .unwrap_or_default();
                format!("{what} of {}{at}", file_name(path))
            }
            View::Pr(number) => format!("pull request #{number}"),
            View::Ci(sha) => format!("the builds of {}", short(sha)),
        }
    }
}

/// A diff as the buttons name it: the file, and where its change comes from.
fn diff_label(key: &DiffKey) -> String {
    let name = file_name(key.path());
    match key {
        DiffKey::Remerge { hash, .. } => format!("{name}, resolution of {}", short(hash)),
        DiffKey::Commit { hash, .. } => format!("{name} in {}", short(hash)),
        DiffKey::Work(file) => match file.area {
            Area::Unstaged => format!("the unstaged changes of {name}"),
            Area::Staged => format!("the staged changes of {name}"),
            Area::Conflicted => format!("the conflict in {name}"),
            Area::Untracked => format!("the new file {name}"),
        },
        DiffKey::Range {
            from, to: Some(to), ..
        } => format!("{name} from {} to {}", short(from), short(to)),
        DiffKey::Range { from, to: None, .. } => {
            format!("{name} from {} to the working tree", short(from))
        }
    }
}

/// Why a change of the working tree can't be shown again, in its area's words.
fn gone_change(file: &git::StatusFile) -> String {
    let path = &file.path;
    match file.area {
        Area::Unstaged => format!("{path} has no unstaged changes any more"),
        Area::Staged => format!("{path} has no staged changes any more"),
        Area::Conflicted => format!("{path} is no longer conflicted"),
        Area::Untracked => format!("{path} is no longer a new file"),
    }
}

/// The last part of a path, as the tooltips name a file.
fn file_name(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(path)
}

/// Notes `view` as what is shown: a new step, or the place shown refined. `jump`: the frame
/// followed a link (a parent, a branch, a tag), which is a step of its own even in the graph.
pub(super) fn record(history: &mut NavHistory<View>, view: View, jump: bool) {
    match history.current() {
        Some(current) if !jump && view.refines(current) => history.replace_current(view),
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
                && file.area == Area::Conflicted
            {
                // Its editor is still being read.
                return None;
            }
            // A file of the pull request shown belongs to the pull request.
            if let Some(pr) = &self.pr_view {
                return Some(View::Pr(pr.number));
            }
            return Some(View::Diff {
                key: diff.key.clone(),
                selected: self.selected_hash(),
            });
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
        let jump = std::mem::take(&mut self.nav_jump);
        if let Some(view) = self.current_view() {
            record(&mut self.nav, view, jump);
        }
    }

    /// Goes back (`-1`) or forward (`1`) through the recent views. Places that are gone (a
    /// change since staged or committed) are passed over, and said so.
    pub(super) fn navigate(&mut self, step: i32, ctx: &egui::Context) {
        if let Some(editor) = self.navigation_editor_open() {
            self.status = Some((format!("Finish or close the {editor} first"), true));
            return;
        }
        let mut passed = None;
        let view = loop {
            let Some(view) = self.nav.go(step).cloned() else {
                break None;
            };
            match self.gone(&view) {
                Some(why) => {
                    passed = Some(why);
                    self.nav.drop_current(step);
                }
                None => break Some(view),
            }
        };
        if let Some(why) = passed {
            self.status = Some((why, false));
        }
        if let Some(view) = view {
            self.restore_navigation(&view, ctx);
        }
        self.nav_jump = false;
        // What could really be shown (a commit gone from the graph leaves another selected)
        // is the place now, without dropping the places forward of it.
        if let Some(shown) = self.current_view() {
            self.nav.replace_current(shown);
        }
    }

    /// What going elsewhere would throw away: an editor taking the big area, or a commit's
    /// message being edited in the commit pane.
    fn navigation_editor_open(&self) -> Option<&'static str> {
        if self.conflict.is_some() {
            Some("conflict editor")
        } else if self.rebase.is_some() {
            Some("interactive rebase")
        } else if self.reword.is_some() {
            Some("commit message being edited")
        } else {
            None
        }
    }

    /// Why a place can't be shown again: a change of the working tree that is gone.
    fn gone(&self, view: &View) -> Option<String> {
        match view {
            View::Diff {
                key: DiffKey::Work(file),
                ..
            } if self.work_file(file).is_none() => Some(gone_change(file)),
            _ => None,
        }
    }

    /// The working tree's change of `file` as it is now (its status letter or old name may
    /// have changed since), if its path still has one in that area.
    fn work_file(&self, file: &git::StatusFile) -> Option<git::StatusFile> {
        let work = &self.work;
        [&work.staged, &work.unstaged, &work.conflicted]
            .into_iter()
            .flatten()
            .find(|f| f.path == file.path && f.area == file.area)
            .cloned()
    }

    /// Closes the pull request, the builds and the file view, those that are open (closing
    /// the pull request closes the diff too, which would read it again).
    fn close_pages(&mut self, file_view: bool, ctx: &egui::Context) {
        if self.pr_view.is_some() {
            self.apply(Action::ClosePr, ctx);
        }
        if self.ci_view.is_some() {
            self.apply(Action::CloseCi, ctx);
        }
        if file_view && self.file_view.is_some() {
            self.apply(Action::CloseFileView, ctx);
        }
    }

    /// Shows `view` again through the actions that open it.
    fn restore_navigation(&mut self, view: &View, ctx: &egui::Context) {
        match view {
            View::Graph { selected } => {
                self.close_pages(true, ctx);
                if self.diff.is_some() {
                    self.apply(Action::CloseDiff, ctx);
                }
                if let Some(hash) = selected {
                    self.reselect(hash, ctx);
                }
            }
            View::Diff { key, selected } => {
                self.close_pages(true, ctx);
                let key = match key {
                    DiffKey::Work(file) => self.work_file(file).map_or(key.clone(), DiffKey::Work),
                    _ => key.clone(),
                };
                if let Some(hash) = selected.as_deref()
                    && self.selected_hash().as_deref() != Some(hash)
                {
                    self.reselect(hash, ctx);
                }
                if self.diff.as_ref().is_none_or(|d| d.key != key) {
                    self.apply(Action::OpenDiff(key), ctx);
                }
            }
            View::File { path, rev, tab } => {
                self.close_pages(false, ctx);
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
                if self.ci_view.is_some() {
                    self.apply(Action::CloseCi, ctx);
                }
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

    /// Selects the commit a place had: further down the history, it is read first; gone
    /// from the graph, that is said.
    fn reselect(&mut self, hash: &str, ctx: &egui::Context) {
        if let Some(&row) = self.index.get(hash) {
            self.apply(Action::Select(row), ctx);
            self.scroll_to = Some(row);
        } else if hash == WIP_HASH {
            // The changes row goes once everything is committed: nothing to say.
        } else if self.history_index.contains_key(hash) || self.has_more {
            self.goto(hash, ctx);
        } else {
            self.status = Some((
                format!("Commit {} is no longer in the graph", short(hash)),
                false,
            ));
        }
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

    /// The toolbar's back and forward buttons, each naming where it goes; small, as the
    /// toolbar is full at the window's narrowest.
    pub(super) fn nav_buttons(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let graph_shown = self.diff_beside_graph(ui.ctx());
        for (step, icon, name, keys) in [
            (-1, "arrow-left", "Back", "Alt+←"),
            (1, "arrow-right", "Forward", "Alt+→"),
        ] {
            let target = self.nav.peek(step);
            let tip = match target {
                Some(view) => format!("{name} to {} ({keys})", view.label(graph_shown)),
                None => format!("Nothing to go {} to ({keys})", name.to_lowercase()),
            };
            let button =
                ui.add_enabled_ui(target.is_some(), |ui| icons::small_button(ui, icon, &tip));
            if button.inner.on_disabled_hover_text(&tip).clicked() {
                acts.push(Action::Navigate(step));
            }
        }
    }
}

#[cfg(test)]
mod tests;
