//! Drawing the reflog view: its bar (which reflog, or the lost commits), the list, and the
//! buttons for the commit selected.

mod rows;
use rows::{
    Columns, entry_tip, kind_chip, loading_row, message, not_scanned, paint_row_back, scanning,
    times,
};

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{Pick, ReflogAct, ReflogView, Source};
use crate::git::{self, LostCommit, ReflogEntry, ReflogKind};
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};
use crate::{icons, spinner};

const ROW_H: f32 = 28.0;
/// The kind's label, on the left of each entry.
const KIND_W: f32 = 96.0;
/// From the right: the date, how long ago, and the old → new hashes.
const DATE_W: f32 = 112.0;
const AGO_W: f32 = 104.0;
const MOVE_W: f32 = 160.0;
/// Narrower than this, the date column is left out (it is in the hover too)…
const ROOM_FOR_DATE: f32 = 900.0;
/// …and narrower than this, the old → new hashes: what the entry says goes last.
const ROOM_FOR_MOVE: f32 = 600.0;
/// How close to the end of the list scrolling reads the next page.
const READ_AHEAD: usize = 30;

impl RepoTab {
    /// The view, above the graph, in a pane whose lower edge can be dragged.
    pub(in crate::repo) fn reflog_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let height = ui.available_height();
        egui::Panel::top("reflog_panel")
            .resizable(true)
            .default_size((height * 0.45).max(220.0))
            .size_range(150.0..=(height * 0.8).max(150.0))
            .frame(
                egui::Frame::new()
                    .fill(t.dock)
                    .stroke(Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| self.reflog_contents(ui, acts));
    }

    fn reflog_contents(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(view) = &self.reflog else { return };
        let t = Tokens::get(ui.ctx());
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| self.reflog_bar(ui, view, acts));
        egui::Panel::bottom("reflog_footer")
            .frame(
                egui::Frame::new()
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| {
                // Wrapped: a narrow pane must not push the buttons past its edge.
                ui.horizontal_wrapped(|ui| match view.selected_pick() {
                    Some(pick) => self.reflog_buttons(ui, &pick, acts),
                    None => {
                        let hint = match view.source {
                            Source::Ref(_) => {
                                crate::i18n::text("repo-workflows-reflog-reflog-contents-7e0b72b2")
                            }
                            Source::Lost => {
                                crate::i18n::text("repo-workflows-reflog-reflog-contents-92034002")
                            }
                        };
                        ui.label(egui::RichText::new(hint).color(t.text_faint));
                    }
                });
            });
        match &view.source {
            Source::Ref(reference) => self.reflog_list(ui, view, reference, acts),
            Source::Lost => self.lost_list(ui, view, acts),
        }
    }

    /// The bar on top: the title, Reflog or Lost commits, which reflog, reload and close.
    fn reflog_bar(&self, ui: &mut Ui, view: &ReflogView, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            // Close and reload first, from the right: the rest gets what is left, never them.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::small_button(
                    ui,
                    "x",
                    crate::i18n::text("repo-workflows-reflog-reflog-bar-961b2a04"),
                )
                .clicked()
                {
                    acts.push(Action::CloseReflog);
                }
                let tip = match view.source {
                    Source::Ref(_) => {
                        crate::i18n::text("repo-workflows-reflog-reflog-bar-83f07529")
                    }
                    Source::Lost => crate::i18n::text("repo-workflows-reflog-reflog-bar-3d04584d"),
                };
                if icons::small_button(ui, "refresh-cw", tip).clicked() {
                    acts.push(Action::Reflog(ReflogAct::Reload));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    icons::inline(ui, "history", 16.0, t.icon);
                    let on = usize::from(view.source == Source::Lost);
                    match widgets::switch(
                        ui,
                        &[
                            crate::i18n::text("repo-workflows-reflog-reflog-bar-abd5e345"),
                            crate::i18n::text("repo-workflows-reflog-reflog-bar-ec09fe01"),
                        ],
                        on,
                    ) {
                        Some(0) => acts.push(Action::Reflog(ReflogAct::Show(Source::head()))),
                        Some(_) => acts.push(Action::Reflog(ReflogAct::Show(Source::Lost))),
                        None => {}
                    }
                    if let Source::Ref(reference) = &view.source {
                        ui.add_space(8.0);
                        self.ref_picker(ui, reference, acts);
                    }
                });
            });
        });
    }

    /// Which reflog: HEAD's or a local branch's.
    fn ref_picker(&self, ui: &mut Ui, reference: &str, acts: &mut Vec<Action>) {
        let shown = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        egui::ComboBox::from_id_salt(("reflog-ref", self.repo_key()))
            .selected_text(shown)
            .height(360.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(reference == "HEAD", "HEAD").clicked() {
                    acts.push(Action::Reflog(ReflogAct::Show(Source::head())));
                }
                for b in &self.refs.local {
                    let source = Source::branch(&b.name);
                    let on = matches!(&source, Source::Ref(r) if r == reference);
                    if ui.selectable_label(on, &b.name).clicked() {
                        acts.push(Action::Reflog(ReflogAct::Show(source)));
                    }
                }
            })
            .response
            .on_hover_text(crate::i18n::text(
                "repo-workflows-reflog-ref-picker-c2e6cb5f",
            ));
    }

    fn reflog_list(&self, ui: &mut Ui, view: &ReflogView, reference: &str, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        if let Some(err) = &view.error {
            message(ui, err, t.danger);
            return;
        }
        if view.entries.is_empty() {
            if view.loading {
                ui.centered_and_justified(|ui| ui.add(spinner::Spinner::new()));
            } else {
                let name = reference.strip_prefix("refs/heads/").unwrap_or(reference);
                let text = crate::i18n::message(
                    "repo-workflows-reflog-reflog-list-31d0c09c",
                    &[("name", (name).into())],
                );
                message(ui, &text, t.text_dim);
            }
            return;
        }
        let now = git::now();
        let rows = view.entries.len() + usize::from(view.more);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .id_salt(("reflog-list", &self.path, reference))
            .auto_shrink(false)
            .show_rows(ui, ROW_H, rows, |ui, range| {
                if range.end + READ_AHEAD >= view.entries.len() && view.more && !view.loading {
                    acts.push(Action::Reflog(ReflogAct::LoadMore));
                }
                for i in range {
                    match view.entries.get(i) {
                        Some(entry) => self.entry_row(ui, view, i, entry, now, acts),
                        None => loading_row(ui),
                    }
                }
            });
    }

    fn entry_row(
        &self,
        ui: &mut Ui,
        view: &ReflogView,
        i: usize,
        e: &ReflogEntry,
        now: i64,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        // Known by the entry, not its place: a reload adds entries on top, and a menu left open
        // must stay this entry's.
        let (rect, resp) = widgets::keyed_row(
            ui,
            ROW_H,
            (
                "reflog-entry",
                &e.new,
                e.time,
                &e.message.action,
                &e.message.detail,
            ),
        );
        paint_row_back(ui, rect, view.selected == Some(i), resp.hovered());
        let mid = rect.center().y;
        let cols = Columns::of(rect);
        let p = ui.painter_at(rect);
        kind_chip(&p, &t, pos2(rect.left() + 10.0, mid), e.message.kind);
        // What it did, then the commit's summary when that says something more.
        let text = ui.painter_at(Rect::from_x_y_ranges(
            rect.left() + 12.0 + KIND_W..=cols.text_end,
            rect.y_range(),
        ));
        let detail = if e.message.detail.is_empty() {
            e.message.action.as_str()
        } else {
            e.message.detail.as_str()
        };
        let said = text.text(
            pos2(rect.left() + 12.0 + KIND_W, mid),
            Align2::LEFT_CENTER,
            detail,
            FontId::proportional(12.5),
            t.text,
        );
        if !e.summary.is_empty() && e.summary != detail {
            text.text(
                pos2(said.right() + 10.0, mid),
                Align2::LEFT_CENTER,
                &e.summary,
                FontId::proportional(12.0),
                t.text_faint,
            );
        }
        if let Some(move_end) = cols.move_end {
            let moved = match &e.old {
                Some(old) if *old != e.new => format!("{} → {}", short(old), short(&e.new)),
                _ => short(&e.new).to_owned(),
            };
            p.text(
                pos2(move_end, mid),
                Align2::RIGHT_CENTER,
                moved,
                theme::mono(11.0),
                t.text_dim,
            );
        }
        times(&p, &t, &cols, mid, e.time, &e.date, now);
        let resp = resp.on_hover_text(entry_tip(e, now));
        self.row_clicks(&resp, i, &Pick::of_entry(e), acts);
    }

    fn lost_list(&self, ui: &mut Ui, view: &ReflogView, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        if let Some(scan) = &view.scan {
            scanning(ui, &t, scan.started.elapsed().as_secs(), acts);
            return;
        }
        let found = match &view.lost {
            None => {
                not_scanned(ui, &t, acts);
                return;
            }
            Some(Err(e)) => {
                message(ui, e, t.danger);
                return;
            }
            Some(Ok(found)) => found,
        };
        if found.is_empty() {
            let text = crate::i18n::text("repo-workflows-reflog-lost-list-625e19f0");
            message(ui, text, t.text_dim);
            return;
        }
        let now = git::now();
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .id_salt(("lost-list", &self.path))
            .auto_shrink(false)
            .show_rows(ui, ROW_H, found.len(), |ui, range| {
                for i in range {
                    if let Some(c) = found.get(i) {
                        self.lost_row(ui, view, i, c, now, acts);
                    }
                }
            });
    }

    fn lost_row(
        &self,
        ui: &mut Ui,
        view: &ReflogView,
        i: usize,
        c: &LostCommit,
        now: i64,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("lost-commit", &c.hash));
        paint_row_back(ui, rect, view.selected == Some(i), resp.hovered());
        let mid = rect.center().y;
        let cols = Columns::of(rect);
        let p = ui.painter_at(rect);
        let hash = p.text(
            pos2(rect.left() + 12.0, mid),
            Align2::LEFT_CENTER,
            short(&c.hash),
            theme::mono(11.5),
            t.text_dim,
        );
        let text = ui.painter_at(Rect::from_x_y_ranges(
            hash.right() + 12.0..=cols.ago_end - AGO_W,
            rect.y_range(),
        ));
        let said = text.text(
            pos2(hash.right() + 12.0, mid),
            Align2::LEFT_CENTER,
            &c.summary,
            FontId::proportional(12.5),
            t.text,
        );
        text.text(
            pos2(said.right() + 10.0, mid),
            Align2::LEFT_CENTER,
            &c.author,
            FontId::proportional(12.0),
            t.text_faint,
        );
        times(&p, &t, &cols, mid, c.time, &c.date, now);
        let tip = crate::i18n::message(
            "repo-workflows-reflog-lost-row-379fcead",
            &[
                ("c-hash-1", (c.hash).clone().into()),
                ("c-summary-2", (c.summary).clone().into()),
                ("c-author-3", (c.author).clone().into()),
                ("c-date-4", (c.date).clone().into()),
                (
                    "git-relative-time-c-time-now-5",
                    (git::relative_time(c.time, now)).clone().into(),
                ),
            ],
        );
        let resp = resp.on_hover_text(tip);
        self.row_clicks(&resp, i, &Pick::of_lost(c), acts);
    }

    /// A click selects the row's commit, a double-click makes a branch there, a right-click
    /// opens its menu.
    fn row_clicks(&self, resp: &egui::Response, i: usize, pick: &Pick, acts: &mut Vec<Action>) {
        if resp.clicked() || resp.secondary_clicked() {
            acts.push(Action::Reflog(ReflogAct::Select(i)));
        }
        if resp.double_clicked() {
            acts.push(super::menu::create_branch(pick));
        }
        crate::menu_item::context(resp)
            .style(theme::menu_style)
            .show(|ui| self.reflog_menu_items(ui, pick, acts));
    }
}
