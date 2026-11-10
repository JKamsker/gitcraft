//! Atlas window controls and treemap interaction.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn atlas_window(&mut self, ctx: &egui::Context) {
        if self
            .atlas
            .as_ref()
            .is_some_and(|v| v.profile != crate::git::profile_config())
        {
            self.atlas = None;
        }
        let Some(view) = &mut self.atlas else {
            return;
        };
        if let Some(result) = view.reader.poll(&self.tx, ctx) {
            match result {
                Ok(data) => {
                    view.data = Some(data);
                    view.zoom = 0;
                    view.page = 0;
                }
                Err(e) => view.error = Some(e),
            }
        }
        let mut open = true;
        let mut refresh = false;
        let mut file = None;
        egui::Window::new(crate::i18n::text(
            "repo-workflows-atlas-atlas-window-611ebb33",
        ))
        .id(egui::Id::new(("atlas", &self.path)))
        .open(&mut open)
        .default_size(vec2(880.0, 680.0))
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::text(
                    "repo-workflows-atlas-atlas-window-1af85190",
                ));
                ui.selectable_value(
                    &mut view.size,
                    Size::Bytes,
                    crate::i18n::text("repo-workflows-atlas-atlas-window-4d800030"),
                );
                ui.selectable_value(
                    &mut view.size,
                    Size::Changes,
                    crate::i18n::text("repo-workflows-atlas-atlas-window-bc087fce"),
                );
                ui.separator();
                ui.label(crate::i18n::text(
                    "repo-workflows-atlas-atlas-window-3a9dfa58",
                ));
                ui.selectable_value(
                    &mut view.colour,
                    Colour::Churn,
                    crate::i18n::text("repo-workflows-atlas-atlas-window-1bfb5492"),
                );
                ui.selectable_value(
                    &mut view.colour,
                    Colour::Contributor,
                    crate::i18n::text("repo-workflows-atlas-atlas-window-ba512e1a"),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::text(
                    "repo-workflows-atlas-atlas-window-6e795d4d",
                ));
                ui.add(
                    egui::TextEdit::singleline(&mut view.from)
                        .hint_text(crate::i18n::text(
                            "repo-workflows-atlas-atlas-window-63bfe85d",
                        ))
                        .desired_width(145.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut view.through)
                        .hint_text(crate::i18n::text(
                            "repo-workflows-atlas-atlas-window-6fd659e0",
                        ))
                        .desired_width(155.0),
                );
                egui::ComboBox::from_id_salt("atlas-clock")
                    .selected_text(if view.clock == Clock::Author {
                        crate::i18n::text("repo-workflows-atlas-atlas-window-1556feb2")
                    } else {
                        crate::i18n::text("repo-workflows-atlas-atlas-window-4562ddd7")
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut view.clock,
                            Clock::Author,
                            crate::i18n::text("repo-workflows-atlas-atlas-window-1556feb2"),
                        );
                        ui.selectable_value(
                            &mut view.clock,
                            Clock::Committer,
                            crate::i18n::text("repo-workflows-atlas-atlas-window-4562ddd7"),
                        );
                    });
                refresh |= ui
                    .button(crate::i18n::text(
                        "repo-workflows-atlas-atlas-window-34dd32fc",
                    ))
                    .clicked();
            });
            if view.reader.busy() {
                ui.label(crate::i18n::text(
                    "repo-workflows-atlas-atlas-window-ef7caa7d",
                ));
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-atlas-atlas-window-d16ccfdf",
                    ))
                    .clicked()
                {
                    view.reader.cancel();
                }
            }
            if let Some(error) = &view.error {
                ui.colored_label(
                    crate::theme::Tokens::get(ctx).danger,
                    crate::repo::activity::display_workflow_error(error),
                );
            }
            let Some(data) = &view.data else {
                return;
            };
            ui.label(crate::i18n::message(
                "repo-workflows-atlas-period-summary",
                &[
                    ("selected", (data.selected).into()),
                    ("scanned", (data.history.commits.len()).into()),
                    ("tips", (data.history.tips).into()),
                    ("tree", (data.head.get(..8).unwrap_or(&data.head)).into()),
                    (
                        "partial",
                        (if data.history.more { "yes" } else { "no" }).into(),
                    ),
                    (
                        "shallow",
                        (if data.history.shallow { "yes" } else { "no" }).into(),
                    ),
                ],
            ));
            ui.small(crate::i18n::message(
                "repo-workflows-atlas-applied-period",
                &[
                    (
                        "from-set",
                        (if data.query.from.is_some() {
                            "yes"
                        } else {
                            "no"
                        })
                        .into(),
                    ),
                    (
                        "from",
                        (data
                            .query
                            .from
                            .map(|d| crate::i18n::date_iso(&d.to_string()))
                            .unwrap_or_default())
                        .into(),
                    ),
                    (
                        "through-set",
                        (if data.query.through.is_some() {
                            "yes"
                        } else {
                            "no"
                        })
                        .into(),
                    ),
                    (
                        "through",
                        (data
                            .query
                            .through
                            .map(|d| crate::i18n::date_iso(&d.to_string()))
                            .unwrap_or_default())
                        .into(),
                    ),
                    (
                        "clock",
                        (match data.query.clock {
                            Clock::Author => "author",
                            Clock::Committer => "committer",
                        })
                        .into(),
                    ),
                ],
            ));
            if data.history.more
                && view.limit < 100_000
                && ui
                    .button(crate::i18n::text(
                        "repo-workflows-atlas-atlas-window-a9656539",
                    ))
                    .clicked()
            {
                view.limit = (view.limit * 2).min(100_000);
                refresh = true;
            }
            let node = &data.nodes[view.zoom];
            ui.horizontal_wrapped(|ui| {
                if view.zoom != 0
                    && ui
                        .button(crate::i18n::text(
                            "repo-workflows-atlas-atlas-window-b4ab9b3d",
                        ))
                        .clicked()
                {
                    view.zoom = 0;
                    view.page = 0;
                }
                ui.label(if node.path.is_empty() {
                    "/".into()
                } else {
                    String::from_utf8_lossy(&node.path).into_owned()
                });
                ui.label(crate::i18n::message(
                    "repo-workflows-atlas-atlas-window-aef428d1",
                    &[
                        ("node-bytes-1", (node.bytes).into()),
                        ("node-changes-2", (node.changes).into()),
                    ],
                ));
                if !node.contributor.is_empty() {
                    ui.label(crate::i18n::message(
                        "repo-workflows-atlas-atlas-window-a5661a75",
                        &[("node-contributor-1", node.contributor.as_ref().into())],
                    ));
                }
            });
            let node = &data.nodes[view.zoom];
            if node.children.is_empty() {
                ui.label(crate::i18n::text(
                    "repo-workflows-atlas-atlas-window-5769ad13",
                ));
                return;
            }
            ui.small(crate::i18n::text(
                "repo-workflows-atlas-atlas-window-f69e93c6",
            ));
            // Bound painter and AccessKit work even for a flat tree with 100,000 files.
            let pages = node.children.len().div_ceil(300);
            view.page = view.page.min(pages.saturating_sub(1));
            if pages > 1 {
                ui.horizontal(|ui| {
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-atlas-atlas-window-6aba5092",
                        ))
                        .clicked()
                    {
                        view.page = view.page.saturating_sub(1);
                    }
                    ui.label(crate::i18n::message(
                        "repo-workflows-atlas-atlas-window-1316052d",
                        &[
                            ("view-page-1-1", (view.page + 1).into()),
                            ("pages", (pages).into()),
                        ],
                    ));
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-atlas-atlas-window-86061296",
                        ))
                        .clicked()
                    {
                        view.page = (view.page + 1).min(pages - 1);
                    }
                });
            }
            let ids: Vec<_> = node
                .children
                .iter()
                .skip(view.page * 300)
                .take(300)
                .copied()
                .collect();
            let weights: Vec<_> = ids
                .iter()
                .map(|id| {
                    let n = &data.nodes[*id];
                    (if view.size == Size::Bytes {
                        n.bytes as f64
                    } else {
                        n.changes as f64
                    })
                    .max(1.0)
                })
                .collect();
            let (rect, _) = ui.allocate_exact_size(
                vec2(
                    ui.available_width().max(50.0),
                    ui.available_height().clamp(160.0, 500.0),
                ),
                egui::Sense::hover(),
            );
            let mut boxes = Vec::new();
            partition(rect, &weights, 0, &mut boxes);
            let max = ids
                .iter()
                .map(|id| data.nodes[*id].changes)
                .max()
                .unwrap_or(1)
                .max(1) as f32;
            let t = crate::theme::Tokens::get(ctx);
            for (at, rect) in boxes {
                let id = ids[at];
                let n = &data.nodes[id];
                let rect = rect.shrink(1.5);
                let label = std::str::from_utf8(&n.path)
                    .map(std::borrow::Cow::Borrowed)
                    .unwrap_or_else(|_| std::borrow::Cow::Owned(n.path.escape_ascii().to_string()));
                let response =
                    ui.interact(rect, ui.id().with(("atlas-node", id)), egui::Sense::click());
                let fill = if view.colour == Colour::Churn {
                    t.accent
                        .gamma_multiply(0.15 + 0.85 * n.changes as f32 / max)
                } else {
                    t.lane(n.contributor_key).gamma_multiply(0.7)
                };
                ui.painter().rect_filled(rect, 3.0, fill);
                crate::a11y::a11y(
                    &response,
                    egui::WidgetType::Button,
                    crate::i18n::message(
                        "repo-workflows-atlas-atlas-window-2657941d",
                        &[
                            ("label", (label).clone().into()),
                            (
                                "if-n-directory-folder-else-file-1",
                                (if n.directory { "folder" } else { "file" }).into(),
                            ),
                            ("n-bytes-2", (n.bytes).into()),
                            ("n-changes-3", (n.changes).into()),
                            ("n-contributor-4", n.contributor.as_ref().into()),
                            (
                                "if-n-directory-zoom-into-folder--5",
                                (if n.directory {
                                    crate::i18n::text("repo-workflows-atlas-atlas-window-41bb5a28")
                                } else if std::str::from_utf8(&n.path).is_ok() {
                                    crate::i18n::text("repo-workflows-atlas-atlas-window-39c66d83")
                                } else {
                                    crate::i18n::text("repo-workflows-atlas-atlas-window-8d9ca065")
                                })
                                .into(),
                            ),
                        ],
                    ),
                );
                if rect.width() > 42.0 && rect.height() > 22.0 {
                    ui.painter().with_clip_rect(rect.shrink(3.0)).text(
                        rect.left_top() + vec2(4.0, 4.0),
                        egui::Align2::LEFT_TOP,
                        n.path
                            .rsplit(|b| *b == b'/')
                            .next()
                            .map(String::from_utf8_lossy)
                            .unwrap_or_default(),
                        egui::FontId::proportional(12.0),
                        t.text,
                    );
                }
                let mut scanned = false;
                response.context_menu(|ui| {
                    if !n.directory
                        && ui
                            .button(crate::i18n::text(
                                "repo-workflows-atlas-atlas-window-67be181b",
                            ))
                            .clicked()
                    {
                        scanned = true;
                        ui.close();
                    }
                });
                let clicked = response.clicked();
                response.on_hover_text(crate::i18n::message(
                    "repo-workflows-atlas-atlas-window-4b6c521c",
                    &[
                        ("label", (label).clone().into()),
                        ("n-bytes-1", (n.bytes).into()),
                        ("n-changes-2", (n.changes).into()),
                        (
                            "if-n-contributor-is-empty-no-cha-3",
                            (if n.contributor.is_empty() {
                                crate::i18n::text("repo-workflows-atlas-atlas-window-862161d0")
                            } else {
                                n.contributor.as_ref()
                            })
                            .into(),
                        ),
                    ],
                ));
                if scanned || (clicked && !n.directory && std::str::from_utf8(&n.path).is_err()) {
                    view.byte_history = Some(ByteHistory::new(
                        data.history.clone(),
                        n.path.clone(),
                        view.profile.clone(),
                        view.clock,
                    ));
                } else if clicked {
                    if !n.directory {
                        if let Ok(path) = String::from_utf8(n.path.clone()) {
                            file = n.revision.as_ref().map(|rev| (path, rev.to_string()));
                        }
                    } else {
                        view.zoom = id;
                        view.page = 0;
                    }
                }
            }
        });
        let mut chosen = None;
        if let Some(history) = &mut view.byte_history {
            let (history_open, commit) = history.show(&self.tx, ctx);
            chosen = commit;
            if !history_open {
                view.byte_history = None;
            }
        }
        if let Some(hash) = chosen {
            self.activity_commit(hash, ctx);
            self.atlas = None;
        } else if let Some((path, head)) = file {
            self.open_file_view(
                path,
                Some(head),
                super::super::file_view::FileTab::History,
                ctx,
            );
            self.atlas = None;
        } else if !open {
            self.atlas = None;
        } else if refresh {
            self.read_atlas();
        }
    }
}
