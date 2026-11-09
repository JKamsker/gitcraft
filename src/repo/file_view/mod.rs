//! Central pane for one file: its history (every commit that changed it, following renames),
//! its blame (who last changed each line) or the whole file as it is at a commit. Opened from a
//! file's right-click menu and from the list of all the files of a commit.

mod blame;
mod history;
mod line_history;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicU64, Ordering};

// Shared across repository tabs, and never reused when a view is closed or replaced.
fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut current = NEXT.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| std::process::abort());
        match NEXT.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(previous) => return previous,
            Err(observed) => current = observed,
        }
    }
}

use egui::{Stroke, Ui};

use super::file_text::{self, FileText};
use super::find::Source;
use super::widgets;
use super::{Action, RepoTab};
use crate::forge::links::{Lines, Target};
use crate::git::{self, FileCommit};
use crate::icons;
use crate::theme::{self, Tokens};

pub(crate) use blame::{BlameData, load_blame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileTab {
    History,
    Blame,
    /// The whole file.
    File,
}

pub(crate) struct FileView {
    pub generation: u64,
    pub path: String,
    /// The commit history, blame and the file are at; HEAD and the working tree when `None`.
    pub rev: Option<String>,
    pub tab: FileTab,
    pub history: Option<Result<Vec<FileCommit>, String>>,
    pub blame: Option<Result<BlameData, String>>,
    /// Blame also depends on configuration, even when file content has a fixed OID.
    pub blame_generation: u64,
    /// A successful previous result remains visible until its replacement arrives.
    pub blame_needs_refresh: bool,
    pub text: Option<Result<FileText, String>>,
    /// The history of only these lines (where their numbers count from, and the lines), not
    /// of the whole file.
    pub lines: Option<(git::LinesAt, git::LineRange)>,
    pub line_inputs: Option<(String, String, git::LineRange)>,
    pub line_input_check: u64,
    history_input: Option<String>,
    text_input: Option<String>,
    observed_inputs: Option<FileInputs>,
    input_check: u64,
    /// A line to scroll to once the blame is read, and to mark (1 is the first).
    pub goto_line: std::cell::Cell<Option<usize>>,
    pub marked_line: Option<usize>,
}

#[derive(Clone, Default)]
pub(super) struct FileInputs {
    history: Option<String>,
    text: Option<String>,
}

pub(super) fn content_key(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn file_inputs(repo: &std::path::Path, rev: Option<&str>, path: &str) -> FileInputs {
    FileInputs {
        history: git::resolve(repo, rev.filter(|rev| !rev.is_empty()).unwrap_or("HEAD")).ok(),
        text: git::file_at(repo, rev, path)
            .ok()
            .map(|bytes| content_key(&bytes)),
    }
}

impl FileView {
    pub fn new(path: &str, rev: Option<String>, tab: FileTab) -> Self {
        Self {
            generation: next_generation(),
            path: path.to_owned(),
            rev,
            tab,
            history: None,
            blame: None,
            blame_generation: next_generation(),
            blame_needs_refresh: false,
            text: None,
            lines: None,
            line_inputs: None,
            line_input_check: 0,
            history_input: None,
            text_input: None,
            observed_inputs: None,
            input_check: 0,
            goto_line: Default::default(),
            marked_line: None,
        }
    }

    /// Invalidate references that can move, including cached failures. A full object ID
    /// identifies immutable content; abbreviations and revision expressions do not.
    pub fn invalidate_mutable(&mut self) -> bool {
        if self.rev.as_deref().is_some_and(git::investigations::oid) {
            return false;
        }
        self.generation = next_generation();
        self.history_input = None;
        self.text_input = None;
        self.observed_inputs = None;
        self.input_check = next_generation();
        self.history = None;
        self.invalidate_blame();
        self.text = None;
        true
    }

    fn invalidate_blame(&mut self) {
        self.blame_generation = next_generation();
        self.blame_needs_refresh = true;
        if !matches!(self.blame, Some(Ok(_))) {
            self.blame = None;
        }
    }

    fn reuses_line_inputs(
        &self,
        inputs: &Result<(String, String, git::LineRange), String>,
    ) -> bool {
        matches!(self.history, Some(Ok(_)))
            && inputs.is_ok()
            && inputs.as_ref().ok() == self.line_inputs.as_ref()
    }

    /// Whether `tab` still has to be loaded.
    pub fn needs(&self, tab: FileTab) -> bool {
        match tab {
            FileTab::History => self.history.is_none(),
            FileTab::Blame => self.blame.is_none() || self.blame_needs_refresh,
            FileTab::File => self.text.is_none(),
        }
    }
}

impl RepoTab {
    /// Refresh the visible pane now; other panes load when selected.
    /// Probe the actual line mapping in the background before rerunning expensive history.
    pub(super) fn status_changed_file_view(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.file_view else {
            return;
        };
        let Some((at, range)) = view.lines.clone() else {
            if view.rev.as_deref().is_some_and(git::investigations::oid) {
                self.invalidate_mutable_file_view(ctx);
                return;
            }
            let generation = view.generation;
            let check = next_generation();
            view.input_check = check;
            let path = view.path.clone();
            let rev = view.rev.clone();
            let repo = self.path.clone();
            self.then(
                ctx,
                move || file_inputs(&repo, rev.as_deref(), &path),
                move |tab, inputs, ctx| {
                    tab.file_status_inputs(generation, check, inputs, ctx);
                },
            );
            return;
        };
        if view.rev.as_deref().is_some_and(git::investigations::oid) {
            return;
        }
        let generation = view.generation;
        let path = view.path.clone();
        let requested_path = path.clone();
        let repo = self.path.clone();
        let check = next_generation();
        view.line_input_check = check;
        self.then(
            ctx,
            move || git::line_history_key(&repo, &at, &path, range),
            move |tab, inputs, ctx| {
                let Some(view) = &mut tab.file_view else {
                    return;
                };
                if view.generation != generation
                    || view.path != requested_path
                    || view.line_input_check != check
                {
                    return;
                }
                // Errors cannot establish unchanged inputs; retry the normal read.
                if view.reuses_line_inputs(&inputs) {
                    return;
                }
                tab.invalidate_mutable_file_view(ctx);
            },
        );
    }

    fn file_status_inputs(
        &mut self,
        generation: u64,
        check: u64,
        inputs: FileInputs,
        ctx: &egui::Context,
    ) {
        let Some(view) = &mut self.file_view else {
            return;
        };
        if view.generation != generation || view.input_check != check {
            return;
        }
        let history_changed = view
            .history_input
            .as_ref()
            .is_some_and(|old| inputs.history.as_ref() != Some(old))
            || view
                .history
                .as_ref()
                .is_some_and(|result| result.is_err() || view.history_input.is_none());
        let text_changed = view
            .text_input
            .as_ref()
            .is_some_and(|old| inputs.text.as_ref() != Some(old))
            || view
                .text
                .as_ref()
                .is_some_and(|result| result.is_err() || view.text_input.is_none());
        if history_changed || text_changed {
            self.invalidate_mutable_file_view(ctx);
            return;
        }
        // A request that has not replied yet compares its own captured inputs with
        // this observation upon arrival. Unrelated probes do not supersede it.
        view.observed_inputs = Some(inputs);
        view.invalidate_blame();
        if view.tab == FileTab::Blame {
            self.load_file_view(ctx);
        }
    }

    pub(super) fn accept_file_inputs(
        &mut self,
        generation: u64,
        path: &str,
        rev: Option<&str>,
        tab: FileTab,
        input: Option<String>,
        ctx: &egui::Context,
    ) -> bool {
        let Some(view) = &mut self.file_view else {
            return false;
        };
        if view.generation != generation || view.path != path || view.rev.as_deref() != rev {
            return false;
        }
        let observed = view.observed_inputs.as_ref().map(|inputs| match tab {
            FileTab::History => &inputs.history,
            FileTab::File => &inputs.text,
            FileTab::Blame => unreachable!("blame uses its independent epoch"),
        });
        if observed.is_some_and(|expected| expected != &input) {
            self.invalidate_mutable_file_view(ctx);
            return false;
        }
        match tab {
            FileTab::History => view.history_input = input,
            FileTab::File => view.text_input = input,
            FileTab::Blame => unreachable!("blame uses its independent epoch"),
        }
        true
    }

    pub(super) fn invalidate_mutable_file_view(&mut self, ctx: &egui::Context) {
        if let Some(view) = &mut self.file_view {
            if !view.invalidate_mutable() {
                // Full OIDs pin content, but blame.ignoreRevsFile can change attribution.
                view.invalidate_blame();
            }
            self.load_file_view(ctx);
        }
    }

    pub(super) fn file_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &self.file_view else { return };
        if view.lines.is_some() {
            self.line_history_view(ui, acts);
            return;
        }

        self.intent_badge(ui, &view.path);
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "history", 16.0, t.icon);
                    ui.label(egui::RichText::new(&view.path).font(theme::medium(13.0)));
                    if let Some(rev) = &view.rev {
                        widgets::chip(
                            ui,
                            &crate::i18n::message(
                                "repo-core-2494",
                                &[("rev", rev.get(..8).unwrap_or(rev).to_string().into())],
                            ),
                            t.text_dim,
                        );
                    }
                    ui.add_space(12.0);
                    let tabs = [
                        (FileTab::History, crate::i18n::text("repo-core-2497")),
                        (FileTab::Blame, crate::i18n::text("repo-core-2498")),
                        (FileTab::File, crate::i18n::text("repo-core-2368")),
                    ];
                    for (tab, label) in tabs {
                        let on = view.tab == tab;
                        let b = ui.add(
                            egui::Button::new(egui::RichText::new(label).color(if on {
                                t.text
                            } else {
                                t.text_dim
                            }))
                            .frame_when_inactive(on),
                        );
                        if on {
                            widgets::active_underline(ui, b.rect, t.accent);
                        }
                        if b.clicked() && !on {
                            acts.push(Action::FileView {
                                path: view.path.clone(),
                                rev: view.rev.clone(),
                                tab,
                            });
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", crate::i18n::text("repo-core-0711"))
                            .clicked()
                        {
                            acts.push(Action::CloseFileView);
                        }
                    });
                });
            });

        // A diff beside the history has its own bar.
        if self.diff.is_none()
            && let Some(what) = self.find_in()
        {
            self.find.bar(ui, what.name());
        }
        let Some(view) = &self.file_view else { return };
        match view.tab {
            FileTab::History => self.file_history(ui, acts),
            FileTab::Blame => self.blame_lines(ui, acts),
            FileTab::File => {
                // Lines link to the commit the file is at; on disk, they may be in none.
                let rev = view.rev.clone();
                let menu = self.link_menu(None);
                let mut link = |ui: &mut Ui, lines: Lines| {
                    if let Some(rev) = &rev
                        && crate::git::investigations::oid(rev)
                        && ui.button(crate::i18n::text("repo-core-2325")).clicked()
                    {
                        acts.push(Action::OpenLineDossier(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: rev.clone(),
                                path: view.path.clone(),
                                line: lines.first as usize,
                            },
                        )));
                    }
                    if let Some(rev) = &rev
                        && crate::git::investigations::oid(rev)
                        && ui.button(crate::i18n::text("repo-core-2326")).clicked()
                    {
                        acts.push(Action::OpenSymbolBiography(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: rev.clone(),
                                path: view.path.clone(),
                                line: lines.first as usize,
                            },
                        )));
                    }
                    if let (Some(rev), Some(menu)) = (&rev, menu.clone()) {
                        let target = Target::File {
                            rev: rev.clone(),
                            path: view.path.clone(),
                            lines: Some(lines),
                        };
                        menu.items(ui, &lines.label(), &target, acts);
                    }
                };
                // Lines are picked only when there's something to link them to.
                let links = view
                    .rev
                    .is_some()
                    .then_some(&mut link as file_text::LineMenu<'_>);
                let picked = file_text::show(
                    ui,
                    ("file_view", &view.path, &view.rev),
                    view.text.as_ref(),
                    &mut self.find,
                    || Source::File {
                        path: view.path.clone(),
                        rev: view.rev.clone(),
                    },
                    Ok(()),
                    links,
                    Some((&view.goto_line, view.marked_line)),
                );
                if let Some(range) = picked {
                    let at = view
                        .rev
                        .clone()
                        .map_or(git::LinesAt::WorkTree, git::LinesAt::Rev);
                    acts.push(Action::LineHistory {
                        path: view.path.clone(),
                        at,
                        range,
                    });
                }
            }
        }
    }
}

fn error(ui: &mut Ui, e: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.colored_label(
            t.danger,
            crate::git::display_error(&crate::forge::inbox::display_error(e)),
        );
    });
}
