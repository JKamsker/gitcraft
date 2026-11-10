use super::*;

impl Rows<'_> {
    /// One side-by-side row: the old line on the left half, the new one on the right, each
    /// `half` wide within the visible `view` and scrolled sideways by up to `overflow`.
    pub(super) fn split_row(
        &mut self,
        ui: &mut Ui,
        row: usize,
        view: Rect,
        dimensions: (f32, f32, f32),
        acts: &mut Vec<Action>,
    ) {
        let (half, overflow, height) = dimensions;
        let Some((l, r)) =
            super::super::presentation::DisplayRows::new(self.data, true).patch_lines(row)
        else {
            return;
        };
        let (rect, _) = ui.allocate_exact_size(vec2(half * 2.0 + overflow, height), Sense::hover());
        let scroll_x = (view.left() - rect.left()).clamp(0.0, overflow);
        let left =
            Rect::from_min_size(pos2(rect.left() + scroll_x, rect.top()), vec2(half, height));
        let right = Rect::from_min_size(pos2(left.right(), rect.top()), vec2(half, height));
        // Hunk headers and notes span both halves.
        if l == r
            && let Some(i) = l
            && matches!(
                self.data.lines.get(i).map(|x| x.kind),
                Some(Kind::Hunk | Kind::Meta | Kind::Fold)
            )
        {
            self.line(ui, left.union(right), i, Side::Both, 0.0, acts);
            return;
        }
        match l {
            Some(i) => self.line(ui, left, i, Side::Old, scroll_x, acts),
            None => filler(ui, left, &self.t),
        }
        match r {
            Some(i) => self.line(ui, right, i, Side::New, scroll_x, acts),
            None => filler(ui, right, &self.t),
        }
        ui.painter().line_segment(
            [right.left_top(), right.left_bottom()],
            Stroke::new(1.0, self.t.separator),
        );
    }

    /// Line `i` in `rect`, where it shows on `side`, with what can be done on it.
    pub(super) fn line(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        i: usize,
        side: Side,
        scroll_x: f32,
        acts: &mut Vec<Action>,
    ) {
        let data = self.data;
        let kind = data.lines.get(i).map(|l| l.kind);
        let pickable = self.partial && matches!(kind, Some(Kind::Add | Kind::Del));
        let spot = self.comment_spot(i, side);
        let active = spot.is_some_and(|spot| self.is_target(spot));
        // A line a pull-request comment is written on shows picked; one a note is written on
        // doesn't, in a diff where picking a line is staging it.
        let local = self.review.is_some_and(|r| r.local);
        let picked = self.view.chosen.contains(&i) || (active && !local);
        // Hunk headers aren't copied, so they don't look selected either.
        let sel_chars = self
            .text_sel
            .and_then(|s| s.chars_of(i, side))
            .filter(|_| kind.is_some_and(|k| side.shows(k)));
        let found = self
            .find
            .and_then(|f| f.marks(i))
            .filter(|_| kind.is_some_and(|k| side.shows(k)));
        let placed = paint_line(
            ui,
            rect,
            data,
            i,
            side,
            scroll_x,
            &self.font,
            &self.t,
            picked,
            sel_chars,
            found,
            self.geometry.as_ref().map(|g| g.text_width),
        );
        if let Some(who) = data.authors.get(i).copied().flatten() {
            crate::repo::authorship_view::stripe(ui.painter(), &self.t, rect, who, 4.0);
        }
        if self.view.keyboard_hunk == Some(i) && self.data.lines[i].kind == Kind::Hunk {
            ui.painter().with_clip_rect(ui.clip_rect()).rect_stroke(
                rect.shrink(1.0),
                0.0,
                Stroke::new(2.0, self.t.accent),
                egui::StrokeKind::Inside,
            );
        }
        // Where the drag started and where the pointer is, in this line's text.
        if let Some((origin, galley)) = placed.filter(|_| kind.is_some_and(|k| side.shows(k))) {
            let text_rect = rect.with_min_x(rect.left() + gutter_width(side) + MARK_W);
            self.hit_test(
                text_rect.intersect(ui.clip_rect()),
                (i, side),
                origin,
                &galley,
            );
            if let Some(s) = self.text_sel.filter(|s| s.side == side && s.is_empty())
                && s.cursor.0 == i
            {
                let caret = galley.pos_from_cursor(egui::text::CCursor::new(s.cursor.1));
                let caret = caret.translate(origin.to_vec2());
                ui.painter()
                    .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                    .line_segment(
                        [caret.left_top(), caret.left_bottom()],
                        Stroke::new(1.0, self.t.text),
                    );
            }
        }
        if pickable {
            let marker = Rect::from_min_size(
                pos2(rect.left() + gutter_width(side), rect.top()),
                vec2(MARK_W, rect.height()),
            );
            let resp = ui
                .interact(marker, ui.id().with(("diff_line", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(crate::i18n::text("diff-pick-line"));
            crate::a11y::a11y_selected(&resp, || line_label(&data.lines[i]), picked);
            // A right-click on a changed line not picked picks it, for its menu to be about it.
            let unpicked = !self.view.chosen.contains(&i);
            if resp.clicked() || resp.secondary_clicked() && unpicked {
                self.clicked = Some(i);
            }
            self.picks.push(resp);
        }
        {
            let resp = ui.interact(
                rect,
                ui.id().with(("diff_read_line", i, column_of(side))),
                Sense::hover(),
            );
            crate::a11y::a11y(&resp, egui::WidgetType::Label, || {
                line_label(&data.lines[i])
            });
        }
        // Numbers take comments independently of both marker and code interaction.
        if let Some(spot) = spot {
            self.comment_gutter(ui, rect, i, side, spot, acts);
        }
        let mut right = rect.right().min(ui.clip_rect().right()) - 6.0;
        if kind == Some(Kind::Hunk) && !self.hunk_picks.is_empty() {
            right = self.hunk_buttons(ui, rect, i, acts) - 8.0;
        }
        if kind == Some(Kind::Hunk)
            && let DiffKey::Commit { hash, path } = &self.view.key
        {
            let next = data.lines[i + 1..]
                .iter()
                .take_while(|l| l.kind != Kind::Hunk)
                .collect::<Vec<_>>();
            let new = next
                .iter()
                .filter(|l| l.kind == Kind::Add)
                .filter_map(|l| l.new)
                .collect::<Vec<_>>();
            let old = next
                .iter()
                .filter(|l| l.kind == Kind::Del)
                .filter_map(|l| l.old)
                .collect::<Vec<_>>();
            let alternate = if !new.is_empty() {
                old.first()
                    .zip(old.last())
                    .map(|(&first, &last)| (first, last))
            } else {
                None
            };
            let (numbers, is_old) = if new.is_empty() {
                (old, true)
            } else {
                (new, false)
            };
            if let (Some(first), Some(last)) = (numbers.first(), numbers.last()) {
                let (button, left) = text_button(
                    ui,
                    right,
                    rect.center().y,
                    crate::i18n::text("repo-core-2306"),
                    self.t.accent,
                    ui.id().with(("prompt-hunk", i)),
                );
                if button
                    .on_hover_text(crate::i18n::text("repo-core-2308"))
                    .clicked()
                {
                    acts.push(Action::ShowPromptHunk(crate::repo::prompt_session::Hunk {
                        commit: hash.clone(),
                        path: path.clone(),
                        range: (*first, *last),
                        old: is_old,
                        alternate,
                    }));
                }
                right = left - 8.0;
            }
        }
        if kind == Some(Kind::Hunk)
            && let Some(tip) = data.weakened.get(&i)
        {
            super::super::weakened::badge(ui, right, rect.center().y, tip, i);
            right -= 24.0;
        }
        if matches!(kind, Some(Kind::Hunk | Kind::Fold)) {
            // Right of where the header's own text ends.
            let gutter = if side == Side::Unified {
                NUM_W * 2.0
            } else {
                0.0
            };
            let text = data.lines.get(i).map_or(String::new(), |l| l.text.clone());
            let text_w = ui
                .painter()
                .layout_no_wrap(text, self.font.clone(), self.t.text)
                .size()
                .x;
            let left = (rect.left() + gutter + 24.0 + text_w + 12.0).max(ui.clip_rect().left());
            super::super::fold_view::controls(ui, data, i, (left, right, rect.center().y), acts);
        }
        if kind.is_some_and(|k| side.shows(k)) {
            self.moved_button(ui, rect, i, acts);
        }
    }
}
