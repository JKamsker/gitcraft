use super::*;

impl RepoTab {
    /// Shows `path`'s history or blame, loading what that tab needs.
    pub(in crate::repo) fn open_file_view(
        &mut self,
        path: String,
        rev: Option<String>,
        tab: file_view::FileTab,
        ctx: &egui::Context,
    ) {
        self.tour = None;
        self.diff = None;
        self.conflict = None;
        let view = match self.file_view.take() {
            // Not one showing the history of some lines only.
            Some(mut v) if v.path == path && v.rev == rev && v.lines.is_none() => {
                v.tab = tab;
                v
            }
            _ => file_view::FileView::new(&path, rev.clone(), tab),
        };
        self.file_view = Some(view);
        self.load_file_view(ctx);
    }

    pub(in crate::repo) fn load_file_view(&mut self, ctx: &egui::Context) {
        let Some(view) = &self.file_view else { return };
        if !view.needs(view.tab) {
            return;
        }
        if view.lines.is_some() {
            self.load_line_history(ctx);
            return;
        }
        let (generation, path, rev, tab) = (
            view.generation,
            view.path.clone(),
            view.rev.clone(),
            view.tab,
        );
        let blame_generation = view.blame_generation;
        let Some(token) = self.begin_file_load(tab) else {
            return;
        };
        let repo = self.path.clone();
        match tab {
            file_view::FileTab::History => {
                let requested_path = path.clone();
                let requested_rev = rev.clone();
                self.then_or(
                    ctx,
                    false,
                    Err,
                    move || {
                        let input = git::resolve(
                            &repo,
                            rev.as_deref()
                                .filter(|rev| !rev.is_empty())
                                .unwrap_or("HEAD"),
                        );
                        let result = input
                            .as_ref()
                            .map_err(Clone::clone)
                            .and_then(|oid| git::file_history(&repo, Some(oid), &path));
                        Ok((input.ok(), result))
                    },
                    move |tab, outcome, ctx| {
                        if !tab.finish_file_load(
                            file_view::FileTab::History,
                            token,
                            generation,
                            outcome.as_ref().err().map(String::as_str),
                            ctx,
                        ) {
                            return;
                        }
                        let (input, result) = outcome.unwrap_or_else(|error| (None, Err(error)));
                        if result.is_err()
                            || tab.accept_file_inputs(
                                generation,
                                &requested_path,
                                requested_rev.as_deref(),
                                file_view::FileTab::History,
                                input,
                                ctx,
                            )
                        {
                            tab.handle(
                                Msg::FileHistory(generation, requested_path, requested_rev, result),
                                ctx,
                            );
                        }
                    },
                );
            }
            file_view::FileTab::Blame => {
                let dark = Tokens::get(ctx).dark();
                let requested_path = path.clone();
                let requested_rev = rev.clone();
                self.then_or(
                    ctx,
                    false,
                    Err,
                    move || {
                        let input = file_view::blame_input(&repo, rev.as_deref(), &path);
                        let result = file_view::load_blame(&repo, rev.as_deref(), &path, dark);
                        Ok((input, result))
                    },
                    move |tab, outcome, ctx| {
                        if !tab.finish_file_load(
                            file_view::FileTab::Blame,
                            token,
                            blame_generation,
                            outcome.as_ref().err().map(String::as_str),
                            ctx,
                        ) {
                            return;
                        }
                        let (input, result) = outcome.unwrap_or_else(|error| (None, Err(error)));
                        if result.is_err() || tab.accept_blame_input(input, ctx) {
                            tab.handle(
                                Msg::Blame(blame_generation, requested_path, requested_rev, result),
                                ctx,
                            );
                        }
                    },
                );
            }
            file_view::FileTab::File => {
                let dark = Tokens::get(ctx).dark();
                let requested_path = path.clone();
                let requested_rev = rev.clone();
                self.then_or(
                    ctx,
                    false,
                    Err,
                    move || {
                        let bytes = git::file_at(&repo, rev.as_deref(), &path);
                        let input = bytes
                            .as_ref()
                            .ok()
                            .map(|bytes| file_view::content_key(bytes));
                        let result =
                            bytes.map(|bytes| file_text::FileText::new(&bytes, &path, dark));
                        Ok((input, result))
                    },
                    move |tab, outcome, ctx| {
                        if !tab.finish_file_load(
                            file_view::FileTab::File,
                            token,
                            generation,
                            outcome.as_ref().err().map(String::as_str),
                            ctx,
                        ) {
                            return;
                        }
                        let (input, result) = outcome.unwrap_or_else(|error| (None, Err(error)));
                        if result.is_err()
                            || tab.accept_file_inputs(
                                generation,
                                &requested_path,
                                requested_rev.as_deref(),
                                file_view::FileTab::File,
                                input,
                                ctx,
                            )
                        {
                            tab.handle(
                                Msg::FileText(generation, requested_path, requested_rev, result),
                                ctx,
                            );
                        }
                    },
                );
            }
        }
    }

    /// Reads every file of commit `hash`, for the commit pane's "View all files".
    pub(in crate::repo) fn load_tree_files(&mut self, hash: String, ctx: &egui::Context) {
        self.all_files.wait_for(&hash);
        let repo = self.path.clone();
        spawn(&self.tx, ctx, move || {
            let result = git::tree_files(&repo, &hash);
            Msg::TreeFiles(hash, result)
        });
    }
}
