//! Virtualized graph viewport, rows and scroll geometry.

use super::*;

impl RepoTab {
    pub(in crate::repo::graph_view) fn graph_rows_view(
        &mut self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
    ) {
        self.graph_motion.expire();
        self.graph_motion.schedule_expiry(ui.ctx());
        self.celebration_ui(ui);
        self.graph_walk_header(ui);
        self.graph_stacks(ui, acts);
        let map_area = ui.available_rect_before_wrap();
        let map = crate::settings::Settings::history_map(ui.ctx());
        let map_rect = egui::Rect::from_min_max(
            egui::pos2(map_area.right() - map.width(), map_area.top()),
            map_area.right_bottom(),
        );
        let show_map = map != crate::settings::HistoryMap::Hidden
            && map_area.width() > 480.0
            && map_area.height() > 120.0;
        let graph_rect = egui::Rect::from_min_max(
            map_area.min,
            egui::pos2(
                map_area.right() - if show_map { map.width() + 6.0 } else { 0.0 },
                map_area.bottom(),
            ),
        );
        // Earlier full-width strips have already grown the parent UI's minimum width.
        // A fresh child enforces the reservation for layout, clipping and hit testing.
        let scroll_geometry = ui
            .scope_builder(egui::UiBuilder::new().max_rect(graph_rect), |ui| {
                self.graph_rows_content(ui, acts)
            })
            .inner;
        // The reserved minimap strip is a sibling of the horizontally clipped graph.
        if show_map
            && !self.ghost.drawing()
            && let Some((offset, viewport)) = scroll_geometry
        {
            self.history_minimap(ui, map_rect, offset, viewport);
        }
    }

    pub(in crate::repo::graph_view) fn graph_rows_content(
        &mut self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
    ) -> Option<(f32, f32)> {
        let t = Tokens::get(ui.ctx());
        if self.search.open {
            self.search_bar(ui, acts);
        }
        if let Some(err) = &self.load_error {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.colored_label(
                    t.danger,
                    crate::git::display_error(&crate::forge::inbox::display_error(err)),
                );
            });
            return None;
        }
        self.tip_bar(ui);
        let first_steps = self.first_steps();
        if self.commits.is_empty() {
            if let Some(steps) = first_steps {
                self.first_steps_view(ui, steps, acts);
                return None;
            }
            ui.centered_and_justified(|ui| {
                if self.loading {
                    ui.add(crate::spinner::Spinner::new());
                } else {
                    ui.label(
                        egui::RichText::new(crate::i18n::text("repo-core-4137")).color(t.text_dim),
                    );
                }
            });
            return None;
        }

        // Keyboard navigation (only when no text field has focus, nor a window of the app's
        // is over the tab: the manual, the palette, the clone form).
        let ghost = self.ghost.drawing();
        // The rows are one Tab stop (see `a11y::list_stop`), whose ↑ / ↓ are these.
        let rows_id = ui.id().with("graph-rows");
        let keys_free =
            !ui.ctx().egui_wants_keyboard_input() || crate::a11y::list_has_focus(ui.ctx(), rows_id);
        if !ghost
            && self.keyboard_graph_active()
            && keys_free
            && self.dialog.is_none()
            && self.package.is_none()
            && !self.stacks.modal_open()
            && !self.keys_blocked
        {
            let ringed = ui.data(|d| d.get_temp::<usize>(rows_id));
            self.arrow_keys(ui, ringed, acts);
        }

        let settings = Settings::get(ui.ctx());
        let lanes = lane_layout(&settings, self.graph.width);
        let mut prefs = preferred_cols(&settings, lanes);
        let available = ui.available_width();
        let mut cols = prefs.allocated(available, settings.description_space);
        let scroll_geometry = egui::ScrollArea::horizontal()
            .id_salt("graph-horizontal")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.set_width(cols.content_width(available));

                // Column headers.
                let (header, header_resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::CLICK);

                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("graph_scroll")
                    .auto_shrink(false);
                let offset_id = ui.id().with("graph_offset");
                let offset_now = ui.data(|d| d.get_temp::<f32>(offset_id)).unwrap_or(0.0);
                match self.ghost.take_shift() {
                    // Rows added on top: the rows in view stay where they are, unless the top is shown.
                    Some(Shift::In(rows)) if offset_now > 0.0 => {
                        let by = rows as f32 * ROW_PITCH;
                        self.ghost.shifted = by;
                        scroll = scroll.vertical_scroll_offset(offset_now + by);
                    }
                    Some(Shift::Out(by)) => {
                        scroll = scroll.vertical_scroll_offset((offset_now - by).max(0.0));
                    }
                    _ => {}
                }
                if let Some(row) = self.take_graph_scroll_row() {
                    let follow_head = self.graph_motion.take_head_scroll();
                    let view_h = ui.available_height();
                    let top = row as f32 * ROW_PITCH;
                    // Only scroll if the row is outside the visible part.
                    let offset = ui.data(|d| d.get_temp::<f32>(offset_id)).unwrap_or(0.0);
                    if top < offset || top + ROW_PITCH > offset + view_h {
                        let target = (top - view_h / 2.0).max(0.0);
                        let offset = if follow_head {
                            self.graph_motion.start_scroll(offset, target, ui.ctx())
                        } else {
                            self.graph_motion.cancel_scroll();
                            target
                        };
                        scroll = scroll.vertical_scroll_offset(offset);
                    }
                }

                if ui.input(|i| i.smooth_scroll_delta.y != 0.0 || i.pointer.any_pressed()) {
                    self.graph_motion.cancel_scroll();
                }
                if let Some(offset) = self.graph_motion.scroll_offset(ui.ctx()) {
                    scroll = scroll.vertical_scroll_offset(offset);
                }
                if let Some(offset) = self.history_minimap.offset.take() {
                    self.graph_motion.cancel_scroll();
                    scroll = scroll.vertical_scroll_offset(offset);
                }
                // `show_rows` sizes rows as ROW_PITCH + item spacing, so the spacing must be zero here.
                ui.spacing_mut().item_spacing.y = 0.0;
                let idle = self.busy_op.is_none();
                let filtered = self.graph_projection_displayed();
                let total = if filtered {
                    self.saved_views.len()
                } else {
                    self.commits.len() + usize::from(self.has_more)
                };
                // The merges shown, for their badges: only those are read.
                let mut shown_merges = Vec::new();
                // A new repository's first steps go under its few rows.
                let first_steps = first_steps.filter(|_| !self.guide.steps_hidden);
                if first_steps.is_some() {
                    scroll = scroll.max_height(total as f32 * ROW_PITCH);
                }
                self.test_lane_frame(&settings, ui.ctx());
                let mut hovered_message = None;
                // The rows on screen, whose files are read ahead (see `prefetch_files`).
                let mut on_screen = None;
                let out = scroll.show_rows(ui, ROW_PITCH, total, |ui, range| {
                    // show_rows includes overscan and partially clipped rows. Require the text's
                    // centre to be inside the viewport with enough room for its glyphs: a label on
                    // a mostly hidden top row would leave the visible lane looking unnamed.
                    let top = ui.max_rect().top();
                    let clip = ui.clip_rect();
                    let mut visible = range.clone().filter(|i| {
                        let y = top + (i - range.start) as f32 * ROW_PITCH;
                        let center = y + ROW_PITCH / 2.0;
                        center >= clip.top() + 8.0 && center <= clip.bottom() - 8.0
                    });
                    let first = visible.clone().next().unwrap_or(range.start);
                    let end = visible.next_back().map_or(first, |i| i + 1);
                    on_screen = Some(first..end);
                    let readable = if filtered {
                        self.readable_rows(
                            ui,
                            (first..end).filter_map(|i| match self.saved_views.row(i) {
                                Some(crate::repo::saved_views::Row::Commit(raw)) => Some(*raw),
                                _ => None,
                            }),
                            lanes.shown,
                        )
                    } else {
                        self.readable_viewport(ui, first..end, lanes.shown)
                    };
                    if settings.fit_visible_refs() {
                        prefs.refs = readable.refs_width;
                    }
                    cols = prefs.allocated(available, settings.description_space);
                    ui.set_width(cols.content_width(available));
                    // Near the end: fetch the next page of history.
                    // Not from the ghost graph, whose rows don't grow as pages come in.
                    if !filtered
                        && range.end + 40 >= self.commits.len()
                        && self.has_more
                        && !self.loading_more
                        && !ghost
                    {
                        acts.push(Action::LoadMore);
                    }
                    self.drag_auto_scroll(ui);
                    // Tab reaches the selected row, else the first in view.
                    let visible_raw = |projected| {
                        if filtered {
                            match self.saved_views.row(projected) {
                                Some(crate::repo::saved_views::Row::Commit(raw)) => Some(*raw),
                                _ => None,
                            }
                        } else {
                            Some(projected)
                        }
                    };
                    let stop = self
                        .selected
                        .filter(|s| {
                            let projected = if filtered {
                                self.saved_views.position(*s)
                            } else {
                                Some(*s)
                            };
                            projected.is_some_and(|p| range.contains(&p))
                        })
                        .or_else(|| range.clone().find_map(visible_raw))
                        .unwrap_or(0);
                    // Where ↑ / ↓ start from while nothing is selected: the row Tab rings.
                    ui.data_mut(|d| d.insert_temp(rows_id, stop));
                    let menu_row = ui.data(|d| d.get_temp::<String>(menu_row_id()));
                    let mut menu_drawn = false;
                    for projected in range {
                        let i = if filtered {
                            match self.saved_views.row(projected) {
                                Some(crate::repo::saved_views::Row::Commit(raw)) => *raw,
                                Some(crate::repo::saved_views::Row::Gap { range, top, bottom }) => {
                                    if connector_row(ui, &t, cols, lanes, range.len(), top, bottom)
                                        .clicked()
                                    {
                                        self.expand_graph_span(range.clone());
                                    }
                                    continue;
                                }
                                None => continue,
                            }
                        } else {
                            projected
                        };
                        if i >= self.commits.len() {
                            loading_row(ui, &t);
                            continue;
                        }
                        if !ghost {
                            if self.commits[i].parents.len() == 2 {
                                shown_merges.push(self.commits[i].hash.clone());
                            }
                            let (resp, hovered) =
                                self.commit_row(ui, i, cols, lanes, &readable, i == stop, acts);
                            if hovered.is_some() {
                                hovered_message = hovered;
                            }
                            if i == stop {
                                crate::a11y::list_stop(&resp, rows_id, self.selected == Some(i));
                            }
                            self.keyboard_graph_focus(&resp);
                            // A merge put aside, double-clicked: brought back (from any branch).
                            if resp.double_clicked()
                                && let Some(parked) = self.parked_at(i)
                            {
                                acts.push(Action::Resolver(
                                    crate::repo::resolver::ResolverAct::BringBack(
                                        parked.id.clone(),
                                    ),
                                ));
                            }
                            if resp.secondary_clicked() {
                                let hash = self.commits[i].hash.clone();
                                ui.data_mut(|d| d.insert_temp(menu_row_id(), hash));
                            }
                            self.row_menu(&resp, i, idle, acts);
                            continue;
                        }
                        // The ghost graph is only looked at: its rows do nothing, but the menu open
                        // on a real commit (whose entry is being previewed) stays open.
                        let resp = self.ghost_row(ui, i, cols, lanes, &readable);
                        if !self.is_ghost_row(i) {
                            menu_drawn |=
                                menu_row.as_deref() == Some(self.commits[i].hash.as_str());
                            self.row_menu(&resp, i, idle, acts);
                        }
                    }
                    if ghost && !menu_drawn {
                        self.keep_menu_open(ui, menu_row.as_deref(), idle, acts);
                    }
                });
                ui.data_mut(|d| d.insert_temp(offset_id, out.state.offset.y));
                if let Some(rows) = on_screen.filter(|_| !filtered && !ghost) {
                    self.prefetch_files(rows, ui.ctx());
                }
                if let Some(steps) = first_steps {
                    self.first_steps_view(ui, steps, acts);
                }
                self.want_remerge_badges(&shown_merges, ui.ctx());
                let graph_label = crate::repo::graph_order::header_label(self.log_order);
                header::column_headers(ui, &t, header, &cols, &graph_label);
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        painted_columns_id(),
                        PaintedColumns {
                            header,
                            prefs,
                            cols,
                        },
                    );
                });
                if self.test_lane.visible {
                    self.test_lane_header(ui, header, &cols, acts);
                }
                self.column_handles(ui, header, &prefs, &cols, &settings);
                header::header_menu(&header_resp, &settings);
                order::order_button(
                    ui,
                    header,
                    &cols,
                    (&graph_label, self.log_order),
                    &settings,
                    acts,
                );
                self.hover_graph_message(
                    ui.ctx(),
                    if ghost {
                        None
                    } else {
                        hovered_message.as_deref()
                    },
                );
                (out.state.offset.y, out.inner_rect.height())
            })
            .inner;
        Some(scroll_geometry)
    }
}
