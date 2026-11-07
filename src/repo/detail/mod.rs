//! Right pane: the selected commit, or the working-tree changes with staging and the commit box.

mod all_files;
mod changes;
mod commit;
mod commit_box;
mod compare;
pub(super) mod compose;
mod extras;
mod stash_picks;

pub(super) use all_files::AllFiles;
pub(super) use changes::{discard_dialog, stage_action};
pub(super) use extras::Extras;

use egui::{Align2, Sense, Ui, pos2, vec2};

use super::branch_tree;
use super::widgets::{self, RowExtra};
use super::{Action, DiffKey, DiffView, RepoTab, folding};
use crate::git::ChangedFile;
use crate::settings::Settings;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn detail_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let wip = self.selected.is_some_and(|r| self.is_wip(r));
        if self.pr_view.is_some() {
            self.pr_files_panel(ui, acts);
        } else if self.compare.is_some() {
            self.compare_panel(ui, acts);
        } else if wip {
            self.changes_panel(ui, acts);
        } else {
            self.commit_panel(ui, acts);
        }
    }

    fn open_key(&self) -> Option<&DiffKey> {
        self.diff.as_ref().map(|d: &DiffView| &d.key)
    }

    /// Changed files, each opening its diff (`key`) when clicked; `rev`: where the files are,
    /// for their menu (`None`: the working tree); `tree`: which list this is, for the folders
    /// opened and closed in it. Generated files and lockfiles are folded at the end.
    pub(super) fn file_list(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        tree: &str,
        files: &[ChangedFile],
        rev: Option<&str>,
        key: impl Fn(&str) -> DiffKey,
    ) {
        let id = egui::Id::new(("file-folder", self.repo_key(), tree));
        let rules = folding::Rules::from_settings(&Settings::get(ui.ctx()));
        let group = folding::split(
            ui,
            id.with(rev),
            files,
            |f| &f.path,
            (&self.folding, &rules),
            acts,
        );
        let selected = |f: &&&ChangedFile| self.open_key() == Some(&key(&f.path));
        let (scope, generation) = super::weakened_marks::scope_of(&key(""), self.work_read);
        let found = self.weakened.get(ui.ctx(), &self.path, scope, generation);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        self.owners_summary(ui, &paths);
        let row = |ui: &mut Ui, indent, f: &ChangedFile, acts: &mut Vec<Action>| {
            let stats = RowExtra::Stats(f.added, f.removed);
            let (extra, flags) = super::weakened_marks::extra(found.as_ref(), &f.path, &stats);
            let r = widgets::file_row_owned(
                ui,
                f.status,
                &f.path,
                f.old_path.as_deref(),
                indent,
                self.open_key() == Some(&key(&f.path)),
                extra,
                self.owner_tag(&f.path).as_ref(),
            );
            super::weakened_marks::hover(ui, &r, flags);
            // A file the commit deleted isn't in it, and has nothing to link to.
            let links = || self.link_menu(None).filter(|_| f.status != 'D');
            widgets::file_menu(
                &r.row,
                &f.path,
                rev,
                false,
                Some(&key(&f.path)),
                &links,
                acts,
            );
            if r.row.clicked() {
                acts.push(Action::OpenDiff(key(&f.path)));
                acts.push(Action::UnfoldDiff);
            }
        };
        // Rows are known by their path within the list of this commit (or range): a menu left
        // open while another one is shown is not that one's file of the same path.
        ui.push_id(("file-list", tree, rev), |ui| {
            branch_tree::files(
                ui,
                (id, true),
                &group.shown,
                |f| &f.path,
                &selected,
                &mut |ui, indent, f| row(ui, indent, f, acts),
            );
            let stats = group.folded.iter().fold((0u32, 0u32), |(a, d), f| {
                (
                    a.saturating_add(f.added.unwrap_or(0)),
                    d.saturating_add(f.removed.unwrap_or(0)),
                )
            });
            let holds_open = group.folded.iter().any(|f| selected(&f));
            let open_id = id.with(("folded-open", rev));
            if group.header(ui, open_id, Some(stats), holds_open) {
                branch_tree::files(
                    ui,
                    (id.with("folded"), true),
                    &group.folded,
                    |f| &f.path,
                    &selected,
                    &mut |ui, indent, f| row(ui, indent, f, acts),
                );
            }
        });
        super::weakened_marks::error_note(ui, found.as_ref());
    }
}

/// "N files changed +a −d", and an optional note on the right.
pub(super) fn files_header(ui: &mut Ui, files: &[ChangedFile], note: Option<&str>) {
    let t = Tokens::get(ui.ctx());
    let (adds, dels) = files.iter().fold((0, 0), |(a, d), f| {
        (a + f.added.unwrap_or(0), d + f.removed.unwrap_or(0))
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.tab_strip);
    let y = rect.center().y;
    let r = p.text(
        pos2(rect.left() + 14.0, y),
        Align2::LEFT_CENTER,
        format!(
            "{} file{} changed",
            files.len(),
            if files.len() == 1 { "" } else { "s" }
        ),
        theme::semibold(11.5),
        t.text_dim,
    );
    let r = p.text(
        pos2(r.right() + 10.0, y),
        Align2::LEFT_CENTER,
        format!("+{adds}"),
        theme::mono(11.0),
        t.success,
    );
    p.text(
        pos2(r.right() + 6.0, y),
        Align2::LEFT_CENTER,
        format!("−{dels}"),
        theme::mono(11.0),
        t.danger,
    );
    if let Some(note) = note {
        p.text(
            pos2(rect.right() - 10.0, y),
            Align2::RIGHT_CENTER,
            note,
            egui::FontId::proportional(11.0),
            t.text_faint,
        );
    }
}
