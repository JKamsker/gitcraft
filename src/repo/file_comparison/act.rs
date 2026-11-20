//! What the raw file comparison panes are asked. The root [`Action`](crate::repo::Action)
//! routes these here in one arm.

use super::Page;
use crate::repo::RepoTab;

pub(in crate::repo) enum ComparisonAct {
    Open(Page),
    Refresh,
    Close,
    Move {
        request: u64,
        index: usize,
    },
    Cell {
        request: u64,
        path: Vec<u8>,
        column: usize,
    },
    Pick {
        request: u64,
        boundary: usize,
        lines: std::collections::BTreeSet<usize>,
        whole: bool,
    },
    NormalPatch,
}

impl RepoTab {
    pub(in crate::repo) fn comparison_act(&mut self, act: ComparisonAct, ctx: &egui::Context) {
        match act {
            ComparisonAct::Open(page) => self.open_file_comparison(page, ctx),
            ComparisonAct::Refresh => self.load_file_comparison(ctx),
            ComparisonAct::Close => self.file_comparison = None,
            ComparisonAct::Move { request, index } => self.comparison_move(request, index, ctx),
            ComparisonAct::Cell {
                request,
                path,
                column,
            } => self.comparison_cell(request, path, column, ctx),
            ComparisonAct::Pick {
                request,
                boundary,
                lines,
                whole,
            } => self.comparison_pick(request, boundary, lines, whole, ctx),
            ComparisonAct::NormalPatch => self.comparison_normal_patch(ctx),
        }
    }
}
