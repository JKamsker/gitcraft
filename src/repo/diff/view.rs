//! The diff pane: its header (file, origin, actions, view options), what shows while there are
//! no lines, and the comment panel for a pull request's file.

use egui::{Sense, Stroke, Ui};

use super::binary::{binary_view, textconv_note};
use super::image_compare::compare_view;
use super::images::image_view;
use super::lfs::lfs_view;
use super::link::LineLinks;
use super::notebook::notebook_bar;
use super::remerge;
use super::rows::Rows;
use super::structural;
use super::{DiffData, Kind, Pick, old_name, picked_in_file, unstage_args};
use super::{rendered, rich};
use crate::forge::links::Target;
use crate::git::{Area, StatusFile};
use crate::icons;
use crate::pull_request::DiffSide;
use crate::repo::find;
use crate::repo::popout::{self, Panel};
use crate::repo::pr_review::{self, PrDraft};
use crate::repo::pr_view::{self, PrView};
use crate::repo::{
    Action, DiffKey, DiffView, Refresh, RepoTab, file_text, folding, ops, short, widgets,
};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(in crate::repo) fn diff_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let settings = Settings::get(ui.ctx());
        if let Some(view) = &mut self.diff {
            // Back where it was, read another way: in the layout shown now.
            if let (Some(Ok(data)), Some(place)) = (&view.data, view.restore) {
                let split = data.shows_split(settings.diff_split);
                view.scroll_to = Some(super::offset_of_place(data, split, place));
                view.restore = None;
            }
            // The choice for its file type changed elsewhere (another tab, the settings).
            let algorithm = super::algorithm_for(&settings, view.key.path());
            if view.algorithm != algorithm {
                acts.push(Action::SetDiffAlgorithm(algorithm));
            }
        }
        let Some(view) = &self.diff else { return };
        let idle = self.busy_op.is_none();

        let (path, origin) = self.diff_origin(&view.key);
        let partial =
            matches!(&view.data, Some(Ok(d)) if d.partial) && idle && !view.structural.enabled;
        // Lines and hunks can be picked: to stage, unstage or discard, or to revert.
        // A stash's changes aren't a commit's to revert.
        let revert = self.stash_view.is_none();
        let pickable = matches!(&view.data, Some(Ok(d)) if d.partial || d.revertable && revert)
            && idle
            && !view.structural.enabled;
        let (split, ignore_ws, whole_file) = (
            settings.diff_split,
            self.diff_ignore_ws,
            settings.diff_whole_file,
        );
        let as_file = self.diff_as_file;
        // A Markdown file can show rendered: `Some(whether it does)`.
        let markdown = view.key.renders().then_some(self.diff_rendered);
        let folded = if as_file || markdown == Some(true) {
            None
        } else {
            self.fold_reason(view, &settings, acts)
        };
        // The rich views the file has, and the one shown (not with the file, nor rendered).
        let rich_kinds = self.rich_kinds_of(&path);
        let rich_on = self
            .diff_rich_kind()
            .filter(|_| !as_file && markdown != Some(true));
        let rich_ready =
            rich_on.is_some() && !matches!(&view.rich, Some((_, Ok(rich::Rich::Plain(_)))));
        let clear = self.diff_header(
            ui,
            view,
            (&path, &origin),
            partial && !rich_ready,
            (split, ignore_ws, whole_file, as_file, folded.is_some()),
            (markdown, &rich_kinds, rich_on, rich_ready),
            acts,
        );

        if markdown == Some(true) {
            let Some(view) = &mut self.diff else { return };
            match &view.rendered {
                None => {
                    ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
                }
                Some(Err(e)) => {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.colored_label(t.danger, e);
                    });
                }
                Some(Ok(md)) => rendered::show(ui, &path, md, &mut view.markdown, !as_file),
            }
            return;
        }
        if let Some(what) = self.find_in() {
            self.find.bar(ui, what.name());
        }
        if as_file {
            let before = matches!(&view.file, Some(Ok(f)) if f.before);
            let old_path = match &view.data {
                Some(Ok(d)) => old_name(&d.header),
                _ => None,
            };
            let rev = sides(&view.key).map(|(new, old)| if before { old } else { new });
            let link_menu = self.link_menu(None);
            let menu = || link_menu.clone();
            let mut link = |ui: &mut Ui, lines| {
                if let (Some(rev), Some(menu)) = (&rev, menu()) {
                    let target = Target::File {
                        rev: rev.clone(),
                        path: path.clone(),
                        lines: Some(lines),
                    };
                    menu.items(ui, &lines.label(), &target, acts);
                }
            };
            // Lines are picked only when there's something to link them to.
            let links = (rev.is_some() && self.link_menu(None).is_some())
                .then_some(&mut link as file_text::LineMenu<'_>);
            let pick = |range| picked_in_file(&view.key, old_path.as_deref(), before, range);
            let history = pick((1, 1))
                .map(|_| ())
                .ok_or("Untracked and conflicted files have no line history");
            // Lines picked in one version aren't those of another one of the same file.
            let id = ("diff_file", &path, format!("{:?}", view.key));
            if let Some(p) = file_text::show(
                ui,
                id,
                view.file.as_ref(),
                &mut self.find,
                || find::Source::DiffFile(path.clone()),
                history,
                links,
            )
            .and_then(pick)
            {
                acts.push(Action::LineHistory {
                    path: p.path,
                    at: p.at,
                    range: p.range,
                });
            }
            return;
        }
        if let Some(kind) = rich_on {
            let Some(view) = &mut self.diff else { return };
            match &view.rich {
                None => {
                    ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
                    return;
                }
                Some((_, Err(e))) => {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.colored_label(t.danger, e);
                    });
                    return;
                }
                Some((_, Ok(rich::Rich::Ready(diff)))) => {
                    rich::show(ui, diff, &mut view.rich_state, acts);
                    return;
                }
                Some((_, Ok(rich::Rich::Plain(note)))) => rich::fallback_note(ui, kind, note),
            }
        }
        let Some(view) = &self.diff else { return };
        if let Some(reason) = folded {
            let data = match &view.data {
                Some(Ok(data)) => Some(data),
                _ => None,
            };
            if folding::folded_diff(ui, &path, data, reason) {
                acts.push(Action::UnfoldDiff);
            }
            return;
        }
        if view.structural.enabled {
            match &view.structural.result {
                None => {
                    ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
                    return;
                }
                Some(Ok(data)) => {
                    structural::show(ui, data);
                    return;
                }
                Some(Err(reason)) => {
                    ui.colored_label(t.warning, reason);
                    ui.label("Showing the patch. Turn off Syntax to stage changes.");
                    ui.separator();
                }
            }
        }
        let Some(data) = ready_data(ui, view, ignore_ws, &t, acts) else {
            return;
        };

        // A file of the pull request shown: its lines take comments.
        // Any other diff: its lines take notes for the coding agent.
        let review = self
            .diff_review(&view.key)
            .or_else(|| Some(self.note_review(&path, &origin)));
        if review
            .as_ref()
            .is_some_and(|r| r.target.is_some() && r.local)
        {
            crate::repo::agents::note_panel(ui, &t, &mut self.agents, data, &origin, acts);
        } else if review.as_ref().is_some_and(|r| r.target.is_some())
            && let Some(pr) = &mut self.pr_view
        {
            let draft = self.pr_drafts.entry(pr.number).or_default();
            line_comment_panel(ui, &t, pr, draft, data, acts);
        }
        // Lines commented on in the pull request stay in view, their fold opened (once each,
        // so a fold that can't open isn't asked again every frame).
        let mut auto_opened = None;
        if let (Some(review), Some(ctx)) = (&review, &data.context) {
            let folded = review.marks.keys().find_map(|&(side, line)| {
                let line = u32::try_from(line).ok()?;
                let old = side == crate::pull_request::DiffSide::Left;
                if view.auto_opened.contains(&(old, line)) {
                    return None;
                }
                super::context::unfold_to(ctx, line, old, 3).map(|f| (f, (old, line)))
            });
            if let Some(((gap, how), mark)) = folded {
                acts.push(Action::DiffUnfold { gap, how });
                auto_opened = Some(mark);
            }
        }
        if let Some(open) = &view.moved {
            super::moved_view::panel(ui, &t, data, open, acts);
        }

        if let Some(program) = &data.textconv {
            let area = match &view.key {
                DiffKey::Work(file) => Some(file.area),
                _ => None,
            };
            textconv_note(ui, program, area);
        }
        let split = data.shows_split(split);
        let reveal = find::search_diff(&mut self.find, ui.ctx(), &path, data, split);
        let shift = ui.input(|i| i.modifiers.shift);
        let menu = || self.link_menu(None);
        let links = sides(&view.key).map(|(new, old)| LineLinks {
            path: path.clone(),
            new,
            old,
            menu: &menu,
        });
        let mut rows = Rows::new(ui, view, data, review.as_ref(), pickable, split)
            .with_find(&self.find, reveal);
        rows.links = links.as_ref();
        let sel = rows.show(ui, &path, acts);
        let (clicked, offset, jumped, seen) = (rows.clicked, rows.offset, rows.jumped, rows.seen);
        self.find.saw(seen);
        let hunk = if let Some(line) = clicked {
            data.hunk_at(false, line)
        } else if view.keyboard_hunk.is_none()
            || (view.scroll - offset).abs() > 0.5
            || jumped.is_some()
        {
            data.hunk_at(split, (offset / super::LINE_H) as usize + 3)
        } else {
            view.keyboard_hunk
        };

        if let Some(view) = &mut self.diff {
            view.keyboard_hunk = hunk;
            view.auto_opened.extend(auto_opened);
            view.text_sel = sel;
            view.scroll = offset;
            view.jump = None;
            view.scroll_to = None;
            view.jump_line = None;
            if let Some(i) = jumped {
                view.at_change = Some((i, offset));
            }
            if clear {
                view.chosen.clear();
                view.anchor = None;
            }
            if let Some(i) = clicked {
                view.pick_line(i, shift);
            }
        }
        self.keyboard_hunk(ui, acts);
    }

    /// Why the diff shown starts folded, while it does (a conflicted file never does, nor one
    /// opened from a file list). Asks what `.gitattributes` says of it the first time.
    fn fold_reason(
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
                format!("Commit {}", hash.get(..8).unwrap_or(hash)),
            ),
            DiffKey::Work(file) => (
                file.path.clone(),
                match file.area {
                    Area::Staged => "Staged changes".to_owned(),
                    Area::Unstaged => "Unstaged changes".to_owned(),
                    Area::Untracked => "New file".to_owned(),
                    Area::Conflicted => "Conflicted".to_owned(),
                },
            ),
            DiffKey::Range { from, to, path } => (path.clone(), self.range_label(from, to)),
            DiffKey::Remerge { hash, path, .. } => (path.clone(), remerge::origin(hash)),
        }
    }

    /// Header: file, where it comes from, options, actions, close. `opts`: (side by side,
    /// ignore whitespace, whole file, the file instead of the diff); `views`: for a Markdown
    /// file, whether it shows rendered; the rich views of the file, the one on, and whether it
    /// is shown in place of the lines. True when the picked lines are to be cleared.
    #[allow(clippy::too_many_arguments)]
    fn diff_header(
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
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "file-diff", 16.0, t.icon);
                    let title = ui.add(
                        egui::Label::new(egui::RichText::new(path).font(theme::medium(13.0)))
                            .sense(Sense::click()),
                    );
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
                        acts,
                        |ui, acts| {
                            self.lock_menu_items(ui, path, rev, acts);
                            self.storage_menu_items(ui, path, rev, acts);
                        },
                    );
                    widgets::chip(ui, origin, t.text_dim);
                    if let Some(ranges) = &view.lines {
                        widgets::chip(ui, &lines_chip(ranges), t.accent);
                    }
                    let before = if markdown == Some(true) {
                        matches!(&view.rendered, Some(Ok(md)) if md.before)
                    } else {
                        matches!(&view.file, Some(Ok(f)) if f.before)
                    };
                    if as_file && before {
                        widgets::chip(ui, "before the change", t.warning);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                            acts.push(Action::CloseDiff);
                        }
                        popout::button(ui, Panel::Diff, !self.docked(Panel::Diff), acts);
                        if let DiffKey::Work(file) = &view.key
                            && !view.structural.enabled
                        {
                            let partial = partial && !as_file && !rendered;
                            clear = self.file_actions(ui, view, file, partial, acts);
                        }
                        if let DiffKey::Commit { hash, path } = &view.key
                            && view.lines.is_none()
                            && self.stash_view.is_none()
                            && !view.structural.enabled
                        {
                            let lines = !as_file && !rendered;
                            clear = self.commit_actions(ui, view, (hash, path), lines, acts);
                        }
                        ui.add_space(8.0);
                        if !as_file && !rendered && !folded && !rich_ready && view.lines.is_none() {
                            structural::toggle(ui, view.structural.enabled, acts);
                            ui.add_space(4.0);
                        }
                        // A folded diff shows no lines to arrange.
                        if !as_file && !rendered && !folded && view.structural.shows_patch() {
                            // A diff of some lines only (`git log -L`) shows them whole, with
                            // whitespace: those options don't apply to it.
                            if view.lines.is_none() {
                                view_options(ui, &t, (split, ignore_ws, whole_file), acts);
                                ui.add_space(4.0);
                                super::algorithm::algorithm_menu(ui, path, view.repairing, acts);
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
                                (true, _) => "Show the Markdown as text",
                                (false, false) => "Show the changes rendered",
                                (false, true) => "Show the file rendered",
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
                        let on = usize::from(as_file);
                        if let Some(i) = widgets::switch(ui, &["Diff", "File"], on) {
                            acts.push(Action::DiffAsFile(i == 1));
                        }
                    });
                });
            });
        clear
    }

    /// Stage, unstage or mark resolved for the whole file, and the buttons for the lines
    /// clicked in the diff. True when "Clear" was clicked.
    fn file_actions(
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
                "Unstage file",
                vec![unstage_args(&self.refs.head, &file.path)],
            ),
            Area::Conflicted => ("Mark resolved", add()),
            _ => ("Stage file", add()),
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
                    egui::Button::new("Keep both…"),
                )
                .on_hover_text(
                    "For a file both sides changed that can't be merged (an image, an \
                     archive…): write each side's version as a file of its own, named after its \
                     branch. Shows what it would write first.",
                )
                .on_disabled_hover_text("Keep both is already open for a file")
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
        let lines = if chosen == 1 { "line" } else { "lines" };
        let picks: &[Pick] = if file.area == Area::Staged {
            &[Pick::Unstage]
        } else {
            &[Pick::Stage, Pick::Discard]
        };
        for &pick in picks {
            let b = egui::Button::new(format!("{} {chosen} {lines}", pick.verb()));
            if ui.add(b).clicked() {
                acts.push(Action::PickLines {
                    pick,
                    lines: view.chosen.clone(),
                });
            }
        }
        ui.button("Clear")
            .on_hover_text("Unselect the lines")
            .clicked()
    }

    /// Revert the commit's changes to the whole file or, once lines are clicked in its diff
    /// (`lines`: they show), those lines. True when "Clear" was clicked.
    fn commit_actions(
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
        // Laid out right to left. The lines' buttons take the place of the file's, which stays
        // in the file's menu, so the header keeps room for the rest.
        if lines && revertable && chosen > 0 {
            let noun = if chosen == 1 { "line" } else { "lines" };
            let b = ui
                .add_enabled(idle, egui::Button::new(format!("Revert {chosen} {noun}")))
                .on_hover_text(
                    "Undo only the lines clicked in the working tree, to look over before \
                     committing. Revert changes to this file is in the file name's menu.",
                );
            if b.clicked() {
                acts.push(Action::PickLines {
                    pick: Pick::Revert,
                    lines: view.chosen.clone(),
                });
            }
            return ui
                .button("Clear")
                .on_hover_text("Unselect the lines")
                .clicked();
        }
        let b = ui
            .add_enabled(idle, egui::Button::new("Revert file"))
            .on_hover_text(
                "Undo this commit's changes to the file in the working tree, to look over \
                 before committing",
            );
        if b.clicked() {
            acts.push(Action::RevertFile {
                hash: hash.to_owned(),
                path: path.to_owned(),
            });
        }
        false
    }

    /// "abc1234 → def5678" for a diff between two commits, with the names of the comparison.
    fn range_label(&self, from: &str, to: &Option<String>) -> String {
        let name = |rev: &str| {
            self.compare
                .as_ref()
                .and_then(|c| c.name_of(rev))
                .or_else(|| self.pr_name_of(rev))
                .unwrap_or_else(|| short(rev).to_owned())
        };
        match to {
            Some(to) => format!("{} → {}", name(from), name(to)),
            None => format!("{} → working tree", name(from)),
        }
    }
}

impl DiffView {
    /// Click on a changed line: toggles it, or with Shift selects every changed line from the
    /// last one clicked.
    fn pick_line(&mut self, i: usize, extend: bool) {
        let Some(Ok(data)) = &self.data else { return };
        match self.anchor {
            Some(a) if extend => {
                let (lo, hi) = (a.min(i), a.max(i));
                self.chosen.extend(
                    (lo..=hi).filter(|&k| matches!(data.lines[k].kind, Kind::Add | Kind::Del)),
                );
            }
            _ => {
                if !self.chosen.remove(&i) {
                    self.chosen.insert(i);
                }
                self.anchor = Some(i);
            }
        }
    }
}

/// The revisions of a commit's diff (or a comparison of two) its lines can link to: the file
/// after the change, and before it. `None` for changes not committed, which no forge has.
fn sides(key: &DiffKey) -> Option<(String, String)> {
    match key {
        DiffKey::Commit { hash, .. } => Some((hash.clone(), format!("{hash}^"))),
        DiffKey::Range {
            from, to: Some(to), ..
        } => Some((to.clone(), from.clone())),
        DiffKey::Remerge { .. } | DiffKey::Range { to: None, .. } | DiffKey::Work(_) => None,
    }
}

/// "Lines 4–9" (and the others, when the lines moved apart): those the diff is limited to.
fn lines_chip(ranges: &[crate::git::LineRange]) -> String {
    let parts: Vec<String> = ranges
        .iter()
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}–{b}")
            }
        })
        .collect();
    let noun = if ranges.len() == 1 && ranges[0].0 == ranges[0].1 {
        "Line"
    } else {
        "Lines"
    };
    format!("{noun} {}", parts.join(", "))
}

/// Whitespace, whole file, side by side and unified toggles, laid out right to left.
fn view_options(
    ui: &mut Ui,
    t: &Tokens,
    (split, ignore_ws, whole_file): (bool, bool, bool),
    acts: &mut Vec<Action>,
) {
    let ws = icons::small_button(
        ui,
        "pilcrow",
        if ignore_ws {
            "Show whitespace changes"
        } else {
            "Ignore whitespace changes"
        },
    );
    if ignore_ws {
        widgets::active_underline(ui, ws.rect, t.accent);
    }
    if ws.clicked() {
        acts.push(Action::SetDiffOptions {
            split,
            ignore_ws: !ignore_ws,
            whole_file,
        });
    }
    let whole = icons::small_button(
        ui,
        "unfold-vertical",
        if whole_file {
            "Show only the changes"
        } else {
            "Show the whole file"
        },
    );
    if whole_file {
        widgets::active_underline(ui, whole.rect, t.accent);
    }
    if whole.clicked() {
        acts.push(Action::SetDiffOptions {
            split,
            ignore_ws,
            whole_file: !whole_file,
        });
    }
    let s = icons::small_button(ui, "columns-2", "Side by side");
    if split {
        widgets::active_underline(ui, s.rect, t.accent);
    }
    if s.clicked() {
        acts.push(Action::SetDiffOptions {
            split: true,
            ignore_ws,
            whole_file,
        });
    }
    let u = icons::small_button(ui, "rows-2", "Unified");
    if !split {
        widgets::active_underline(ui, u.rect, t.accent);
    }
    if u.clicked() {
        acts.push(Action::SetDiffOptions {
            split: false,
            ignore_ws,
            whole_file,
        });
    }
}

/// The previous and next change buttons, laid out right to left.
fn change_steps(ui: &mut Ui, acts: &mut Vec<Action>) {
    if icons::small_button(ui, "arrow-down", "Next change (F7, Alt+↓)").clicked() {
        acts.push(Action::DiffStep(1));
    }
    if icons::small_button(ui, "arrow-up", "Previous change (Shift+F7, Alt+↑)").clicked() {
        acts.push(Action::DiffStep(-1));
    }
}

/// The diff when it has lines to show; otherwise shows a spinner, the error, the LFS objects,
/// the images or a "no changes" note in its place.
fn ready_data<'a>(
    ui: &mut Ui,
    view: &'a DiffView,
    ignore_ws: bool,
    t: &Tokens,
    acts: &mut Vec<Action>,
) -> Option<&'a DiffData> {
    let data = match &view.data {
        None => {
            ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
            return None;
        }
        Some(Err(e)) => {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.colored_label(t.danger, e);
            });
            return None;
        }
        Some(Ok(data)) => data,
    };
    if let Some(lfs) = &data.lfs {
        lfs_view(ui, lfs);
        return None;
    }
    if let Some(cmp) = &data.compare {
        compare_view(ui, cmp, acts);
        return None;
    }
    if let Some(images) = &data.image {
        image_view(ui, images);
        return None;
    }
    if let Some(binary) = &data.binary {
        binary_view(ui, binary, view.key.path());
        return None;
    }
    // A notebook shows its cells, unless its JSON is picked.
    if let Some(nb) = &data.notebook
        && !notebook_bar(ui, nb, view.key.path(), acts)
    {
        return None;
    }
    if data.lines.is_empty() {
        ui.centered_and_justified(|ui| {
            let msg = if ignore_ws {
                "No changes besides whitespace"
            } else {
                "No textual changes"
            };
            ui.label(egui::RichText::new(msg).color(t.text_dim));
        });
        return None;
    }
    Some(data)
}

/// The panel below the diff to write a comment on the line picked in a pull request's file.
fn line_comment_panel(
    ui: &mut Ui,
    t: &Tokens,
    pr: &mut PrView,
    draft: &mut PrDraft,
    data: &DiffData,
    acts: &mut Vec<Action>,
) {
    egui::Panel::bottom("pr_line_comment")
        .frame(
            egui::Frame::new()
                .fill(t.card)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .stroke(Stroke::new(1.0, t.separator)),
        )
        .show(ui, |ui| {
            // The line as it is in the pull request, to suggest a change to.
            let current = draft
                .target
                .as_ref()
                .filter(|t| t.side == DiffSide::Right)
                .and_then(|t| {
                    data.lines
                        .iter()
                        .find(|l| l.kind != Kind::Del && l.new.map(u64::from) == Some(t.line))
                        .and_then(|l| l.raw.get(1..))
                });
            pr_review::line_panel(ui, pr, draft, current, acts);
            pr_view::only_web_links(ui.ctx());
        });
}
