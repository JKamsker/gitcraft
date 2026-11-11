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
        let locale_changed = self.diff.as_mut().is_some_and(|view| {
            let generation = crate::i18n::generation();
            if view.presentation_generation == generation {
                return false;
            }
            view.presentation_generation = generation;
            view.rich = None;
            view.rich_state = Default::default();
            true
        });
        if locale_changed {
            self.refresh_diff(ui.ctx());
            self.load_diff_rich(ui.ctx());
        }
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
        let partial = matches!(&view.data, Some(Ok(d)) if d.partial)
            && idle
            && !view.structural.enabled
            && !view.encrypted.enabled;
        // Lines and hunks can be picked: to stage, unstage or discard, or to revert.
        // A stash's changes aren't a commit's to revert.
        let revert = self.stash_view.is_none();
        let pickable = matches!(&view.data, Some(Ok(d)) if d.partial || d.revertable && revert)
            && idle
            && !view.structural.enabled
            && !view.encrypted.enabled;
        let (split, ignore_ws, whole_file) = (
            settings.diff_split,
            self.diff_ignore_ws,
            settings.diff_whole_file,
        );
        let as_file = self.diff_as_file;
        // A Markdown file can show rendered: `Some(whether it does)`.
        let markdown = view.key.renders().then_some(self.diff_rendered);
        let folded = if as_file || markdown == Some(true) || view.encrypted.enabled {
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

        if matches!(&view.data, Some(Ok(data)) if data.sops) {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::text("repo-core-2344"));
                if view.lines.is_some() {
                    ui.label(crate::i18n::text("repo-core-2345"));
                }
                let mut encoding = view.encrypted.json_encoding;
                egui::ComboBox::from_id_salt("sops_json_encoding")
                    .selected_text(encoding.label())
                    .show_ui(ui, |ui| {
                        use super::encrypted::JsonEncoding;
                        for choice in [
                            JsonEncoding::Ask,
                            JsonEncoding::Document,
                            JsonEncoding::Binary,
                        ] {
                            ui.selectable_value(&mut encoding, choice, choice.label());
                        }
                    });
                if encoding != view.encrypted.json_encoding {
                    acts.push(Action::DiffDecryptedEncoding(encoding));
                }
                if view.encrypted.enabled {
                    if ui.button(crate::i18n::text("repo-core-2347")).clicked() {
                        acts.push(Action::DiffDecrypted(false));
                    }
                } else if ui
                    .add_enabled(
                        idle && !self.loading && view.request == view.data_request,
                        egui::Button::new(crate::i18n::text("repo-core-2348")),
                    )
                    .on_hover_text(crate::i18n::text("repo-core-2349"))
                    .clicked()
                {
                    acts.push(Action::DiffDecrypted(true));
                }
            });
        }
        if view.encrypted.enabled {
            if let Some(view) = &mut self.diff {
                super::encrypted::show(ui, &mut view.encrypted);
            }
            return;
        }
        if let Some(what) = self.find_in() {
            self.find.bar(ui, what.name());
        }
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
                        ui.colored_label(
                            t.danger,
                            crate::git::display_error(&crate::forge::inbox::display_error(e)),
                        );
                    });
                }
                Some(Ok(md)) => {
                    if self.find.open {
                        self.find.rendered(
                            ui,
                            format!(
                                "markdown:{:?}:{}:{}:{as_file}",
                                view.key, view.request, view.rendered_load
                            ),
                            |t| {
                                let mut doc = find::rendered::Document::default();
                                if as_file {
                                    doc.markdown(&md.file, t.text);
                                } else {
                                    for (change, text) in &md.blocks {
                                        let color = match change {
                                            rendered::Change::Kept => t.text,
                                            rendered::Change::Removed => t.danger,
                                            rendered::Change::Added => t.success,
                                        };
                                        doc.markdown(text, color);
                                    }
                                }
                                doc
                            },
                        );
                    } else {
                        rendered::show(ui, &path, md, &mut view.markdown, !as_file);
                    }
                }
            }
            return;
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
                .ok_or(crate::i18n::text("repo-core-1856"));
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
                None,
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
                        ui.colored_label(
                            t.danger,
                            crate::git::display_error(&crate::forge::inbox::display_error(e)),
                        );
                    });
                    return;
                }
                Some((_, Ok(rich::Rich::Ready(diff)))) => {
                    let metadata_generation = view.rich_state.search_generation(ui.ctx());
                    if self.find.open {
                        self.find.rendered(
                            ui,
                            format!(
                                "rich:{:?}:{}:{}:{metadata_generation}",
                                view.key, view.request, view.rich_load
                            ),
                            |t| rich::search_document(diff, &view.rich_state, t),
                        );
                    } else {
                        rich::show(ui, diff, &mut view.rich_state, acts);
                    }
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
                    if self.find.open {
                        self.find.rendered(
                            ui,
                            format!(
                                "syntax:{:?}:{}:{}",
                                view.key,
                                view.request,
                                view.structural.generation()
                            ),
                            |t| structural::search_document(data, t),
                        );
                    } else {
                        structural::show(ui, data);
                    }
                    return;
                }
                Some(Err(reason)) => {
                    ui.colored_label(t.warning, reason);
                    ui.label(crate::i18n::text("repo-core-2354"));
                    ui.separator();
                }
            }
        }
        let Some(data) = ready_data(ui, view, ignore_ws, &t, &mut self.find, acts) else {
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
        let text_started = rows.text_started;
        let (clicked, offset, jumped, seen) = (rows.clicked, rows.offset, rows.jumped, rows.seen);
        let wrap_offsets = rows.geometry.as_ref().map(|g| g.offsets.clone());
        let viewport_height = rows.viewport_height;
        let layout_changed = rows.layout_changed;
        self.find.saw(seen);
        let focused_line = clicked.or(sel.filter(|_| text_started).map(|s| s.cursor.0));
        let hunk = super::presentation::keyboard_target(
            view,
            data,
            split,
            wrap_offsets.as_deref().map(Vec::as_slice),
            offset..offset + viewport_height,
            focused_line,
            (view.scroll - offset).abs() > 0.5 || jumped.is_some() || layout_changed,
        );

        if let Some(view) = &mut self.diff {
            view.keyboard_hunk = hunk;
            view.auto_opened.extend(auto_opened);

            view.scroll = offset;
            view.wrap_offsets = wrap_offsets;
            view.jump = None;
            view.scroll_to = None;
            view.jump_line = None;
            if let Some(i) = jumped {
                view.at_change = Some((i, offset));
            }
            super::presentation::select(view, sel, clear, text_started, clicked, shift);
        }
        self.keyboard_hunk(ui, acts);
    }
}

mod ready;
use ready::{line_comment_panel, ready_data};

mod controls;
use controls::{change_steps, lines_chip, sides, view_options, wrap_control};

mod header;

#[cfg(test)]
#[path = "header_tests.rs"]
mod header_tests;

#[cfg(test)]
#[path = "selection_tests.rs"]
mod selection_tests;
