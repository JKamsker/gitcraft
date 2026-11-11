use super::*;

impl RepoTab {
    /// Stage, unstage or mark resolved for the whole file, and the buttons for the lines
    /// clicked in the diff. True when "Clear" was clicked.
    pub(super) fn file_actions(
        &self,
        ui: &mut Ui,
        view: &DiffView,
        file: &StatusFile,
        partial: bool,
        acts: &mut Vec<Action>,
    ) -> bool {
        let idle = self.busy_op.is_none();
        let add = || ops(&[&["--literal-pathspecs", "add", "--", &file.path]]);
        let (label, steps) = match file.area {
            Area::Staged => (
                crate::i18n::text("repo-core-2369"),
                vec![unstage_args(&self.refs.head, &file.path)],
            ),
            Area::Conflicted => (crate::i18n::text("repo-core-0179"), add()),
            _ => (crate::i18n::text("repo-core-2370"), add()),
        };
        if ui.add_enabled(idle, egui::Button::new(label)).clicked() {
            acts.push(Action::Ops {
                label: label.to_owned(),
                ops: steps,
                refresh: Refresh::Status,
            });
        }
        if file.area == Area::Conflicted
            && ui
                .add_enabled(
                    idle && self.keep_both.is_none(),
                    egui::Button::new(crate::i18n::text("repo-core-2371")),
                )
                .on_hover_text(crate::i18n::text("repo-core-2372"))
                .on_disabled_hover_text(crate::i18n::text("repo-core-2373"))
                .clicked()
        {
            acts.push(Action::KeepBoth(file.path.clone()));
        }
        // The lines clicked in the diff.
        let chosen = view.chosen.len();
        if !partial || chosen == 0 {
            return false;
        }
        ui.add_space(8.0);

        let picks: &[Pick] = if file.area == Area::Staged {
            &[Pick::Unstage]
        } else {
            &[Pick::Stage, Pick::Discard]
        };
        for &pick in picks {
            let b = egui::Button::new(crate::i18n::message(
                "repo-core-2376",
                &[
                    ("verb", pick.verb().to_string().into()),
                    ("chosen", (chosen).into()),
                ],
            ));
            if ui.add(b).clicked() {
                acts.push(Action::PickLines {
                    pick,
                    lines: view.chosen.clone(),
                });
            }
        }
        ui.button(crate::i18n::text("repo-core-1149"))
            .on_hover_text(crate::i18n::text("repo-core-2377"))
            .clicked()
    }

    /// Revert the commit's changes to the whole file or, once lines are clicked in its diff
    /// (`lines`: they show), those lines. True when "Clear" was clicked.
    pub(super) fn commit_actions(
        &self,
        ui: &mut Ui,
        view: &DiffView,
        (hash, path): (&str, &str),
        lines: bool,
        acts: &mut Vec<Action>,
    ) -> bool {
        let idle = self.busy_op.is_none();
        let chosen = view.chosen.len();
        let revertable = matches!(&view.data, Some(Ok(d)) if d.revertable);
        // Selected lines take the place of the file action. Whole-file revert remains
        // available in the file name's menu while the selection is active.
        if lines && revertable && chosen > 0 {
            let b = ui
                .add_enabled(idle, |ui: &mut Ui| {
                    icons::small_button(
                        ui,
                        "undo-2",
                        &crate::i18n::message(
                            "dev-integration-ui-revert-selected",
                            &[("count", chosen.into())],
                        ),
                    )
                })
                .on_disabled_hover_text(crate::i18n::text("repo-core-1125"))
                .on_hover_text(crate::i18n::text("repo-core-2379"));
            if b.clicked() {
                acts.push(Action::PickLines {
                    pick: Pick::Revert,
                    lines: view.chosen.clone(),
                });
            }
            return ui
                .button(crate::i18n::text("repo-core-1149"))
                .on_hover_text(crate::i18n::text("repo-core-2377"))
                .clicked();
        }
        let b = ui
            .add_enabled(idle, |ui: &mut Ui| {
                icons::small_button(ui, "undo-2", crate::i18n::text("repo-core-2380"))
            })
            .on_disabled_hover_text(crate::i18n::text("repo-core-1125"))
            .on_hover_text(crate::i18n::text("repo-core-2381"));
        if b.clicked() {
            acts.push(Action::RevertFile {
                hash: hash.to_owned(),
                path: path.to_owned(),
            });
        }
        false
    }

    /// "abc1234 → def5678" for a diff between two commits, with the names of the comparison.
    pub(super) fn range_label(&self, from: &str, to: &Option<String>) -> String {
        let name = |rev: &str| {
            self.compare
                .as_ref()
                .and_then(|c| c.name_of(rev))
                .or_else(|| self.pr_name_of(rev))
                .unwrap_or_else(|| short(rev).to_owned())
        };
        match to {
            Some(to) => format!("{} → {}", name(from), name(to)),
            None => {
                crate::i18n::message("repo-core-2382", &[("from", name(from).to_string().into())])
            }
        }
    }
    /// Header: file, where it comes from, options, actions, close. `opts`: (side by side,
    /// ignore whitespace, whole file, the file instead of the diff); `views`: for a Markdown
    /// file, whether it shows rendered; the rich views of the file, the one on, and whether it
    /// is shown in place of the lines. True when the picked lines are to be cleared.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn diff_header(
        &self,
        ui: &mut Ui,
        view: &DiffView,
        (path, origin): (&str, &str),
        partial: bool,
        (split, ignore_ws, whole_file, as_file, folded): (bool, bool, bool, bool, bool),
        (markdown, rich_kinds, rich_on, rich_ready): (
            Option<bool>,
            &[rich::RichKind],
            Option<rich::RichKind>,
            bool,
        ),
        acts: &mut Vec<Action>,
    ) -> bool {
        let rendered = markdown == Some(true) || rich_ready;
        let t = Tokens::get(ui.ctx());
        let mut clear = false;
        self.intent_badge(ui, path);
        self.reproducer_badge(ui, &view.key);
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                // Reserve fixed actions first. Neither the path nor a changing selection can
                // move the Diff / File pair away from the right edge.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", crate::i18n::text("repo-core-0711"))
                            .clicked()
                        {
                            acts.push(Action::CloseDiff);
                        }
                        popout::button(ui, Panel::Diff, !self.docked(Panel::Diff), acts);
                        if icons::toggle_button(
                            ui,
                            "book-open",
                            crate::i18n::text("repo-core-2368"),
                            as_file,
                        )
                        .clicked()
                        {
                            acts.push(Action::DiffAsFile(true));
                        }
                        if icons::toggle_button(
                            ui,
                            "file-diff",
                            crate::i18n::text("repo-core-2367"),
                            !as_file,
                        )
                        .clicked()
                        {
                            acts.push(Action::DiffAsFile(false));
                        }
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width().max(0.0));
                            let title = ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new(path).font(theme::medium(13.0)),
                                    )
                                    .truncate()
                                    .sense(Sense::click()),
                                )
                                .on_hover_text(path);
                            let (rev, ignorable) = match &view.key {
                                DiffKey::Commit { hash, .. } | DiffKey::Remerge { hash, .. } => {
                                    (Some(hash.as_str()), false)
                                }
                                DiffKey::Range { to: Some(to), .. } => (Some(to.as_str()), false),
                                DiffKey::Range { to: None, .. } => (None, false),
                                DiffKey::Work(_) => (None, true),
                            };
                            // Linked as it is in the last commit: a new or renamed file isn't there by
                            // this name. Compared with the working tree, the file shown is in no commit.
                            let links = || {
                                self.link_menu(None).filter(|_| match &view.key {
                                    DiffKey::Work(f) => {
                                        !matches!(f.code, 'A' | '?') && f.old_path.is_none()
                                    }
                                    DiffKey::Range { to: None, .. } => false,
                                    DiffKey::Commit { .. }
                                    | DiffKey::Range { .. }
                                    | DiffKey::Remerge { .. } => true,
                                })
                            };
                            widgets::file_menu_with(
                                &title,
                                path,
                                rev,
                                ignorable,
                                (!view.structural.enabled).then_some(&view.key),
                                &links,
                                false,
                                acts,
                                |ui, acts| {
                                    self.lock_menu_items(ui, path, rev, acts);
                                    self.storage_menu_items(ui, path, rev, acts);
                                },
                            );
                            let mut source = origin.to_owned();
                            if let Some(ranges) = &view.lines {
                                source.push_str(&format!(" · {}", lines_chip(ranges)));
                            }
                            let before = if markdown == Some(true) {
                                matches!(&view.rendered, Some(Ok(md)) if md.before)
                            } else {
                                matches!(&view.file, Some(Ok(f)) if f.before)
                            };
                            if as_file && before {
                                source.push_str(" · ");
                                source.push_str(crate::i18n::text("repo-core-2360"));
                            }
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&source).small().color(t.text_dim),
                                )
                                .truncate(),
                            )
                            .on_hover_text(source);
                        });
                    });
                });
                let mut controls = |ui: &mut Ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if let DiffKey::Work(file) = &view.key
                            && !view.structural.enabled
                            && !view.encrypted.enabled
                        {
                            let partial = partial && !as_file && !rendered;
                            clear = self.file_actions(ui, view, file, partial, acts);
                        }
                        if let DiffKey::Commit { hash, path } = &view.key
                            && view.lines.is_none()
                            && self.stash_view.is_none()
                            && !view.structural.enabled
                            && !view.encrypted.enabled
                        {
                            let lines = !as_file && !rendered;
                            clear = self.commit_actions(ui, view, (hash, path), lines, acts);
                        }
                        ui.add_space(8.0);
                        if !as_file && !rendered && !folded && !rich_ready && view.lines.is_none() {
                            if matches!(view.key, DiffKey::Work(_))
                                && ui.button(crate::i18n::text("repo-core-2361")).clicked()
                            {
                                acts.push(Action::OpenFileComparison(
                                    crate::repo::file_comparison::Page::Lens,
                                ));
                            }
                            if !matches!(view.key, DiffKey::Remerge { .. })
                                && ui.button(crate::i18n::text("repo-core-2362")).clicked()
                            {
                                acts.push(Action::OpenFileComparison(
                                    crate::repo::file_comparison::Page::CodeMoves,
                                ));
                            }
                            structural::toggle(ui, view.structural.enabled, acts);
                            ui.add_space(4.0);
                        }
                        // A folded diff shows no lines to arrange.
                        if !as_file
                            && !rendered
                            && !folded
                            && !view.encrypted.enabled
                            && view.structural.shows_patch()
                        {
                            // A diff of some lines only (`git log -L`) shows them whole, with
                            // whitespace: those options don't apply to it.
                            wrap_control(ui, acts);
                            if view.lines.is_none() {
                                view_options(ui, &t, (split, ignore_ws, whole_file), acts);
                                ui.add_space(4.0);
                                super::super::algorithm::algorithm_menu(
                                    ui,
                                    path,
                                    view.repairing,
                                    acts,
                                );
                                ui.add_space(8.0);
                            }
                            change_steps(ui, acts);
                            ui.add_space(8.0);
                        }
                        if self.split_toggle(ui) {
                            ui.add_space(8.0);
                        }
                        for &kind in rich_kinds.iter().rev() {
                            let on = rich_on == Some(kind);
                            let r = icons::small_button(ui, kind.icon(), kind.tip(on));
                            if on {
                                widgets::active_underline(ui, r.rect, t.accent);
                            }
                            if r.clicked() {
                                acts.push(Action::DiffRich((!on).then_some(kind)));
                            }
                        }
                        if markdown.is_some() {
                            let tip = match (markdown == Some(true), as_file) {
                                (true, _) => crate::i18n::text("repo-core-2363"),
                                (false, false) => crate::i18n::text("repo-core-2364"),
                                (false, true) => crate::i18n::text("repo-core-2365"),
                            };
                            let r = icons::small_button(ui, "eye", tip);
                            if markdown == Some(true) {
                                widgets::active_underline(ui, r.rect, t.accent);
                            }
                            if r.clicked() {
                                acts.push(Action::DiffRendered(markdown != Some(true)));
                            }
                            ui.add_space(4.0);
                        }
                    });
                };
                if ui.available_width() < 620.0 {
                    let button = icons::small_button(
                        ui,
                        "settings",
                        crate::i18n::text("dev-integration-ui-diff-options"),
                    );
                    egui::Popup::menu(&button)
                        .style(theme::menu_style)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .width(280.0)
                        .show(|ui| controls(ui));
                } else {
                    controls(ui);
                }
            });
        clear
    }

    /// Why the diff shown starts folded, while it does (a conflicted file never does, nor one
    /// opened from a file list). Asks what `.gitattributes` says of it the first time.
    pub(super) fn fold_reason(
        &self,
        view: &DiffView,
        settings: &Settings,
        acts: &mut Vec<Action>,
    ) -> Option<folding::FoldReason> {
        let conflicted = matches!(&view.key, DiffKey::Work(f) if f.area == Area::Conflicted);
        let rules = folding::Rules::from_settings(settings);
        // A diff that failed to read shows why, folded or not.
        let failed = matches!(view.data, Some(Err(_)));
        if view.unfolded || conflicted || failed || !rules.on {
            return None;
        }
        let path = view.key.path();
        let unknown = self.folding.unknown([path]);
        if !unknown.is_empty() {
            acts.push(Action::FoldAttributes(unknown));
        }
        let reason = self.folding.reason(&rules, path);
        if reason.is_none() && matches!(view.data, Some(Ok(_))) {
            // Its lines show: they stay, rather than fold away under the reader when an
            // answer about its attributes comes (the first one, or one read again).
            acts.push(Action::UnfoldDiff);
        }
        reason
    }

    /// The file shown and where its diff comes from.
    pub(in crate::repo) fn diff_origin(&self, key: &DiffKey) -> (String, String) {
        match key {
            DiffKey::Commit { hash, path } => (
                path.clone(),
                crate::i18n::message(
                    "repo-core-2355",
                    &[("hash", hash.get(..8).unwrap_or(hash).to_string().into())],
                ),
            ),
            DiffKey::Work(file) => (
                file.path.clone(),
                match file.area {
                    Area::Staged => crate::i18n::text("repo-core-2356").to_owned(),
                    Area::Unstaged => crate::i18n::text("repo-core-2357").to_owned(),
                    Area::Untracked => crate::i18n::text("repo-core-2358").to_owned(),
                    Area::Conflicted => crate::i18n::text("repo-core-2359").to_owned(),
                },
            ),
            DiffKey::Range { from, to, path } => (path.clone(), self.range_label(from, to)),
            DiffKey::Remerge { hash, path, .. } => (path.clone(), remerge::origin(hash)),
        }
    }
}
