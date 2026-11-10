use super::*;

impl RepoTab {
    pub(in crate::repo) fn keyboard_work_files(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let groups = [
            &self.work.conflicted,
            &self.work.unstaged,
            &self.work.staged,
        ];
        let navigating = self.keyboard.borrow().enter
            || (keymap::queued(ui.ctx(), Command::FileNext)
                || keymap::queued(ui.ctx(), Command::FilePrevious)
                || self.keyboard.borrow().step != 0);
        let files: Vec<_> = if navigating && Settings::get(ui.ctx()).files_as_tree {
            groups
                .into_iter()
                .flat_map(|files| {
                    let mut nodes =
                        branch_tree::build(files.iter().map(|file| (file.path.as_str(), file)));
                    branch_tree::compact(&mut nodes);
                    let mut out = Vec::new();
                    let mut levels = vec![nodes.iter()];
                    while let Some(level) = levels.last_mut() {
                        match level.next() {
                            Some(branch_tree::Node::Leaf { item, .. }) => out.push(*item),
                            Some(branch_tree::Node::Folder(folder)) => {
                                levels.push(folder.children.iter())
                            }
                            None => {
                                levels.pop();
                            }
                        }
                    }
                    out
                })
                .collect()
        } else {
            groups.into_iter().flatten().collect()
        };
        self.keyboard_files(
            ui,
            (Id::new("working-files"), None),
            &files,
            |file| file.path.as_str(),
            |file| DiffKey::Work((*file).clone()),
            acts,
        );
    }

    /// Only key presses build a navigation order. Ordinary frames reconcile one selected
    /// identity; collapsed folders are revealed when navigation moves into them.
    pub(in crate::repo) fn keyboard_files<T>(
        &self,
        ui: &mut Ui,
        (scope, folder): (Id, Option<Id>),
        files: &[T],
        path: impl Fn(&T) -> &str,
        key: impl Fn(&T) -> DiffKey,
        acts: &mut Vec<Action>,
    ) {
        self.keyboard_scope();
        let active = self.keyboard.borrow().pane == Some(Pane::Detail) && available(self, ui.ctx());
        let requested = if active {
            std::mem::take(&mut self.keyboard.borrow_mut().step)
        } else {
            0
        };
        let delta = if active && take_key(ui.ctx(), Key::J, true) {
            1
        } else if active && take_key(ui.ctx(), Key::K, true) {
            -1
        } else {
            requested
        };
        let mut state = self.keyboard.borrow_mut();
        let previous = state
            .cursor
            .as_ref()
            .filter(|(old_scope, _)| *old_scope == scope)
            .map(|(_, key)| key.clone());
        let current = previous
            .as_ref()
            .and_then(|old| {
                files
                    .iter()
                    .find(|file| path(file) == old.path() && same_file(&key(file), old))
            })
            .map(&key)
            .or_else(|| {
                // Staging may move a path to the other section; a rename may change its path.
                let DiffKey::Work(old) = previous.as_ref()? else {
                    return None;
                };
                files.iter().find_map(|file| match key(file) {
                    DiffKey::Work(new)
                        if new.path == old.path || new.old_path.as_deref() == Some(&old.path) =>
                    {
                        Some(DiffKey::Work(new))
                    }
                    _ => None,
                })
            });
        let enter = active && state.enter;
        if active {
            state.enter = false;
        }
        let selected = if delta != 0 || enter {
            let mut ordered: Vec<&T> = if folder.is_some() && Settings::get(ui.ctx()).files_as_tree
            {
                let mut nodes = branch_tree::build(files.iter().map(|file| (path(file), file)));
                branch_tree::compact(&mut nodes);
                let mut out = Vec::new();
                let mut levels = vec![nodes.iter()];
                while let Some(level) = levels.last_mut() {
                    match level.next() {
                        Some(branch_tree::Node::Leaf { item, .. }) => out.push(*item),
                        Some(branch_tree::Node::Folder(folder)) => {
                            levels.push(folder.children.iter())
                        }
                        None => {
                            levels.pop();
                        }
                    }
                }
                out
            } else {
                files.iter().collect()
            };
            let at = current.as_ref().and_then(|old| {
                ordered
                    .iter()
                    .position(|file| path(file) == old.path() && same_file(&key(file), old))
            });
            let next = match at {
                Some(i) if delta > 0 => (i + 1).min(ordered.len().saturating_sub(1)),
                Some(i) if delta < 0 => i.saturating_sub(1),
                Some(i) => i,
                None if delta < 0 => ordered.len().saturating_sub(1),
                None => 0,
            };
            ordered.get_mut(next).map(|file| key(file))
        } else {
            current
        };
        state.cursor = selected.clone().map(|key| (scope, key));
        if delta != 0 || enter {
            state.focus_row = true;
        }
        drop(state);
        let Some(selected) = selected else {
            return;
        };
        if delta != 0 || enter {
            let folder = folder.unwrap_or_else(|| {
                let tree = match &selected {
                    DiffKey::Work(f) if f.area == Area::Staged => "staged",
                    DiffKey::Work(f) if f.area == Area::Conflicted => "conflicted",
                    _ => "unstaged",
                };
                Id::new(("file-folder", self.repo_key(), tree))
            });
            let path = selected.path();
            ui.data_mut(|d| {
                for (i, _) in path.match_indices('/') {
                    d.remove::<bool>(folder.with(&path[..i]));
                }
                d.insert_temp(folder.with("keyboard-reveal"), path.to_owned());
            });
        }
        if !active
            || self.busy_op.is_some()
            || self.loading
            || self.work_pending != 0
            || self.load_error.is_some()
        {
            return;
        }
        let rev = match &selected {
            DiffKey::Work(_) => None,
            DiffKey::Commit { hash, .. } | DiffKey::Remerge { hash, .. } => Some(hash.as_str()),
            DiffKey::Range { to, .. } => to.as_deref(),
        };
        crate::repo::widgets::selection_commands(ui.ctx(), selected.path(), rev, acts);
        let open = take_key(ui.ctx(), Key::Enter, false);
        if open || take_key(ui.ctx(), Key::Space, false) {
            acts.push(Action::Keyboard(Request {
                path: self.path.clone(),
                read: self.work_read,
                epoch: self.work_epoch,
                target: Target::File {
                    scope,
                    key: selected,
                    open,
                },
            }));
        }
    }

    pub(in crate::repo) fn keyboard_hunk(&self, ui: &Ui, acts: &mut Vec<Action>) {
        if self.keyboard.borrow().pane != Some(Pane::Graph)
            || !available(self, ui.ctx())
            || self.loading
            || self.busy_op.is_some()
            || self.work_pending != 0
            || self.load_error.is_some()
        {
            return;
        }
        let Some(view) = &self.diff else {
            return;
        };
        let (DiffKey::Work(file), Some(Ok(data)), Some(hunk)) =
            (&view.key, &view.data, view.keyboard_hunk)
        else {
            return;
        };
        if view.data_request != view.request
            || !data.partial
            || view.structural.enabled
            || self.diff_ignore_ws
            || !matches!(file.area, Area::Staged | Area::Unstaged)
        {
            return;
        }
        let pick = if take_key(ui.ctx(), Key::S, false) && file.area == Area::Unstaged {
            Some(diff::Pick::Stage)
        } else if take_key(ui.ctx(), Key::U, false) && file.area == Area::Staged {
            Some(diff::Pick::Unstage)
        } else if take_key(ui.ctx(), Key::D, false) && file.area == Area::Unstaged {
            Some(diff::Pick::Discard)
        } else {
            None
        };
        if let Some(pick) = pick {
            let lines = diff::hunk_changes(&data.lines, hunk);
            if !lines.is_empty() {
                acts.push(Action::Keyboard(Request {
                    path: self.path.clone(),
                    read: self.work_read,
                    epoch: self.work_epoch,
                    target: Target::Hunk {
                        key: view.key.clone(),
                        request: view.request,
                        pick,
                        lines,
                        confirmed: false,
                    },
                }));
            }
        }
    }

    pub(in crate::repo) fn keyboard_step(&self, step: i32) {
        self.keyboard.borrow_mut().step = step;
    }

    /// Capture a dynamic file/hunk target for palette and registry actions. Application still
    /// revalidates the captured read, epoch, file and diff request at execution/confirmation.
    pub(in crate::repo) fn keyboard_command(&self, command: Command) -> Result<Action, String> {
        let missing = || crate::i18n::text("repo-core-2851").to_owned();
        if matches!(command, Command::FileNext | Command::FilePrevious) {
            if self.keyboard.borrow().pane != Some(Pane::Detail) {
                return Err(missing());
            }
            return Ok(Action::KeyboardStep(if command == Command::FileNext {
                1
            } else {
                -1
            }));
        }
        if self.loading || self.work_pending != 0 || self.load_error.is_some() {
            return Err(crate::i18n::text("repo-core-2852").into());
        }
        let target = if matches!(command, Command::FileOpen | Command::FileToggleStage) {
            if self.keyboard.borrow().pane != Some(Pane::Detail) {
                return Err(missing());
            }
            let (scope, key) = self.keyboard.borrow().cursor.clone().ok_or_else(missing)?;
            if command == Command::FileToggleStage
                && !matches!(&key, DiffKey::Work(f) if f.area != Area::Conflicted)
            {
                return Err(crate::i18n::text("repo-core-2853").into());
            }
            Target::File {
                scope,
                key,
                open: command == Command::FileOpen,
            }
        } else {
            let view = self.diff.as_ref().ok_or_else(missing)?;
            let (DiffKey::Work(file), Some(Ok(data)), Some(hunk)) =
                (&view.key, &view.data, view.keyboard_hunk)
            else {
                return Err(missing());
            };
            if view.request != view.data_request
                || !data.partial
                || self.diff_ignore_ws
                || view.structural.enabled
                || self.diff_as_file
                || self.diff_rendered
                || self.diff_rich_kind().is_some()
                || self.conflict.is_some()
                || self.rebase.is_some()
            {
                return Err(crate::i18n::text("repo-core-2854").into());
            }
            let pick = match (command, file.area) {
                (Command::HunkStage, Area::Unstaged) => diff::Pick::Stage,
                (Command::HunkUnstage, Area::Staged) => diff::Pick::Unstage,
                (Command::HunkDiscard, Area::Unstaged) => diff::Pick::Discard,
                _ => {
                    return Err(crate::i18n::text("repo-core-2855").into());
                }
            };
            let lines = diff::hunk_changes(&data.lines, hunk);
            if lines.is_empty() {
                return Err(crate::i18n::text("repo-core-2856").into());
            }
            Target::Hunk {
                key: view.key.clone(),
                request: view.request,
                pick,
                lines,
                confirmed: false,
            }
        };
        Ok(Action::Keyboard(Request {
            path: self.path.clone(),
            read: self.work_read,
            epoch: self.work_epoch,
            target,
        }))
    }
}
