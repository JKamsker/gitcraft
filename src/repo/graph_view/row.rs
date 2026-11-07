//! One commit row of the graph: its background, the description and the hash, author, when
//! and date columns.

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Stroke};
use egui::{Ui, pos2, vec2};

use super::badge::Badge;
use super::{AVATAR_D, ROW_GAP, ROW_H, ROW_PITCH};
use crate::a11y::labels;
use crate::git::{self, Commit};
use crate::repo::columns::Cols;
use crate::repo::test_lane::LANE_W;
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
    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_row(
        &self,
        ui: &mut Ui,
        i: usize,
        cols: Cols,
        lanes: Lanes,
        now: i64,
        tab_stop: bool,
        acts: &mut Vec<Action>,
    ) -> Response {
        let t = Tokens::get(ui.ctx());
        let (auto_id, rect) = ui.allocate_space(vec2(ui.available_width(), ROW_PITCH));
        let sense = crate::a11y::list_row_sense(tab_stop);
        let Some(commit) = self.commits.get(i) else {
            return ui.interact(rect, auto_id, sense);
        };
        let is_wip = commit.hash == WIP_HASH;
        // Commits are dragged (the changes row and stashes aren't): egui tells a drag from a
        // click once the pointer moves a few points, so clicks stay clicks.
        let draggable = !is_wip && self.stash_at(i).is_none();
        let sense = if draggable {
            sense | Sense::DRAG
        } else {
            sense
        };
        // Known by its commit, not its place: rows shift when a reload adds commits or the
        // changes row above them, and a menu opened on one must stay that commit's.
        let resp = ui.interact(rect, row_id(ui, &commit.hash), sense);
        let label = || self.row_label(commit, now);
        widgets::a11y_selected(&resp, label, self.selected == Some(i));
        let painter = ui.painter_at(rect);
        let is_match = self.search.matches.contains(&commit.hash);
        row_background(
            &painter,
            &t,
            rect,
            self.selected == Some(i),
            resp.hovered(),
            is_match,
            self.is_compared(commit, is_wip),
        );
        let resp = match self.bisect_mark(&painter, &t, rect, &commit.hash) {
            Some(said) => resp.on_hover_text(said),
            None => resp,
        };
        let resp = match self.parallel_mark(&painter, &t, rect, &commit.hash) {
            Some(said) => resp.on_hover_text(said),
            None => resp,
        };

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

        let (resp, pill) = self.refs_cell(ui, &painter, &row, resp, acts);
        self.graph_cell(ui, &painter, &row, lanes.shown);
        if is_wip {
            self.wip_description(&painter, &t, &row);
        } else {
            let ci = self.ci_status(&commit.hash);
            let robot = self.authorship.of(commit).map_or(0.0, |a| {
                let left = row.gx + row.cols.graph + 6.0;
                let id = ui.id().with(("robot", &commit.hash));
                crate::repo::authorship_view::robot(ui, left, row.mid(), &a, id)
            });
            let lane = if self.test_lane.visible { LANE_W } else { 0.0 };
            // With the lane, the CI dot's place is kept on every row, so the lane is straight.
            let dots = if lane > 0.0 || ci.is_some() {
                14.0
            } else {
                0.0
            } + lane;
            let dot_w = dots;
            // Only where the description has room for it beside some of the summary.
            let room = rect.right() - cols.trailing() - dot_w - (gx + cols.graph);
            let resolved = self
                .remerge_badge(commit)
                .and_then(|b| b.fit(&painter, room).map(|(label, w)| (b, label, w)));
            let badge_w = resolved.map_or(0.0, |(_, _, w)| w + 6.0);
            let end = description(ui, &painter, &t, &row, is_match, dot_w + badge_w, robot);
            if self.ghost.kind(i) == crate::repo::ghost::Kind::Going {
                // Left on no ref by the operation a ghost run shows: struck through.
                let x = row.gx + row.cols.graph + 6.0;
                painter.line_segment(
                    [pos2(x, row.mid()), pos2(end, row.mid())],
                    Stroke::new(1.2, t.text),
                );
            }
            trailing_columns(ui, &painter, &t, &row, now);
            if let Some(status) = ci {
                let center = pos2(rect.right() - cols.trailing() - 9.0, row.mid());
                let id = ui.id().with(("ci-dot", &commit.hash));
                acts.extend(ci_status::dot_in(
                    ui,
                    id,
                    center,
                    &commit.hash,
                    status,
                    tab_stop,
                ));
            }
            if let Some(badge) = resolved {
                let end = pos2(rect.right() - cols.trailing() - 6.0 - dot_w, row.mid());
                self.show_remerge_badge(ui, &painter, (i, &commit.hash), badge, end, acts);
            }
            if lane > 0.0 {
                let center = pos2(rect.right() - cols.trailing() - 9.0 - 14.0, row.mid());
                let id = ui.id().with(("test-dot", &commit.hash));
                acts.extend(self.test_dot(ui, id, center, &commit.tree));
            }
        }
        if draggable {
            self.row_drag_drop(ui, &resp, &row, pill.as_ref(), acts);
        }
        resp
    }

    /// What a screen reader says for the row of `commit`: the changes row's counts, a stash's
    /// message, or a commit's hash, summary, author, age and branches.
    fn row_label(&self, commit: &Commit, now: i64) -> String {
        if commit.hash == WIP_HASH {
            let w = &self.work;
            return labels::changes(w.conflicted.len(), w.staged.len(), w.unstaged.len());
        }
        let when = git::relative_time(commit.time, now);
        if let Some(stash) = self.stashes.iter().find(|s| s.hash == commit.hash) {
            return labels::join([labels::stash(&stash.name, &stash.message), when]);
        }
        let refs: Vec<String> = self
            .graph_badges(commit)
            .iter()
            .map(Badge::spoken)
            .collect();
        let short = crate::repo::short(&commit.hash);
        labels::commit(short, &commit.summary, &commit.author, &when, &refs)
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
    fn wip_description(&self, painter: &Painter, t: &Tokens, row: &Row) {
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
                x += pill(painter, pos2(x, mid), &format!("{n} {label}"), color) + 6.0;
            }
        }
    }
}

impl RepoTab {
    /// Tints the row of a commit the bisect going on marked (good, bad, skipped, or the first
    /// bad one it found) and gives what it was marked, to show on hover.
    fn bisect_mark(&self, painter: &Painter, t: &Tokens, rect: Rect, hash: &str) -> Option<String> {
        let bisect = self.bisect.state()?;
        let culprit = self.bisect.is_culprit(hash);
        let mark = self.bisect.mark_of(hash);
        let (color, said) = match (culprit, mark) {
            (true, _) => (
                t.danger,
                "The first bad commit, as the bisect found".to_owned(),
            ),
            (false, Some(mark)) => {
                let word = match mark {
                    git::Mark::Skip => "skipped",
                    _ => bisect.word(mark),
                };
                (
                    crate::repo::bisect::mark_color(t, mark),
                    format!("Marked {word} in the bisect"),
                )
            }
            (false, None) => return None,
        };
        let band = rect.shrink2(vec2(0.0, ROW_GAP / 2.0));
        let tint = if culprit { 0.16 } else { 0.08 };
        painter.rect_filled(band, 0.0, color.gamma_multiply(tint));
        painter.rect_filled(Rect::from_min_size(band.min, vec2(3.0, ROW_H)), 0.0, color);
        Some(said)
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
) {
    // The row's own band, without the gap.
    let band = rect.shrink2(vec2(0.0, ROW_GAP / 2.0));
    if selected {
        painter.rect_filled(band, 0.0, t.row_selected);
    } else if hovered {
        painter.rect_filled(band, 0.0, t.hover);
    } else if is_match {
        painter.rect_filled(band, 0.0, t.warning.gamma_multiply(0.10));
    }
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
}

/// The summary, then the rest of the message faded, cut with an ellipsis.
/// `reserved`: room left at the end (for the build status, a merge's badge).
fn description(
    ui: &Ui,
    painter: &Painter,
    t: &Tokens,
    row: &Row,
    is_match: bool,
    reserved: f32,
    robot: f32,
) -> f32 {
    let (rect, mid) = (row.rect, row.mid());
    let x = row.gx + row.cols.graph + 6.0 + robot;
    let text_right = rect.right() - row.cols.trailing() - 10.0 - reserved;
    let font = if is_match {
        theme::medium(12.5)
    } else {
        FontId::proportional(12.5)
    };
    let text_clip = painter.with_clip_rect(Rect::from_min_max(
        pos2(x, rect.top()),
        pos2(text_right, rect.bottom()),
    ));
    let summary = text_clip.text(
        pos2(x + 2.0, mid),
        Align2::LEFT_CENTER,
        &row.commit.summary,
        font.clone(),
        t.text,
    );
    let room = text_right - summary.right() - 12.0;
    let end = summary.right().min(text_right);
    if !row.commit.body.is_empty() && room > 24.0 {
        let mut job = egui::text::LayoutJob::single_section(
            row.commit.body.clone(),
            egui::TextFormat::simple(FontId::proportional(12.5), t.text.gamma_multiply(0.5)),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        text_clip.galley(
            pos2(summary.right() + 10.0, mid - galley.size().y / 2.0),
            galley,
            t.text,
        );
    }
    end
}

/// The hash, author, when and date columns, those the settings show.
fn trailing_columns(ui: &Ui, painter: &Painter, t: &Tokens, row: &Row, now: i64) {
    let (rect, mid, cols, commit) = (row.rect, row.mid(), row.cols, row.commit);
    let small = FontId::proportional(11.5);
    if cols.hash > 0.0 {
        painter.text(
            pos2(rect.right() - cols.trailing(), mid),
            Align2::LEFT_CENTER,
            // A ghost run's merge has no commit yet, so no hash.
            if commit.hash.starts_with('\0') {
                "—"
            } else {
                commit.hash.get(..7).unwrap_or(&commit.hash)
            },
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
fn pill(painter: &Painter, left_center: Pos2, text: &str, color: Color32) -> f32 {
    let galley = painter.layout_no_wrap(text.to_owned(), theme::medium(11.0), color);
    let w = galley.size().x + 12.0;
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - 9.0), vec2(w, 18.0));
    painter.rect_filled(r, CornerRadius::same(9), color.gamma_multiply(0.18));
    painter.galley(
        pos2(r.left() + 6.0, r.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    w
}
