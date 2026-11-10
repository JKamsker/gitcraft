//! Working-file rows, selection and staging interactions.

use super::*;

impl RepoTab {
    /// Working-tree files, each with `buttons`; `on` handles what was clicked on a row. `tree`:
    /// which list this is, for the folders opened and closed in it. Generated files and
    /// lockfiles are folded at the end.
    pub(in crate::repo::detail::changes) fn work_list(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        tree: &str,
        files: &[StatusFile],
        buttons: &[(&str, &str)],
        mut on: impl FnMut(&mut Vec<Action>, &StatusFile, RowClick),
    ) {
        let id = egui::Id::new(("file-folder", self.repo_key(), tree));
        let rules = folding::Rules::from_settings(&Settings::get(ui.ctx()));
        let group = folding::split(ui, id, files, |f| &f.path, (&self.folding, &rules), acts);
        let selected = |f: &&&StatusFile| {
            self.open_key() == Some(&DiffKey::Work((**f).clone()))
                || self.keyboard_file_selected(&DiffKey::Work((**f).clone()))
        };
        let mut row = |ui: &mut Ui, indent, f: &StatusFile, acts: &mut Vec<Action>| {
            let click = self.work_row(ui, acts, f, indent, buttons);
            let opened = matches!(click, RowClick::Open);
            if matches!(click, RowClick::Pick) {
                acts.push(Action::PickToStash(f.path.clone()));
            } else {
                on(acts, f, click);
            }
            if opened {
                // Clicked: its lines are what was asked for, folded or not.
                acts.push(Action::UnfoldDiff);
            }
        };
        // Rows are known by their path within the list: a file can be both staged and unstaged.
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        let mut reveal = None;
        crate::repo::detail::horizontal_files(ui, ("work-files-scroll", tree), &paths, |ui| {
            ui.push_id(("work-list", tree), |ui| {
                reveal = branch_tree::files(
                    ui,
                    (id, true),
                    &group.shown,
                    |f| &f.path,
                    &selected,
                    &mut |ui, indent, f| row(ui, indent, f, acts),
                )
                .or(reveal);
                let holds_open = group.folded.iter().any(|f| selected(&f));
                if group.header(ui, id.with("folded-open"), None, holds_open) {
                    reveal = branch_tree::files(
                        ui,
                        (id.with("folded"), true),
                        &group.folded,
                        |f| &f.path,
                        &selected,
                        &mut |ui, indent, f| row(ui, indent, f, acts),
                    )
                    .or(reveal);
                }
            });
        });
        // egui consumes both scroll targets at the inner horizontal list. Forward the
        // same typed row rectangle to its outer vertical scroller after that list ends.
        if let Some(rect) = reveal {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
        }
        if let Some(first) = files.first() {
            let area = crate::repo::weakened_marks::area_of(first.area);
            let scope = crate::weakened::Scope::Work(area);
            let found = self
                .weakened
                .get(ui.ctx(), &self.path, scope, self.work_read);
            crate::repo::weakened_marks::error_note(ui, found.as_ref());
        }
    }

    /// One working-tree file with hover buttons (hidden while an operation runs).
    pub(in crate::repo::detail::changes) fn work_row(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        f: &StatusFile,
        indent: Option<f32>,
        buttons: &[(&str, &str)],
    ) -> RowClick {
        let key = DiffKey::Work(f.clone());
        let picked = &self.stash_picks.picked;
        let selected = self.open_key() == Some(&key) || self.keyboard_file_selected(&key);
        let buttons = if self.busy_op.is_none() { buttons } else { &[] };
        let area = crate::repo::weakened_marks::area_of(f.area);
        let scope = crate::weakened::Scope::Work(area);
        let found = self
            .weakened
            .get(ui.ctx(), &self.path, scope, self.work_read);
        let plain = RowExtra::Buttons(buttons);
        let (extra, flags) = crate::repo::weakened_marks::extra(found.as_ref(), &f.path, &plain);
        let r = widgets::file_row_owned(
            ui,
            f.code,
            &f.path,
            f.old_path.as_deref(),
            indent,
            selected,
            extra,
            self.owner_tag(&f.path).as_ref(),
        );
        if picked.contains(&f.path) {
            mark_picked(ui, r.row.rect);
        }
        // A file git can't stash by its name (conflicted, a staged rename or deletion) is
        // neither picked nor offered for stashing, but says why.
        let problem = row_problem(f);
        let stash = |ui: &mut Ui, acts: &mut Vec<Action>| {
            ui.separator();
            stash_menu_items(ui, &f.path, problem.as_deref(), picked, acts);
        };
        self.keyboard_file_row(egui::Id::new("working-files"), &r.row, &key);
        if r.row.clicked() || r.button.is_some() {
            self.keyboard_select_file(egui::Id::new("working-files"), key.clone());
        }
        crate::repo::weakened_marks::hover(ui, &r, flags);
        self.lock_badge(ui, &r.row, &f.path, (!buttons.is_empty(), selected));
        if f.code != 'D' {
            // Linked as it is in the last commit: a new or renamed file isn't there by this name.
            let links = || {
                self.link_menu(None)
                    .filter(|_| !matches!(f.code, 'A' | '?') && f.old_path.is_none())
            };
            widgets::file_menu_with(
                &r.row,
                &f.path,
                None,
                true,
                Some(&key),
                &links,
                self.keyboard_file_selected(&key),
                acts,
                |ui, acts| {
                    self.lock_menu_items(ui, &f.path, None, acts);
                    self.storage_menu_items(ui, &f.path, None, acts);
                    stash(ui, acts);
                },
            );
        } else {
            // A deleted file has nothing to open, but can be stashed.
            crate::menu_item::context(&r.row)
                .style(theme::menu_style)
                .show(|ui| stash_menu_items(ui, &f.path, problem.as_deref(), picked, acts));
        }
        match r.button {
            Some(i) => RowClick::Button(i),
            None if r.row.clicked() && ui.input(|i| i.modifiers.command) => {
                if problem.is_some() {
                    RowClick::None
                } else {
                    RowClick::Pick
                }
            }
            None if r.row.clicked() => RowClick::Open,
            None => RowClick::None,
        }
    }
}
