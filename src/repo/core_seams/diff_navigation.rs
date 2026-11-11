use super::*;

impl RepoTab {
    /// Shows a diff (or the conflict editor) in the central area; asking for the open one
    /// closes it.
    pub(in crate::repo) fn open_diff(&mut self, key: DiffKey, ctx: &egui::Context) {
        self.open_diff_of_lines(key, None, ctx);
    }

    /// [`Self::open_diff`]; with `lines`, only the part of the commit's diff that changed those
    /// lines (see [`DiffView::lines`]).
    pub(in crate::repo) fn open_diff_of_lines(
        &mut self,
        key: DiffKey,
        lines: Option<Vec<git::LineRange>>,
        ctx: &egui::Context,
    ) {
        if let DiffKey::Work(file) = &key
            && file.area == git::Area::Conflicted
            && self
                .submodules
                .iter()
                .any(|sub| sub.path == file.path && sub.state == git::SubmoduleState::Conflicted)
        {
            self.diff = None;
            self.conflict = None;
            self.repository_tools_act(
                crate::repo::repository_tools::Act::SubmoduleConflict(file.path.clone()),
                ctx,
            );
            return;
        }
        self.tour = None;
        if self
            .diff
            .as_ref()
            .is_some_and(|d| d.key == key && d.lines == lines)
        {
            self.diff = None;
            self.conflict = None;
            return;
        }
        // A new opening: what is still on its way for an earlier one is not for this one.
        self.conflict_generation += 1;
        let generation = self.conflict_generation;
        self.conflict = match &key {
            // Elsewhere, read in the background (see `conflict_opened`).
            DiffKey::Work(file)
                if file.area == git::Area::Conflicted && crate::remote::is_remote(&self.path) =>
            {
                let (repo, path, state) =
                    (self.path.clone(), file.path.clone(), self.work.in_progress);
                let certain = self.rerere.reused.contains(&path);
                // Not rebuilt again when it couldn't be since the last operation.
                let skip = self.rerere.unopenable.contains(&path);
                spawn(&self.tx, ctx, move || {
                    let editor = conflict::ConflictEditor::open(&repo, &path, state);
                    if editor.is_some() || skip {
                        return Msg::ConflictOpened(generation, path, editor.map(Box::new));
                    }
                    // Its markers gone: what stands in their place (rerere's resolution).
                    let found = conflict::rerere::open_reused(&repo, &path, state, certain);
                    Msg::Then(Box::new(move |tab, _| {
                        tab.reused_opened(generation, path, found);
                    }))
                });
                None
            }
            DiffKey::Work(file) if file.area == git::Area::Conflicted => {
                let editor =
                    conflict::ConflictEditor::open(&self.path, &file.path, self.work.in_progress);
                if editor.is_none() {
                    self.open_reused_later(generation, file.path.clone(), ctx);
                }
                editor.map(|mut ed| {
                    ed.generation = generation;
                    ed
                })
            }
            _ => None,
        };
        let mut view = DiffView::new(key);
        view.lines = lines;
        self.diff = Some(view);
        self.refresh_diff(ctx);
    }

    /// The conflict editor of `path`, read in the background for the opening numbered
    /// `generation`, if that is still the latest and its diff the one open: one read for an
    /// opening closed since would replace (or reset) the editor opened after it.
    pub(in crate::repo) fn conflict_opened(
        &mut self,
        generation: u64,
        path: String,
        editor: Option<Box<conflict::ConflictEditor>>,
    ) {
        let open = matches!(&self.diff, Some(DiffView { key: DiffKey::Work(file), .. })
            if file.path == path && file.area == git::Area::Conflicted);
        if open && generation == self.conflict_generation {
            self.conflict = editor.map(|e| {
                let mut ed = *e;
                ed.generation = generation;
                ed
            });
        }
    }

    /// The file of the conflict editor opened as `generation` changed on disk (seen in the
    /// background): opened again from what is there now, if that editor is still the one open.
    pub(in crate::repo) fn conflict_changed(
        &mut self,
        generation: u64,
        path: String,
        ctx: &egui::Context,
    ) {
        if self
            .conflict
            .as_ref()
            .is_none_or(|ed| ed.path != path || ed.generation != generation || ed.saving)
        {
            return;
        }
        let Some(DiffView {
            key: DiffKey::Work(file),
            ..
        }) = &self.diff
        else {
            return;
        };
        let key = DiffKey::Work(file.clone());
        self.diff = None;
        self.conflict = None;
        self.open_diff(key, ctx);
    }

    /// Reads the open diff again, showing the old one meanwhile.
    pub(in crate::repo) fn refresh_diff(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.diff else { return };
        let (first, previous) = (view.request == 0, view.request);
        let pending = previous != view.data_request;
        let request = view.next_request();
        let key = view.key.clone();
        let (active, queued) = crate::tasks::counts().unwrap_or_default();
        crate::timing::log(format_args!(
            "diff#{request} {} {} {} ({}); jobs: {active} running, {queued} waiting",
            if first { "opened" } else { "read again" },
            match &key {
                DiffKey::Work(file) => format!("{:?}", file.area).to_lowercase(),
                DiffKey::Commit { .. } => "commit".into(),
                DiffKey::Range { .. } => "range".into(),
                DiffKey::Remerge { .. } => "remerge".into(),
            },
            crate::timing::kind_of(key.path()),
            if pending && !first {
                format!("diff#{previous} still being read")
            } else {
                "nothing pending".into()
            },
        ));
        let asked = std::time::Instant::now();
        let lines = view.lines.clone();
        let path = self.path.clone();
        let settings = Settings::get(ctx);
        let opts = git::DiffOptions {
            ignore_ws: self.diff_ignore_ws,
            whole_file: settings.diff_whole_file,
            algorithm: diff::algorithm_for(&settings, key.path()),
            no_textconv: false,
        };
        view.algorithm = opts.algorithm;
        let dark = Tokens::get(ctx).dark();
        spawn(&self.tx, ctx, move || {
            let _scope = crate::timing::scope(format!("diff#{request}"));
            crate::timing::step("waited for a worker", asked);
            let started = std::time::Instant::now();
            let result = diff::load_lines(&path, &key, lines.as_deref(), opts, dark).map(Box::new);
            crate::timing::step("read in all", started);
            Msg::Diff(request, result)
        });
        self.load_diff_file(ctx);
        self.load_diff_rendered(ctx);
        self.load_diff_structural(ctx);
    }

    /// Renders the Markdown file of the open diff, when diffs show Markdown rendered.
    pub(in crate::repo) fn load_diff_rendered(&mut self, ctx: &egui::Context) {
        let rendered = self.diff_rendered;
        let Some(view) = self.diff.as_mut().filter(|v| rendered && v.key.renders()) else {
            return;
        };
        view.rendered_load += 1;
        let ids = (view.request, view.rendered_load);
        let (key, path) = (view.key.clone(), self.path.clone());
        spawn(&self.tx, ctx, move || {
            let result = diff::rendered::load(&path, &key);
            Msg::DiffRendered(ids, Box::new(result))
        });
    }

    /// Shows Markdown files rendered in diffs, or as text.
    pub(in crate::repo) fn show_diff_rendered(&mut self, on: bool, ctx: &egui::Context) {
        self.discard_decrypted();
        self.diff_rendered = on;
        if on {
            self.diff_rich = None;
            if let Some(view) = &mut self.diff {
                view.structural.enabled = false;
            }
        }
        // Read again each time it's turned on: the file may have changed since.
        if on {
            self.load_diff_rendered(ctx);
        }
    }

    /// Reads the whole file of the open diff, when diffs show the file.
    pub(in crate::repo) fn load_diff_file(&mut self, ctx: &egui::Context) {
        let Some(view) = self.diff.as_ref().filter(|_| self.diff_as_file) else {
            return;
        };
        let (request, key, path) = (view.request, view.key.clone(), self.path.clone());
        let dark = Tokens::get(ctx).dark();
        spawn(&self.tx, ctx, move || {
            let result = file_text::load_for_diff(&path, &key, dark);
            Msg::DiffFile(request, result)
        });
    }

    /// Shows diffs as the whole file, or as the changes.
    pub(in crate::repo) fn show_diff_as_file(&mut self, on: bool, ctx: &egui::Context) {
        self.discard_decrypted();
        self.diff_as_file = on;
        if on && let Some(view) = &mut self.diff {
            view.structural.enabled = false;
        }
        if self.diff.as_ref().is_some_and(|d| d.file.is_none()) {
            self.load_diff_file(ctx);
        }
    }

    pub(in crate::repo) fn reopen_diff(&mut self, ctx: &egui::Context) {
        if let Some(view) = self.diff.take() {
            self.conflict = None;
            self.open_diff(view.key, ctx);
            // A folded file opened stays open.
            if let Some(reopened) = &mut self.diff {
                reopened.unfolded = view.unfolded;
            }
        }
    }
}

impl RepoTab {
    /// Refresh rich content only when the accepted source changes or is absent.
    pub(in crate::repo) fn diff_arrived(
        &mut self,
        request: u64,
        result: Result<diff::DiffData, String>,
        ctx: &egui::Context,
    ) {
        let mut reread = false;
        if let Some(view) = &mut self.diff {
            // The rich view is read again only when the change itself changed.
            let source = |d: &Option<Result<diff::DiffData, String>>| match d {
                Some(Ok(d)) => Some(d.source),
                _ => None,
            };
            let before = source(&view.data);
            let fresh = request == view.request;
            view.arrived(request, result);
            reread = fresh && (before != source(&view.data) || view.rich.is_none());
        }
        if reread {
            self.load_diff_rich(ctx);
        }
    }
}
