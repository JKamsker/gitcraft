//! The file view's Blame tab: who last changed each line, newer changes marked brighter.

use std::path::Path;

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::error;
use crate::git::{self, BlameLine};
use crate::repo::find::{self, Layout, Seen, Source, SourceKey};
use crate::repo::highlight::{self, Spans};
use crate::repo::{Action, RepoTab, file_text};
use crate::theme::{self, Tokens};

const LINE_H: f32 = 19.0;
const GUTTER_W: f32 = 270.0;
const NUM_W: f32 = 46.0;

pub(crate) struct BlameData {
    lines: Vec<BlameLine>,
    spans: Option<Vec<Spans>>,
    /// Oldest and newest commit time, for the age colour.
    span: (i64, i64),
    /// Characters in the longest line, for the width to scroll (counted once, not every frame).
    longest: usize,
}

/// Runs `git blame` and colours the lines; on a background thread.
pub(crate) fn load_blame(
    repo: &Path,
    rev: Option<&str>,
    path: &str,
    dark: bool,
) -> Result<BlameData, String> {
    let mut lines = git::blame(repo, rev, path)?;
    let mut spans = highlight::highlight(path, lines.iter().map(|l| l.text.as_str()), dark);
    for (i, line) in lines.iter_mut().enumerate() {
        let _ = highlight::cap_line(&mut line.text, spans.as_mut().and_then(|s| s.get_mut(i)));
    }
    let times = lines.iter().filter(|l| !l.uncommitted()).map(|l| l.time);
    let span = (times.clone().min().unwrap_or(0), times.max().unwrap_or(0));
    // A tab is drawn four spaces wide (the diff expands them; blame keeps them).
    let longest = lines
        .iter()
        .map(|l| l.text.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum())
        .max()
        .unwrap_or(0);
    Ok(BlameData {
        lines,
        spans,
        span,
        longest,
    })
}

impl RepoTab {
    pub(super) fn blame_lines(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &self.file_view else { return };
        let data = match &view.blame {
            None => {
                ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
                return;
            }
            Some(Err(e)) => {
                error(ui, e);
                return;
            }
            Some(Ok(d)) => d,
        };
        let now = git::now();
        let font = theme::mono(12.0);
        let char_w = ui
            .painter()
            .layout_no_wrap("M".to_owned(), font.clone(), t.text)
            .size()
            .x;
        let longest = data.longest as f32;
        let width = (GUTTER_W + NUM_W + 16.0 + longest * char_w + 24.0).max(ui.available_width());
        let find = &mut self.find;
        if find.open {
            let key = SourceKey {
                what: Source::Blame {
                    path: view.path.clone(),
                    rev: view.rev.clone(),
                },
                at: data.lines.as_ptr() as usize,
                len: data.lines.len(),
            };
            let text = |i: usize| data.lines.get(i).map(|l| l.text.as_str());
            find.update(ui.ctx(), key, data.lines.len(), text, Layout::Lines);
        }
        let mut scroll = egui::ScrollArea::both()
            .id_salt(("file_blame", &view.path, &view.rev))
            .auto_shrink(false);
        if let Some(line) = view.goto_line.take() {
            let row = line.clamp(1, data.lines.len().max(1)) - 1;
            scroll = scroll.vertical_scroll_offset(row.saturating_sub(5) as f32 * LINE_H);
        }
        if let Some(hit) = find.take_reveal() {
            let text = data.lines.get(hit.line).map_or("", |l| l.text.as_str());
            scroll = find.scroll_to(scroll, &hit, text, char_w);
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let found = &*find;
        let out = scroll.show_rows(ui, LINE_H, data.lines.len(), |ui, range| {
            for i in range {
                let Some(line) = data.lines.get(i) else {
                    continue;
                };
                let (rect, _) = ui.allocate_exact_size(vec2(width, LINE_H), Sense::hover());
                let clip = rect.intersect(ui.clip_rect());
                let p = ui.painter().with_clip_rect(clip);
                let first = i == 0 || data.lines.get(i - 1).map(|l| &l.hash) != Some(&line.hash);
                // Each change starts with a thin rule, so blocks read as one.
                if first && i > 0 {
                    p.hline(rect.x_range(), rect.top(), Stroke::new(1.0, t.separator));
                }
                if view.marked_line == Some(i + 1) {
                    p.rect_filled(rect, 0.0, t.row_selected);
                }
                let gutter = Rect::from_min_size(rect.min, vec2(GUTTER_W, LINE_H));
                p.rect_filled(gutter, 0.0, t.card);
                // Who wrote it, as its commit records: at the gutter's right edge.
                if let Some(a) = self.authorship.map.get(&line.hash) {
                    let edge = Rect::from_min_size(
                        pos2(gutter.right() - 4.0, rect.top()),
                        vec2(3.0, LINE_H),
                    );
                    crate::repo::authorship_view::stripe(
                        &p,
                        &t,
                        edge,
                        a.line(&line.file, line.orig),
                        0.0,
                    );
                }
                p.rect_filled(
                    Rect::from_min_size(rect.min, vec2(3.0, LINE_H)),
                    0.0,
                    age_color(&t, line, data.span),
                );
                let mid = rect.center().y;
                // One Tab stop per block of lines of a commit, not one per line.
                let stop = first && !line.uncommitted();
                let sense = crate::a11y::list_row_sense(stop);
                let gutter_resp = ui.interact(gutter, ui.id().with(("blame", i)), sense);
                if stop {
                    crate::repo::widgets::a11y(&gutter_resp, egui::WidgetType::Button, || {
                        let short = line.hash.get(..7).unwrap_or(&line.hash);
                        format!("line {}, go to commit {short}", i + 1)
                    });
                }
                if first {
                    let small = FontId::proportional(11.5);
                    if line.uncommitted() {
                        p.text(
                            pos2(gutter.left() + 10.0, mid),
                            Align2::LEFT_CENTER,
                            "Not committed yet",
                            small,
                            t.warning,
                        );
                    } else {
                        let hash = p.text(
                            pos2(gutter.left() + 10.0, mid),
                            Align2::LEFT_CENTER,
                            line.hash.get(..7).unwrap_or(&line.hash),
                            theme::mono(11.0),
                            if gutter_resp.hovered() {
                                t.accent_fg
                            } else {
                                t.text_faint
                            },
                        );
                        let when = p.text(
                            pos2(gutter.right() - 8.0, mid),
                            Align2::RIGHT_CENTER,
                            git::relative_time(line.time, now),
                            small.clone(),
                            t.text_faint,
                        );
                        p.with_clip_rect(Rect::from_min_max(
                            pos2(hash.right() + 8.0, rect.top()),
                            pos2(when.left() - 8.0, rect.bottom()),
                        ))
                        .text(
                            pos2(hash.right() + 10.0, mid),
                            Align2::LEFT_CENTER,
                            &line.author,
                            small,
                            t.text_dim,
                        );
                    }
                }
                if !line.uncommitted() {
                    let gutter_resp = gutter_resp.on_hover_text(format!(
                        "{} · {}\n{}\nClick to show the commit",
                        line.author,
                        git::relative_time(line.time, now),
                        line.summary
                    ));
                    if gutter_resp.clicked() {
                        acts.push(Action::CloseFileView);
                        acts.push(Action::SelectHash(line.hash.clone()));
                    }
                }
                p.text(
                    pos2(gutter.right() + NUM_W - 8.0, mid),
                    Align2::RIGHT_CENTER,
                    (i + 1).to_string(),
                    theme::mono(11.0),
                    t.text_faint,
                );
                let x = gutter.right() + NUM_W + 8.0;
                let text_response = ui.interact(
                    Rect::from_min_max(pos2(x, rect.top()), rect.right_bottom()),
                    ui.id().with(("blame-dossier", i)),
                    Sense::click(),
                );
                egui::Popup::context_menu(&text_response)
                    .style(crate::theme::menu_style)
                    .show(|ui| {
                        if !line.uncommitted()
                            && let Ok(path) = git::investigations::dossier::quoted_path(&line.file)
                            && ui.button("Explain history").clicked()
                        {
                            acts.push(Action::OpenLineDossier(Some(
                                git::investigations::dossier::Seed {
                                    commit: line.hash.clone(),
                                    path,
                                    line: line.orig as usize,
                                },
                            )));
                        }
                    });
                let spans = data.spans.as_ref().and_then(|s| s.get(i));
                let galley = file_text::line_galley(ui, spans, &line.text, &font, t.text);
                find::paint_line(&p, (x, mid, LINE_H), galley, found.marks(i), &t);
            }
        });
        self.find.saw(Seen {
            x: out.state.offset.x,
            y: out.state.offset.y,
            height: out.inner_rect.height(),
            text_w: out.inner_rect.width() - GUTTER_W - NUM_W - 8.0,
            line_h: LINE_H,
        });
    }
}

/// Newer lines get a stronger accent bar; uncommitted ones the warning colour.
fn age_color(t: &Tokens, line: &BlameLine, (oldest, newest): (i64, i64)) -> Color32 {
    if line.uncommitted() {
        return t.warning;
    }
    let f = if newest > oldest {
        (line.time - oldest) as f32 / (newest - oldest) as f32
    } else {
        1.0
    };
    t.accent.gamma_multiply(0.15 + 0.85 * f)
}
