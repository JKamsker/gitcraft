use super::*;

impl Rows<'_> {
    pub(super) fn moved_button(&self, ui: &mut Ui, rect: Rect, i: usize, acts: &mut Vec<Action>) {
        let moves = &self.data.moves;
        let Some((b, _)) = moves.at(i) else { return };
        let Some(block) = moves.blocks.get(b) else {
            return;
        };
        let lines = &self.data.lines;
        let number = |at: usize, old: bool| {
            lines
                .get(at)
                .and_then(|l| if old { l.old } else { l.new })
                .unwrap_or(0)
        };
        let label = if i == block.del.start {
            crate::i18n::message(
                "repo-core-2309",
                &[(
                    "false",
                    format!("{}", number(block.add.start, false)).into(),
                )],
            )
        } else if i == block.add.start {
            crate::i18n::message(
                "repo-core-2310",
                &[("true", format!("{}", number(block.del.start, true)).into())],
            )
        } else {
            return;
        };
        let label = if block.exact {
            label
        } else {
            crate::i18n::message(
                "repo-core-2311",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&label).into(),
                )],
            )
        };
        let right = rect.right().min(ui.clip_rect().right()) - 6.0;
        let id = ui.id().with(("moved_btn", i));
        let (resp, _) = text_button(ui, right, rect.center().y, &label, self.t.accent, id);
        // The block whose places the panel below shows.
        if self.view.moved.as_ref().is_some_and(|o| o.block == b) {
            ui.painter().rect_stroke(
                resp.rect,
                self.t.radius_sm,
                Stroke::new(1.5, self.t.accent),
                egui::StrokeKind::Inside,
            );
        }
        if resp
            .on_hover_text(crate::i18n::text("repo-core-2313"))
            .clicked()
        {
            acts.push(Action::DiffMoved(Some(b)));
        }
    }

    /// The side and number line `i` takes a comment on where it shows on `side`, when a pull
    /// request's file is shown.
    pub(super) fn comment_spot(&self, i: usize, side: Side) -> Option<(DiffSide, u64)> {
        let review = self.review?;
        let column = column_of(side);
        self.data
            .lines
            .get(i)
            .filter(|_| side != Side::Both)
            .and_then(|l| match column {
                None => unified_spot(l.kind, l.old, l.new, &review.marks),
                _ => target_of(l.kind, l.old, l.new, column),
            })
    }

    /// Whether the comment being written is on `spot`.
    pub(super) fn is_target(&self, (side, line): (DiffSide, u64)) -> bool {
        self.review
            .and_then(|r| r.target.as_ref())
            .is_some_and(|t| t.side == side && t.line == line)
    }

    /// Notes where the press and the pointer are in the text of line `i` on `side`, laid out
    /// in `galley` from `origin`.
    pub(super) fn hit_test(
        &mut self,
        rect: Rect,
        (i, side): (usize, Side),
        origin: Pos2,
        galley: &Galley,
    ) {
        let hit = |p: Pos2| (i, side, galley.cursor_from_pos(p - origin).index.0);
        if let Some(p) = self.press.filter(|p| rect.contains(*p)) {
            self.press_hit = Some(hit(p));
        }
        if let Some(p) = self.pointer.filter(|p| rect.y_range().contains(p.y)) {
            let over = rect.x_range().contains(p.x);
            self.pointer_hits.push((hit(p), over));
            self.text_hover |= over;
        }
    }

    /// The line's gutter in a pull request's file: its comments, and a click to write one.
    pub(super) fn comment_gutter(
        &self,
        ui: &mut Ui,
        rect: Rect,
        i: usize,
        side: Side,
        (spot_side, number): (DiffSide, u64),
        acts: &mut Vec<Action>,
    ) {
        let Some(review) = self.review else { return };
        let gutter = if side == Side::Unified {
            NUM_W * 2.0
        } else {
            NUM_W
        };
        let area = Rect::from_min_size(rect.min, vec2(gutter, LINE_H));
        // Where a click picks the line (to stage it), a note takes Alt+click: the plain click
        // stays the line's.
        let alt_only = review.local && self.partial;
        let alt = ui.input(|i| i.modifiers.alt);
        let sense = if alt_only && !alt {
            Sense::hover()
        } else {
            Sense::click()
        };
        let tip = match (review.local, alt_only) {
            (false, _) => crate::i18n::text("repo-core-2314"),
            (true, false) => crate::i18n::text("repo-core-2315"),
            (true, true) => crate::i18n::text("repo-core-2316"),
        };
        let resp = ui.interact(
            area,
            ui.id().with(("diff_gutter", i, column_of(side))),
            sense,
        );
        let resp = if sense == Sense::click() {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            resp
        }
        .on_hover_text(tip);
        let count = review.marks.get(&(spot_side, number)).copied();
        let active = self.is_target((spot_side, number));
        crate::a11y::a11y(&resp, egui::WidgetType::Button, || {
            crate::i18n::message(
                "repo-core-2318",
                &[
                    ("tip", tip.to_string().into()),
                    ("number", format!("{}", number).into()),
                    ("count", (count.unwrap_or(0)).into()),
                ],
            )
        });
        paint_gutter_mark(ui, area, count, resp.hovered(), active);
        if resp.clicked() {
            let target = LineTarget {
                path: review.path.clone(),
                line: number,
                side: spot_side,
            };
            acts.push(if review.local {
                Action::Agent(crate::repo::agents::AgentAct::OpenNote(target))
            } else {
                Action::Pr(PrAct::OpenLine(target))
            });
        }
    }

    /// The hunk actions in the header at line `i`, at the visible right end, however wide the
    /// line is.
    /// The hunk's buttons right-aligned in `rect`; returns where the leftmost one starts.
    pub(super) fn hunk_buttons(
        &self,
        ui: &mut Ui,
        rect: Rect,
        i: usize,
        acts: &mut Vec<Action>,
    ) -> f32 {
        let mut right = rect.right().min(ui.clip_rect().right()) - 6.0;
        for &pick in self.hunk_picks.iter().rev() {
            let (resp, left) = hunk_button(ui, right, rect.center().y, pick, i);
            if resp.clicked() {
                acts.push(Action::PickLines {
                    pick,
                    lines: hunk_changes(&self.data.lines, i),
                });
            }
            right = left - 4.0;
        }
        right + 4.0
    }
}
