//! Central pane for one file: its history (every commit that changed it, following renames),
//! its blame (who last changed each line) or the whole file as it is at a commit. Opened from a
//! file's right-click menu and from the list of all the files of a commit.

mod blame;
mod history;
mod line_history;

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
    pub path: String,
    /// The commit history, blame and the file are at; HEAD and the working tree when `None`.
    pub rev: Option<String>,
    pub tab: FileTab,
    pub history: Option<Result<Vec<FileCommit>, String>>,
    pub blame: Option<Result<BlameData, String>>,
    pub text: Option<Result<FileText, String>>,
    /// The history of only these lines (where their numbers count from, and the lines), not
    /// of the whole file.
    pub lines: Option<(git::LinesAt, git::LineRange)>,
    /// A line to scroll to once the blame is read, and to mark (1 is the first).
    pub goto_line: std::cell::Cell<Option<usize>>,
    pub marked_line: Option<usize>,
}

impl FileView {
    pub fn new(path: &str, rev: Option<String>, tab: FileTab) -> Self {
        Self {
            path: path.to_owned(),
            rev,
            tab,
            history: None,
            blame: None,
            text: None,
            lines: None,
            goto_line: Default::default(),
            marked_line: None,
        }
    }

    /// Whether `tab` still has to be loaded.
    pub fn needs(&self, tab: FileTab) -> bool {
        match tab {
            FileTab::History => self.history.is_none(),
            FileTab::Blame => self.blame.is_none(),
            FileTab::File => self.text.is_none(),
        }
    }
}

impl RepoTab {
    pub(super) fn file_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &self.file_view else { return };
        if view.lines.is_some() {
            self.line_history_view(ui, acts);
            return;
        }

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
                            &format!("at {}", rev.get(..8).unwrap_or(rev)),
                            t.text_dim,
                        );
                    }
                    ui.add_space(12.0);
                    let tabs = [
                        (FileTab::History, "History"),
                        (FileTab::Blame, "Blame"),
                        (FileTab::File, "File"),
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
                        if icons::small_button(ui, "x", "Close (Esc)").clicked() {
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
                        && ui.button("Explain history").clicked()
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
                        && ui.button("Symbol biography").clicked()
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
        ui.colored_label(t.danger, e);
    });
}
