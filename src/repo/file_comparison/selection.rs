//! Pinned comparison selection and guarded mutation dispatch.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn comparison_move(
        &mut self,
        request: u64,
        index: usize,
        ctx: &egui::Context,
    ) {
        let entries = self
            .file_comparison
            .as_ref()
            .filter(|s| s.data_request == request)
            .and_then(|s| s.data.as_ref())
            .and_then(|r| r.as_ref().ok())
            .and_then(|d| match d.as_ref() {
                Data::Migration(m) => m.moves.get(index),
                _ => None,
            })
            .map(|m| {
                (
                    [Some(m.before.clone()), Some(m.after.clone())],
                    [m.before.name(), m.after.name()],
                )
            });
        if let Some((entries, labels)) = entries {
            self.load_comparison_pair(entries, labels, ctx);
        }
    }
    pub(in crate::repo) fn comparison_cell(
        &mut self,
        request: u64,
        path: Vec<u8>,
        column: usize,
        ctx: &egui::Context,
    ) {
        let Some(state) = self
            .file_comparison
            .as_mut()
            .filter(|s| s.data_request == request)
        else {
            return;
        };
        // Every selection transition revokes the old pair, including 2→1 and 1→0.
        // Displayed matrix cells retain their independent data_request ownership.
        state.begin(false);
        state.loading = false;
        let cell = (path, column);
        if let Some(i) = state.cells.iter().position(|c| *c == cell) {
            state.cells.remove(i);
            return;
        }
        if state.cells.len() == 2 {
            state.cells.clear();
        }
        state.cells.push(cell);
        if state.cells.len() != 2 {
            return;
        }
        let Some(Ok(data)) = &state.data else {
            return;
        };
        let Data::Matrix(matrix) = data.as_ref() else {
            return;
        };
        let cells = state
            .cells
            .iter()
            .filter_map(|(path, col)| {
                matrix.columns.get(*col).map(|c| {
                    (
                        c.files.get(path).cloned(),
                        format!(
                            "{} · {} · {}",
                            c.label,
                            &c.oid[..8],
                            files::display_path(path)
                        ),
                    )
                })
            })
            .collect::<Vec<_>>();
        if cells.len() == 2 {
            self.load_comparison_pair(
                [cells[0].0.clone(), cells[1].0.clone()],
                [cells[0].1.clone(), cells[1].1.clone()],
                ctx,
            );
        }
    }
    pub(in crate::repo) fn comparison_pick(
        &mut self,
        request: u64,
        boundary: usize,
        lines: BTreeSet<usize>,
        whole: bool,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(state) = self
            .file_comparison
            .as_ref()
            .filter(|s| !s.loading && s.data_request == request)
        else {
            return;
        };
        let Some(Ok(data)) = &state.data else {
            return;
        };
        let Data::Lens(lens) = data.as_ref() else {
            return;
        };
        let Some(part) = lens.boundaries.get(boundary).filter(|p| p.writable()) else {
            return;
        };
        if lens.snapshot.staging_warning.is_some()
            || (!whole && !lines.is_subset(&part.changed()))
            || (whole && part.old_mode == part.new_mode && part.changed().is_empty())
        {
            return;
        }
        let pick = if boundary == 0 {
            crate::repo::diff::Pick::Unstage
        } else {
            crate::repo::diff::Pick::Stage
        };
        let patch = if whole {
            Some(part.full.clone())
        } else {
            part.patch(&lens.snapshot.spec.path, pick, &lines)
        };
        let Some(patch) = patch else {
            return;
        };
        let snapshot = lens.snapshot.clone();
        let repo = self.path.clone();
        let label = format!("{} {} from staging lens", pick.verb(), snapshot.spec.path);
        self.busy_op = Some(label.clone());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        let failure_label = label.clone();
        spawn_mutation(
            &self.tx,
            ctx,
            move |e| {
                Msg::Then(Box::new(move |tab, ctx| {
                    tab.op_done(
                        failure_label,
                        Err(e),
                        Refresh::Status,
                        OpKind::plain(),
                        false,
                        ctx,
                    );
                }))
            },
            move || {
                let result = files::apply_lens_patch(&repo, &snapshot, &patch, boundary == 0);
                Msg::Then(Box::new(move |tab, ctx| {
                    tab.op_done(label, result, Refresh::Status, OpKind::plain(), false, ctx);
                    if tab
                        .file_comparison
                        .as_ref()
                        .is_some_and(|s| s.page == Page::Lens)
                    {
                        tab.load_file_comparison(ctx);
                    }
                }))
            },
        );
    }
    pub(in crate::repo) fn comparison_normal_patch(&mut self, ctx: &egui::Context) {
        let file = self
            .file_comparison
            .as_ref()
            .and_then(|s| {
                self.work
                    .unstaged
                    .iter()
                    .chain(&self.work.staged)
                    .find(|f| f.path == s.path)
            })
            .cloned();
        self.file_comparison = None;
        self.diff_as_file = false;
        self.diff_rendered = false;
        if let Some(file) = file {
            self.open_diff(DiffKey::Work(file), ctx);
        }
    }
}
