use super::*;

impl RepoTab {
    pub(in crate::repo) fn clear_identity(&mut self, name: bool, email: bool, ctx: &egui::Context) {
        let mut list: Vec<&[&str]> = Vec::new();
        if name {
            list.push(&["config", "--local", "--unset-all", "user.name"]);
        }
        if email {
            list.push(&["config", "--local", "--unset-all", "user.email"]);
        }
        if !list.is_empty() {
            self.run_ops(
                "Use the profile's author",
                ops(&list),
                Refresh::All,
                OpKind::plain(),
                ctx,
            );
        }
    }
    pub(in crate::repo) fn wrap_diff(&mut self, on: bool, ctx: &egui::Context) {
        let mut settings = Settings::get(ctx);
        settings.diff_wrap = on;
        settings.set(ctx);
        if let Some(view) = &mut self.diff {
            view.text_sel = None;
            let logical = crate::repo::diff::logical_wrap_offset(
                view.wrap_offsets.as_deref().map(Vec::as_slice),
                view.scroll,
            );
            view.scroll_to = Some(logical);
            view.wrap_offsets = None;
        }
    }
    pub(in crate::repo) fn step_displayed_diff(&mut self, delta: i32, ctx: &egui::Context) {
        // Shown in place of the lines (not fallen back to them).
        let rich = self.diff_rich_kind().is_some()
            && self
                .diff
                .as_ref()
                .is_some_and(|v| matches!(&v.rich, Some((_, Ok(diff::rich::Rich::Ready(_))))));
        let rendered = self.diff_rendered && self.diff.as_ref().is_some_and(|v| v.key.renders());
        if let Some(view) = self
            .diff
            .as_mut()
            .filter(|v| !self.diff_as_file && !rendered && !rich && v.structural.shows_patch())
        {
            view.step_change(Settings::get(ctx).diff_split, delta);
        }
    }
    pub(in crate::repo) fn open_configured_editor(&mut self, rel: PathBuf, ctx: &egui::Context) {
        let editor = Settings::get(ctx).editor;
        if let Err(e) = open_in_editor(
            &editor,
            &crate::remote::join(&self.path, &rel.to_string_lossy()),
        ) {
            self.status = Some(crate::repo::Notice::plain((e, true)));
        }
    }
    pub(in crate::repo) fn fetch_now(&mut self, ctx: &egui::Context) {
        self.last_fetch = Instant::now();
        self.run_ops(
            "Fetch",
            ops(&[&["fetch", "--all", "--prune"]]),
            Refresh::All,
            OpKind::plain(),
            ctx,
        )
    }
    pub(in crate::repo) fn reveal_branches(
        &mut self,
        section: Option<&'static str>,
        ctx: &egui::Context,
    ) {
        let mut settings = Settings::get(ctx);
        settings.show_branches = true;
        settings.set(ctx);
        match section {
            Some(key) => self.sidebar_reveal = Some(key),
            None => self.focus_branch_filter = true,
        }
    }
    pub(in crate::repo) fn open_pinned_file_line(
        &mut self,
        seed: git::investigations::dossier::Seed,
        ctx: &egui::Context,
    ) -> bool {
        if !git::investigations::oid(&seed.commit) || seed.line == 0 {
            return false;
        }
        crate::repo::file_text::focus(
            ctx,
            ("file_view", &seed.path, Some(&seed.commit)),
            seed.line,
        );
        self.apply(
            Action::FileView {
                path: seed.path,
                rev: Some(seed.commit),
                tab: crate::repo::file_view::FileTab::File,
            },
            ctx,
        );
        true
    }
}
