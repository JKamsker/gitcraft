//! What the file view in the central pane is asked: opening a file's history, blame or
//! contents, or the history of some of its lines. The root [`Action`](crate::repo::Action)
//! routes these here in one arm.

use super::FileTab;
use crate::repo::{DiffKey, RepoTab};

pub(crate) enum FileViewAct {
    /// A file's history, blame or contents in the central pane (`rev`: the commit they are at).
    Open {
        path: String,
        rev: Option<String>,
        tab: FileTab,
    },
    Close,
    /// The history of lines `range` of `path` (numbered as they are `at`), in the file view.
    LineHistory {
        path: String,
        at: crate::git::LinesAt,
        range: crate::git::LineRange,
    },
    /// The part of commit `hash`'s diff that changed lines `lines` of `path` (as they are in
    /// the commit); its whole diff of the file when there are none.
    DiffOfLines {
        hash: String,
        path: String,
        lines: Vec<crate::git::LineRange>,
    },
    /// A file's blame (at `rev`, or HEAD and the working tree), scrolled to `line` (1 is the
    /// first) and marking it.
    BlameAt {
        path: String,
        rev: Option<String>,
        line: usize,
    },
}

impl RepoTab {
    pub(in crate::repo) fn file_view_act(&mut self, act: FileViewAct, ctx: &egui::Context) {
        match act {
            FileViewAct::Open { path, rev, tab } => {
                self.open_file_view(path, rev, tab, ctx);
                // A line marked by an earlier `BlameAt` isn't what this one asks for.
                if let Some(view) = &mut self.file_view {
                    view.marked_line = None;
                }
            }
            FileViewAct::Close => self.file_view = None,
            FileViewAct::LineHistory { path, at, range } => {
                self.open_line_history(path, at, range, ctx)
            }
            FileViewAct::DiffOfLines { hash, path, lines } => {
                let lines = (!lines.is_empty()).then_some(lines);
                self.open_diff_of_lines(DiffKey::Commit { hash, path }, lines, ctx);
            }
            FileViewAct::BlameAt { path, rev, line } => {
                self.open_file_view(path, rev, FileTab::Blame, ctx);
                if let Some(view) = &mut self.file_view {
                    view.goto_line.set(Some(line));
                    view.marked_line = Some(line);
                }
            }
        }
    }
}
