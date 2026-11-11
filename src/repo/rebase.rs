//! Interactive rebase: the commits after a base, newest on top like the graph, each picked,
//! reworded, squashed, fixed up, dropped or stopped at to be edited, and reordered (by drag
//! and drop or with the keyboard, see [`rows`]). Starting it writes git's todo list and hands
//! it over as `sequence.editor`; rewords become an `exec git commit --amend -F`.

mod rows;
mod weather;
mod weather_git;

#[path = "core_seams/rebase_todo.rs"]
mod rebase_todo;
#[allow(unused_imports)]
pub(crate) use rebase_todo::*;

use std::path::{Path, PathBuf};

use egui::{Stroke, Ui};

use super::widgets;
use super::{Action, RepoTab};
use crate::git::RebaseCommit;
use crate::icons;
use crate::theme::{self, Tokens};

/// A rebase's command, and the branches it moves along: (their commit now, the ref).
pub(crate) type Moving = (Vec<String>, Vec<(String, String)>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Pick,
    Reword,
    Squash,
    Fixup,
    Drop,
    /// Stop after the commit, to amend it (`edit`): Commit in the changes pane goes on.
    Edit,
}

impl Step {
    const ALL: [Step; 6] = [
        Step::Pick,
        Step::Reword,
        Step::Edit,
        Step::Squash,
        Step::Fixup,
        Step::Drop,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Step::Pick => crate::i18n::text("repo-core-3200"),
            Step::Reword => crate::i18n::text("repo-core-3201"),
            Step::Squash => crate::i18n::text("repo-core-2444"),
            Step::Fixup => crate::i18n::text("repo-core-2446"),
            Step::Drop => crate::i18n::text("repo-core-3197"),
            Step::Edit => crate::i18n::text("repo-core-1116"),
        }
    }

    fn explain(self) -> &'static str {
        match self {
            Step::Pick => crate::i18n::text("repo-core-3202"),
            Step::Reword => crate::i18n::text("repo-core-3203"),
            Step::Squash => crate::i18n::text("repo-core-3204"),
            Step::Fixup => crate::i18n::text("repo-core-3205"),
            Step::Drop => crate::i18n::text("repo-core-3206"),
            Step::Edit => crate::i18n::text("repo-core-3207"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Entry {
    pub commit: RebaseCommit,
    pub step: Step,
    /// The new message, for rewords.
    pub message: String,
}

pub(crate) struct RebaseEditor {
    /// What the commits are replayed onto (a commit or a branch); empty to rewrite from the
    /// root commit on (`--root`).
    pub onto: String,
    pub onto_label: String,
    /// Oldest first, as git lists them.
    pub entries: Vec<Entry>,
    /// HEAD when the commits were read: the todo list replaces the branch's commits with
    /// these, so one made since (a commit, a pull) would be dropped.
    pub head: String,
    /// Which editor this is (each one opened has its own): a rebase prepared for one closed
    /// since never starts.
    pub id: u64,
    /// Its rebase is being prepared (on another machine, in the background): it can't be
    /// started again, nor changed, meanwhile.
    pub starting: bool,
    /// The row the keys act on (an index into `entries`): the newest, until another is
    /// clicked.
    pub selected: Option<usize>,
    /// How tall each row (by commit) was when last drawn: rows out of sight only take their
    /// place.
    pub heights: std::collections::HashMap<String, f32>,
    /// A text field had the keyboard at the end of the last frame: the Esc that takes it away
    /// (egui does, before the frame) doesn't also close the editor.
    pub typing: bool,
    /// Every commit is made again (`--no-ff`), even one already on its parent: commits a
    /// rewrite prepared with `commit-tree` are then signed and hooked like any other.
    pub no_ff: bool,
    /// Move the branches on the commits rewritten along with them (`--update-refs`): the lower
    /// layers of a stack rebased from its top.
    pub update_refs: bool,
    /// Captured when the editor is opened; approval never rebinds it to a later
    /// branch at the same commit. None keeps non-UI command builders compatible.
    pub branch_guard: Option<Vec<String>>,
    approval: Option<Approval>,
    weather: weather::Weather,
}

#[derive(Clone, PartialEq, Eq)]
struct Approval {
    branch: Vec<String>,
    head: String,
    onto: String,
    no_ff: bool,
    update_refs: bool,
    entries: Vec<(String, Step, String)>,
}

impl Clone for RebaseEditor {
    fn clone(&self) -> Self {
        Self {
            onto: self.onto.clone(),
            onto_label: self.onto_label.clone(),
            entries: self.entries.clone(),
            head: self.head.clone(),
            id: self.id,
            starting: self.starting,
            selected: self.selected,
            heights: self.heights.clone(),
            typing: self.typing,
            no_ff: self.no_ff,
            update_refs: self.update_refs,
            branch_guard: self.branch_guard.clone(),
            approval: self.approval.clone(),
            weather: weather::Weather::default(),
        }
    }
}

impl RebaseEditor {
    pub fn new(onto: String, onto_label: String, commits: Vec<RebaseCommit>) -> Self {
        static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        // `git log --reverse onto..HEAD`: HEAD comes last.
        let head = commits.last().map(|c| c.hash.clone()).unwrap_or_default();
        // The newest row, at the top, takes the keys until another is clicked.
        let selected = commits.len().checked_sub(1);
        let entries = commits
            .into_iter()
            .map(|c| Entry {
                message: c.message.clone(),
                commit: c,
                step: Step::Pick,
            })
            .collect();
        Self {
            onto,
            onto_label,
            entries,
            head,
            id: IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            starting: false,
            selected,
            heights: std::collections::HashMap::new(),
            typing: false,
            no_ff: false,
            update_refs: false,
            branch_guard: None,
            approval: None,
            weather: weather::Weather::default(),
        }
    }

    fn approval_key(&self, branch: &[String]) -> Approval {
        Approval {
            branch: branch.to_vec(),
            head: self.head.clone(),
            onto: self.onto.clone(),
            no_ff: self.no_ff,
            update_refs: self.update_refs,
            entries: self
                .entries
                .iter()
                .map(|e| (e.commit.hash.clone(), e.step, e.message.clone()))
                .collect(),
        }
    }
    pub fn approve(&mut self, branch: &[String]) -> Result<(), String> {
        if self
            .branch_guard
            .as_ref()
            .is_some_and(|original| original != branch)
        {
            return Err(crate::i18n::text("repo-core-3208").into());
        }
        self.branch_guard = Some(branch.to_vec());
        self.approval = Some(self.approval_key(branch));
        Ok(())
    }
    pub fn approved(&self, branch: &[String], head: &str) -> bool {
        self.head == head
            && self.branch_guard.as_deref() == Some(branch)
            && self
                .approval
                .as_ref()
                .is_some_and(|key| key == &self.approval_key(branch))
    }
    pub fn has_approval(&self) -> bool {
        self.approval.is_some()
    }

    /// Why the commits listed are no longer the branch's, when HEAD is at `head` now.
    pub fn moved(&self, head: &str) -> Option<String> {
        (!self.head.is_empty() && self.head != head)
            .then(|| crate::i18n::text("repo-core-3209").to_owned())
    }

    /// Why the rebase can't start as set up, if it can't.
    pub fn problem(&self) -> Option<String> {
        if self.entries.iter().any(|e| e.commit.merge) {
            return Some(crate::i18n::text("repo-core-3210").to_owned());
        }
        let mut kept = self.entries.iter().filter(|e| e.step != Step::Drop);
        match kept.next() {
            None => {
                return Some(crate::i18n::text("repo-core-3211").to_owned());
            }
            Some(e) if matches!(e.step, Step::Squash | Step::Fixup) => {
                return Some(crate::i18n::message(
                    "repo-core-3212",
                    &[
                        ("label", e.step.label().to_string().into()),
                        ("hash", short(&e.commit.hash).to_string().into()),
                    ],
                ));
            }
            _ => {}
        }
        if self
            .entries
            .iter()
            .any(|e| e.step == Step::Reword && e.message.trim().is_empty())
        {
            return Some(crate::i18n::text("repo-core-3213").to_owned());
        }
        None
    }

    fn changed(&self, e: &Entry) -> bool {
        e.step == Step::Reword && e.message.trim_end() != e.commit.message.trim_end()
    }

    /// Writes the todo list and the new messages to a fresh temporary folder and returns the
    /// git command that runs the rebase with them. For a repository in WSL or over SSH, on
    /// that machine, where git reads them.
    pub fn command(&self, repo: &Path) -> Result<Vec<String>, String> {
        self.command_with_refs(repo).map(|(command, _)| command)
    }

    /// [`Self::command`], and the branches Update dependent branches moves along: (their
    /// commit now, `refs/heads/<branch>`).
    pub fn command_with_refs(&self, repo: &Path) -> Result<Moving, String> {
        if let Some(target) = crate::remote::parse(repo) {
            return self.command_there(repo, &target);
        }
        let dir = private_dir(&std::env::temp_dir()).map_err(|e| {
            crate::i18n::message("repo-core-3214", &[("e", format!("{}", e).into())])
        })?;
        let write = |name: &str, text: &str| -> Result<PathBuf, String> {
            let path = dir.join(name);
            std::fs::write(&path, text).map_err(|e| {
                crate::i18n::message("repo-core-3214", &[("e", format!("{}", e).into())])
            })?;
            Ok(path)
        };
        let mut files = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            files.push(if self.changed(e) {
                Some(write(&format!("message-{i}.txt"), &e.message)?)
            } else {
                None
            });
        }
        let refs = self.moved_refs(repo)?;
        let todo = todo(
            &self.entries,
            |i| files.get(i).cloned().flatten().map(|p| sh_path(&p)),
            &refs,
        );
        let todo_file = write("todo.txt", &todo)?;
        Ok((self.rebase_args(&sh_path(&todo_file)), refs))
    }

    /// [`Self::command`] for a repository on another machine: the files in a private folder
    /// there.
    fn command_there(&self, repo: &Path, target: &crate::remote::Target) -> Result<Moving, String> {
        let write = |name: &str, text: &str| {
            crate::remote::host_temp_file(target, name, text.as_bytes())
                .map_err(|e| crate::i18n::message("repo-core-3214", &[("e", e.to_string().into())]))
        };
        let mut files = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            files.push(if self.changed(e) {
                Some(write(&format!("rebase-message-{i}.txt"), &e.message)?)
            } else {
                None
            });
        }
        let refs = self.moved_refs(repo)?;
        let todo = todo(
            &self.entries,
            |i| {
                files
                    .get(i)
                    .cloned()
                    .flatten()
                    .map(|p| crate::remote::sh_quote(&p))
            },
            &refs,
        );
        let todo_file = write("rebase-todo.txt", &todo)?;
        Ok((self.rebase_args(&crate::remote::sh_quote(&todo_file)), refs))
    }

    /// The branches moved along with the commits, when Update dependent branches is ticked.
    fn moved_refs(&self, repo: &Path) -> Result<Vec<(String, String)>, String> {
        if !self.update_refs {
            return Ok(Vec::new());
        }
        let commits: Vec<&str> = self
            .entries
            .iter()
            .map(|e| e.commit.hash.as_str())
            .collect();
        dependent_refs(repo, &commits)
    }

    /// The rebase, its todo list copied from `todo_file` (quoted for `sh`).
    fn rebase_args(&self, todo_file: &str) -> Vec<String> {
        let mut args = vec![
            "-c".to_owned(),
            format!("sequence.editor=cp {todo_file}"),
            "rebase".to_owned(),
            "-i".to_owned(),
            "--autostash".to_owned(),
        ];
        if self.update_refs {
            args.push("--update-refs".to_owned());
        }
        if self.no_ff {
            args.push("--no-ff".to_owned());
        }
        if self.onto.is_empty() {
            args.push("--root".to_owned());
        } else {
            args.extend(["--end-of-options".to_owned(), self.onto.clone()]);
        }
        args
    }
}

impl RepoTab {
    pub(super) fn rebase_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        self.poll_rebase_weather(ui.ctx());
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let branch = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        let head = self.refs.head.clone();
        let keys_free =
            self.dialog.is_none() && !self.keys_blocked && !ui.ctx().egui_wants_keyboard_input();
        let Some(editor) = &mut self.rebase else {
            return;
        };

        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "git-merge", 16.0, t.icon);
                    ui.label(
                        egui::RichText::new(crate::i18n::message(
                            "repo-core-3236",
                            &[
                                ("branch", branch.to_string().into()),
                                ("onto-label", editor.onto_label.to_string().into()),
                            ],
                        ))
                        .font(theme::medium(13.0)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", crate::i18n::text("repo-core-3237"))
                            .clicked()
                        {
                            acts.push(Action::CloseRebase);
                        }
                        if icons::small_button(
                            ui,
                            "refresh-cw",
                            crate::i18n::text("repo-core-3238"),
                        )
                        .clicked()
                        {
                            editor.weather.refresh(ui.ctx());
                        }
                    });
                });
            });

        let problem = editor.problem().or_else(|| editor.moved(&head));
        egui::Panel::bottom("rebase_footer")
            .frame(
                egui::Frame::new()
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .stroke(Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| {
                // An option of the rebase, above what starts it.
                ui.add_enabled(
                    !editor.starting,
                    egui::Checkbox::new(
                        &mut editor.update_refs,
                        crate::i18n::text("repo-core-3240"),
                    ),
                )
                .on_hover_text(crate::i18n::text("repo-core-3241"));
                ui.add_space(4.0);
                // The buttons first, from the right: the text takes what is left, cut short.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(
                        ui,
                        crate::i18n::text("repo-core-3242"),
                        idle && problem.is_none() && !editor.starting,
                        120.0,
                    )
                    .clicked()
                    {
                        acts.push(Action::RunRebase);
                    }
                    if ui.button(crate::i18n::text("repo-core-0443")).clicked() {
                        acts.push(Action::CloseRebase);
                    }
                    if editor.starting {
                        ui.add(crate::spinner::Spinner::new());
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let note = match &problem {
                            Some(p) => egui::RichText::new(p).color(t.danger),
                            None => {
                                let n = editor.entries.len();
                                egui::RichText::new(crate::i18n::message(
                                    "repo-core-3243",
                                    &[
                                        ("n", (n).into()),
                                        (
                                            "Undo",
                                            crate::keymap::label(
                                                ui.ctx(),
                                                crate::keymap::Command::Undo,
                                            )
                                            .to_string()
                                            .into(),
                                        ),
                                    ],
                                ))
                                .color(t.text_dim)
                            }
                        };
                        ui.add(egui::Label::new(note).truncate())
                            .on_hover_text(keys_hint(ui.ctx()));
                    });
                });
            });

        // The row keys act on this frame, unless something else has the keyboard.
        let keys = editor.keys(ui, keys_free);
        editor.list(ui, keys);
        editor.weather.observe(&editor.entries, ui.ctx());
        editor.typing = ui.ctx().memory(|m| m.focused().is_some());
    }
}

#[cfg(test)]
#[path = "direct_tests/rebase_tests.rs"]
mod tests;
