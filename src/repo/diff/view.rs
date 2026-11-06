//! The diff pane: its header (file, origin, actions, view options), what shows while there are
//! no lines, and the comment panel for a pull request's file.

use egui::{Sense, Stroke, Ui};

use super::images::image_view;
use super::lfs::lfs_view;
use super::rendered;
use super::rows::Rows;
use super::{DiffData, Kind, Pick, old_name, picked_in_file, unstage_args};
use crate::git::{Area, StatusFile};
use crate::icons;
use crate::pull_request::DiffSide;
use crate::repo::pr_review::{self, PrDraft};
use crate::repo::pr_view::{self, PrView};
use crate::repo::{Action, DiffKey, DiffView, Refresh, RepoTab, file_text, ops, short, widgets};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(in crate::repo) fn diff_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &self.diff else { return };
        let idle = self.busy_op.is_none();

        let (path, origin) = self.diff_origin(&view.key);
        let partial = matches!(&view.data, Some(Ok(d)) if d.partial) && idle;
        // Lines and hunks can be picked: to stage, unstage or discard, or to revert.
        // A stash's changes aren't a commit's to revert.
        let revert = self.stash_view.is_none();
        let pickable =
            matches!(&view.data, Some(Ok(d)) if d.partial || d.revertable && revert) && idle;
        let settings = Settings::get(ui.ctx());
        let (split, ignore_ws, whole_file) = (
            settings.diff_split,
            self.diff_ignore_ws,
            settings.diff_whole_file,
        );
        let as_file = self.diff_as_file;
        // A Markdown file can show rendered: `Some(whether it does)`.
        let markdown = rendered::is_markdown(&path).then_some(self.diff_rendered);
        let clear = self.diff_header(
            ui,
            view,
            (&path, &origin),
            partial,
            (split, ignore_ws, whole_file, as_file),
            markdown,
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
        if as_file {
            let before = matches!(&view.file, Some(Ok(f)) if f.before);
            let old_path = match &view.data {
                Some(Ok(d)) => old_name(&d.header),
                _ => None,
            };
            let pick = |range| picked_in_file(&view.key, old_path.as_deref(), before, range);
            let history = pick((1, 1))
                .map(|_| ())
                .ok_or("Untracked and conflicted files have no line history");
            // Lines picked in one version aren't those of another one of the same file.
            let id = ("diff_file", &path, format!("{:?}", view.key));
            if let Some(p) = file_text::show(ui, id, view.file.as_ref(), history).and_then(pick) {
                acts.push(Action::LineHistory {
                    path: p.path,
                    at: p.at,
                    range: p.range,
                });
            }
            return;
        }
        let Some(data) = ready_data(ui, view, ignore_ws, &t) else {
            return;
        };

        // A file of the pull request shown: its lines take comments.
        let review = self.diff_review(&view.key);
        if review.as_ref().is_some_and(|r| r.target.is_some())
            && let Some(pr) = &mut self.pr_view
        {
            let draft = self.pr_drafts.entry(pr.number).or_default();
            line_comment_panel(ui, &t, pr, draft, data, acts);
        }

        let split = data.shows_split(split);
        let shift = ui.input(|i| i.modifiers.shift);
        let mut rows = Rows::new(ui, view, data, review.as_ref(), pickable, split);
        let sel = rows.show(ui, &path, acts);
        let (clicked, offset, jumped) = (rows.clicked, rows.offset, rows.jumped);

        if let Some(view) = &mut self.diff {
            view.text_sel = sel;
            view.scroll = offset;
            view.jump = None;
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
    }

    /// The file shown and where its diff comes from.
    fn diff_origin(&self, key: &DiffKey) -> (String, String) {
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
        }
    }

    /// Header: file, where it comes from, options, actions, close. `opts`: (side by side,
    /// ignore whitespace, whole file, the file instead of the diff); `markdown`: for a Markdown
    /// file, whether it shows rendered. True when the picked lines are to be cleared.
    #[allow(clippy::too_many_arguments)]
    fn diff_header(
        &self,
        ui: &mut Ui,
        view: &DiffView,
        (path, origin): (&str, &str),
        partial: bool,
        (split, ignore_ws, whole_file, as_file): (bool, bool, bool, bool),
        markdown: Option<bool>,
        acts: &mut Vec<Action>,
    ) -> bool {
        let rendered = markdown == Some(true);
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
                        DiffKey::Commit { hash, .. } => (Some(hash.as_str()), false),
                        DiffKey::Range { to: Some(to), .. } => (Some(to.as_str()), false),
                        DiffKey::Range { to: None, .. } => (None, false),
                        DiffKey::Work(_) => (None, true),
                    };
                    match &view.key {
                        DiffKey::Commit { hash, .. } if self.stash_view.is_none() => {
                            widgets::commit_file_menu(&title, path, hash, acts)
                        }
                        _ => widgets::file_menu(&title, path, rev, ignorable, acts),
                    }
                    widgets::chip(ui, origin, t.text_dim);
                    if let Some(ranges) = &view.lines {
                        widgets::chip(ui, &lines_chip(ranges), t.accent);
                    }
                    let before = if rendered {
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
                        if let DiffKey::Work(file) = &view.key {
                            let partial = partial && !as_file && !rendered;
                            clear = self.file_actions(ui, view, file, partial, acts);
                        }
                        if let DiffKey::Commit { hash, path } = &view.key
                            && view.lines.is_none()
                            && self.stash_view.is_none()
                        {
                            let lines = !as_file && !rendered;
                            clear = self.commit_actions(ui, view, (hash, path), lines, acts);
                        }
                        ui.add_space(8.0);
                        if !as_file && !rendered {
                            // A diff of some lines only (`git log -L`) shows them whole, with
                            // whitespace: those options don't apply to it.
                            if view.lines.is_none() {
                                view_options(ui, &t, (split, ignore_ws, whole_file), acts);
                                ui.add_space(8.0);
                            }
                            change_steps(ui, acts);
                            ui.add_space(8.0);
                        }
                        if markdown.is_some() {
                            let tip = match (rendered, as_file) {
                                (true, _) => "Show the Markdown as text",
                                (false, false) => "Show the changes rendered",
                                (false, true) => "Show the file rendered",
                            };
                            let r = icons::small_button(ui, "eye", tip);
                            if rendered {
                                widgets::active_underline(ui, r.rect, t.accent);
                            }
                            if r.clicked() {
                                acts.push(Action::DiffRendered(!rendered));
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
    if let Some(images) = &data.image {
        image_view(ui, images);
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
