//! One commit row of the graph: its background, the description and the hash, author, when
//! and date columns.

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Stroke};
use egui::{Ui, pos2, vec2};

use super::{AVATAR_D, ROW_GAP, ROW_H, ROW_PITCH};
use crate::git::{self, Commit};
use crate::repo::columns::Cols;
use crate::repo::{Action, RepoTab, WIP_HASH, ci_status, widgets};
use crate::theme::{self, Tokens};

/// The lanes the graph column draws.
#[derive(Clone, Copy)]
pub(super) struct Lanes {
    /// How many: those further right are left out.
    pub(super) shown: usize,
    /// From the centre of one to the next.
    pub(super) width: f32,
}

/// The row being painted.
pub(super) struct Row<'a> {
    pub(super) i: usize,
    pub(super) commit: &'a Commit,
    pub(super) rect: Rect,
    pub(super) cols: Cols,
    /// Left edge of the graph column.
    pub(super) gx: f32,
    /// From one lane to the next.
    pub(super) lane_w: f32,
    /// Where the commit's dot is, if the graph has the row.
    pub(super) node_x: Option<f32>,
    /// The colour of the commit's lane.
    pub(super) color: Color32,
}

impl Row<'_> {
    pub(super) fn mid(&self) -> f32 {
        self.rect.center().y
    }

    /// The centre of `lane` in the graph column.
    pub(super) fn lane_x(&self, lane: usize) -> f32 {
        lane_x(self.gx, lane, self.lane_w)
    }
}

fn lane_x(gx: f32, lane: usize, lane_w: f32) -> f32 {
    gx + 6.0 + lane as f32 * lane_w + lane_w / 2.0
}

/// The id of the graph row of the commit (or stash, or changes row) `hash`.
pub(super) fn row_id(ui: &Ui, hash: &str) -> egui::Id {
    ui.id().with(("commit-row", hash))
}

impl RepoTab {
    pub(super) fn commit_row(
        &mut self,
        ui: &mut Ui,
        i: usize,
        cols: Cols,
        lanes: Lanes,
        viewport: &super::readability::Viewport,
        acts: &mut Vec<Action>,
    ) -> (Response, Option<String>) {
        let t = Tokens::get(ui.ctx());
        let (auto_id, rect) = ui.allocate_space(vec2(ui.available_width(), ROW_PITCH));
        let Some(commit) = self.commits.get(i) else {
            return (ui.interact(rect, auto_id, Sense::click()), None);
        };
        // Known by its commit, not its place: rows shift when a reload adds commits or the
        // changes row above them, and a menu opened on one must stay that commit's.
        let resp = ui.interact(rect, row_id(ui, &commit.hash), Sense::click());
        if self.graph_walk_focus.as_deref() == Some(commit.hash.as_str()) {
            resp.request_focus();
            self.graph_walk_focus = None;
            self.graph_walk_focus_id = Some(resp.id);
        }
        if self.graph_walk_focus_id == Some(resp.id)
            && crate::settings::Settings::get(ui.ctx()).graph_lane_walk
        {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    resp.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        resp.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                self.selected == Some(i),
                format!(
                    "Commit {}, {}, by {}. {}",
                    commit.hash.get(..8).unwrap_or(&commit.hash),
                    commit.summary,
                    commit.author,
                    self.graph_description(i)
                ),
            )
        });
        let painter = ui.painter_at(rect);
        let mut hovered_message = None;
        let is_wip = commit.hash == WIP_HASH;
        let is_match = self.search.matches.contains(&commit.hash);
        let background = row_background(
            &painter,
            &t,
            rect,
            self.selected == Some(i),
            resp.hovered(),
            is_match,
            self.is_compared(commit, is_wip),
        );

        if self.graph_motion.new_commit(&commit.hash) {
            let fade = self.graph_motion.highlight(ui.ctx());
            painter.rect_filled(rect, 0.0, t.accent.gamma_multiply(fade));
        }

        let gx = rect.left() + cols.refs;
        let graph_row = self.graph.rows.get(i);
        let row = Row {
            i,
            commit,
            rect,
            cols,
            gx,
            lane_w: lanes.width,
            node_x: graph_row.map(|r| lane_x(gx, r.col, lanes.width)),
            color: graph_row.map(|r| t.lane(r.col)).unwrap_or(t.text_faint),
        };

        let resp = self.refs_cell(ui, &painter, &row, resp, acts);
        if let Some(label) = viewport.labels.get(&i) {
            faint_lane_label(ui, &painter, &t, &row, label);
        }
        self.graph_cell(ui, &painter, &row, lanes.shown);
        if is_wip {
            self.wip_description(&painter, &t, &row, background);
        } else {
            let ci = self.ci_status(&commit.hash);
            let change = self.gerrit.as_ref().zip(commit.change_id.as_ref());
            description(
                ui,
                &painter,
                &t,
                &row,
                (is_match, self.graph_motion.new_commit(&commit.hash)),
                usize::from(ci.is_some()) + usize::from(change.is_some()),
            );
            if let Some((server, id)) = change {
                let center = pos2(
                    rect.right() - cols.trailing() - 9.0 - if ci.is_some() { 16.0 } else { 0.0 },
                    row.mid(),
                );
                let link = ui.interact(
                    Rect::from_center_size(center, vec2(14.0, 18.0)),
                    ui.id().with(("gerrit-change", &commit.hash)),
                    Sense::click(),
                );
                painter.text(
                    center,
                    Align2::CENTER_CENTER,
                    "↗",
                    FontId::proportional(14.0),
                    t.accent,
                );
                if link
                    .on_hover_text(format!("Open Gerrit change {id}"))
                    .clicked()
                {
                    acts.push(Action::OpenUrl(server.change_url(id)));
                }
            }
            let description_rect = Rect::from_min_max(
                pos2((gx + cols.graph).min(rect.right()), rect.top()),
                pos2(
                    (rect.right() - cols.trailing()).max(gx + cols.graph),
                    rect.bottom(),
                ),
            );
            if resp.hovered() && ui.rect_contains_pointer(description_rect.intersect(rect)) {
                hovered_message = Some(commit.hash.clone());
                resp.clone().on_hover_ui(|ui| {
                    match self.graph_messages.message(&self.path, &commit.hash) {
                        Some(Ok(message)) => {
                            ui.label(message.as_str());
                        }
                        Some(Err(error)) => {
                            ui.label(error);
                        }
                        None => {
                            ui.label("Loading full commit message…");
                        }
                    }
                });
            }
            trailing_columns(ui, &painter, &t, &row, viewport.now);
            if let Some(status) = ci {
                let center = pos2(rect.right() - cols.trailing() - 9.0, row.mid());
                let id = ui.id().with(("ci-dot", &commit.hash));
                acts.extend(ci_status::dot(ui, id, center, &commit.hash, status));
            }
        }
        (resp, hovered_message)
    }

    /// Whether `commit` is an end of the comparison shown on the right (the changes row for
    /// the working tree).
    fn is_compared(&self, commit: &Commit, is_wip: bool) -> bool {
        self.compare
            .as_ref()
            .and_then(|c| c.data.as_ref()?.as_ref().ok())
            .is_some_and(|d| {
                d.from == commit.hash
                    || d.to.as_deref() == Some(commit.hash.as_str())
                    || (is_wip && d.to.is_none())
            })
    }

    /// The changes row's description: how many files are conflicted, staged and unstaged.
    fn wip_description(&self, painter: &Painter, t: &Tokens, row: &Row, background: Color32) {
        let mid = row.mid();
        let mut x = row.gx + row.cols.graph + 6.0;
        let font = theme::medium(12.5);
        let r = painter.text(
            pos2(x, mid),
            Align2::LEFT_CENTER,
            "Uncommitted changes",
            font,
            t.text,
        );
        x = r.right() + 10.0;
        let conflicts = if self.work.conflicted.len() == 1 {
            "conflict"
        } else {
            "conflicts"
        };
        for (n, label, color) in [
            (self.work.conflicted.len(), conflicts, t.danger),
            (self.work.staged.len(), "staged", t.success),
            (self.work.unstaged.len(), "unstaged", t.warning),
        ] {
            if n > 0 {
                x += pill(
                    painter,
                    t,
                    pos2(x, mid),
                    &format!("{n} {label}"),
                    color,
                    background,
                ) + 6.0;
            }
        }
    }
}

/// The row's band: selected, hovered or a search match, outlined when `compared`.
fn row_background(
    painter: &Painter,
    t: &Tokens,
    rect: Rect,
    selected: bool,
    hovered: bool,
    is_match: bool,
    compared: bool,
) -> Color32 {
    // The row's own band, without the gap.
    let band = rect.shrink2(vec2(0.0, ROW_GAP / 2.0));
    let background = if selected {
        painter.rect_filled(band, 0.0, t.row_selected);
        t.row_selected
    } else if hovered {
        painter.rect_filled(band, 0.0, t.hover);
        t.hover
    } else if is_match {
        let tint = t.match_tint();
        painter.rect_filled(band, 0.0, tint);
        theme::palette::over(tint, t.dock)
    } else {
        t.dock
    };
    if is_match {
        painter.rect_filled(
            Rect::from_min_size(band.min, vec2(3.0, ROW_H)),
            0.0,
            t.warning,
        );
    }
    if compared {
        painter.rect_stroke(
            band.shrink(1.0),
            0.0,
            Stroke::new(1.0, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    background
}

/// The summary, then the rest of the message in the faint text colour, cut with an ellipsis.
/// `dot`: room is left at the end for the build status.
fn description(
    ui: &Ui,
    painter: &Painter,
    t: &Tokens,
    row: &Row,
    flags: (bool, bool),
    dots: usize,
) {
    let (is_match, incoming) = flags;
    let (rect, mid) = (row.rect, row.mid());
    let mut x = row.gx + row.cols.graph + 6.0;
    let text_right = rect.right() - row.cols.trailing() - 10.0 - 16.0 * dots as f32;
    let font = if is_match {
        theme::medium(12.5)
    } else {
        FontId::proportional(12.5)
    };
    let text_clip = painter.with_clip_rect(Rect::from_min_max(
        pos2(x, rect.top()),
        pos2(text_right.max(x), rect.bottom()),
    ));
    if incoming {
        text_clip.text(
            pos2(x + 2.0, mid),
            Align2::LEFT_CENTER,
            "new",
            theme::medium(11.0),
            t.text_dim,
        );
        x += 28.0;
    }
    let mut job = egui::text::LayoutJob::single_section(
        row.commit.summary.clone(),
        egui::TextFormat::simple(font, t.text),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((text_right - x - 2.0).max(0.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let summary_right = x + 2.0 + galley.size().x;
    text_clip.galley(pos2(x + 2.0, mid - galley.size().y / 2.0), galley, t.text);
    let room = text_right - summary_right - 12.0;
    if !row.commit.body.is_empty() && room > 24.0 {
        let mut job = egui::text::LayoutJob::single_section(
            row.commit.body.clone(),
            egui::TextFormat::simple(FontId::proportional(12.5), t.text_faint),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        text_clip.galley(
            pos2(summary_right + 10.0, mid - galley.size().y / 2.0),
            galley,
            t.text,
        );
    }
}

/// The hash, author, when and date columns, those the settings show.
fn trailing_columns(ui: &Ui, painter: &Painter, t: &Tokens, row: &Row, now: i64) {
    let (rect, mid, cols, commit) = (row.rect, row.mid(), row.cols, row.commit);
    let small = FontId::proportional(11.5);
    if cols.hash > 0.0 {
        painter.text(
            pos2(rect.right() - cols.trailing(), mid),
            Align2::LEFT_CENTER,
            commit.hash.get(..7).unwrap_or(&commit.hash),
            theme::mono(11.0),
            t.text_faint,
        );
    }
    if cols.author > 0.0 {
        let author_clip = Rect::from_min_max(
            pos2(
                rect.right() - cols.author - cols.when - cols.date,
                rect.top(),
            ),
            pos2(rect.right() - cols.when - cols.date - 8.0, rect.bottom()),
        );
        let avatar = pos2(author_clip.left() + AVATAR_D / 2.0, mid);
        widgets::avatar(ui, avatar, AVATAR_D / 2.0, &commit.author, &commit.email);
        painter.with_clip_rect(author_clip).text(
            pos2(author_clip.left() + AVATAR_D + 6.0, mid),
            Align2::LEFT_CENTER,
            &commit.author,
            small.clone(),
            t.text_dim,
        );
    }
    if cols.when > 0.0 {
        let when_clip = Rect::from_min_max(
            pos2(rect.right() - cols.when - cols.date, rect.top()),
            pos2(rect.right() - cols.date - 8.0, rect.bottom()),
        );
        painter.with_clip_rect(when_clip).text(
            pos2(when_clip.left(), mid),
            Align2::LEFT_CENTER,
            git::relative_time(commit.time, now),
            small.clone(),
            t.text_dim,
        );
    }
    if cols.date > 0.0 {
        painter.text(
            pos2(rect.right() - cols.date, mid),
            Align2::LEFT_CENTER,
            &commit.date,
            theme::mono(11.0),
            t.text_faint,
        );
    }
}

/// A small rounded count label; returns its width.
fn pill(
    painter: &Painter,
    t: &Tokens,
    left_center: Pos2,
    text: &str,
    color: Color32,
    background: Color32,
) -> f32 {
    let galley = painter.layout_no_wrap(text.to_owned(), theme::medium(11.0), color);
    let w = galley.size().x + 12.0;
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - 9.0), vec2(w, 18.0));
    painter.rect_filled(
        r,
        CornerRadius::same(9),
        t.text_tint(color, 0.18, background),
    );
    painter.galley(
        pos2(r.left() + 6.0, r.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    w
}

fn faint_lane_label(ui: &Ui, painter: &Painter, t: &Tokens, row: &Row, label: &str) {
    let x = row.rect.left() + 8.0;
    let mut job = egui::text::LayoutJob::single_section(
        label.to_owned(),
        egui::TextFormat::simple(FontId::proportional(11.0), t.text_faint),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((row.cols.refs - 24.0).max(0.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let right = x + galley.size().x;
    let clip = Rect::from_min_max(row.rect.min, pos2(row.gx - 4.0, row.rect.bottom()));
    painter.with_clip_rect(clip).galley(
        pos2(x, row.mid() - galley.size().y / 2.0),
        galley,
        t.text_faint,
    );
    if let Some(node_x) = row.node_x {
        painter.line_segment(
            [
                pos2(right + 6.0, row.mid()),
                pos2(node_x.min(row.gx + row.cols.graph), row.mid()),
            ],
            Stroke::new(1.0, t.text_faint.gamma_multiply(0.35)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painted_graph_bodies_and_wip_chips_meet_aa_in_combined_row_states() {
        let ctx = theme::test_paint::context();
        for builtin in theme::catalog::BUILTIN {
            let t = builtin.palette().tokens();
            theme::set_tokens(&ctx, t);
            let mut tab = RepoTab::new_deferred("/nonexistent/contrast-paint".into(), &ctx);
            let file = git::StatusFile {
                path: "file".into(),
                old_path: None,
                code: 'M',
                area: git::Area::Unstaged,
            };
            tab.work.unstaged.push(file.clone());
            tab.work.staged.push(file.clone());
            tab.work.conflicted.push(file);
            let commit = Commit {
                summary: "Subject".into(),
                body: "faintbody".into(),
                ..Default::default()
            };
            for state in 0..4 {
                let mut output = ctx.run_ui(Default::default(), |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(t.dock))
                        .show(ctx, |ui| {
                            let painter = ui.painter();
                            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, ROW_PITCH));
                            let mut row = Row {
                                i: 0,
                                commit: &commit,
                                rect,
                                cols: Cols {
                                    refs: 0.0,
                                    graph: 0.0,
                                    hash: 0.0,
                                    author: 0.0,
                                    when: 0.0,
                                    date: 0.0,
                                    push: 0.0,
                                },
                                gx: 0.0,
                                lane_w: 0.0,
                                node_x: None,
                                color: t.accent,
                            };
                            row_background(
                                painter,
                                &t,
                                row.rect,
                                state == 2,
                                state == 1,
                                state == 3,
                                false,
                            );
                            description(ui, painter, &t, &row, state == 3, 0);
                            row.rect = row.rect.translate(vec2(0.0, ROW_PITCH));
                            let background = row_background(
                                painter,
                                &t,
                                row.rect,
                                state == 2,
                                state == 1,
                                state == 3,
                                false,
                            );
                            tab.wip_description(painter, &t, &row, background);
                        });
                });
                output.textures_delta.clear();
                for label in ["faintbody", "1 conflict", "1 staged", "1 unstaged"] {
                    theme::test_paint::assert_text_aa(
                        &output.shapes,
                        t.dock,
                        label,
                        &format!("{} state {state}", builtin.name),
                    );
                }
            }
        }
    }
}
