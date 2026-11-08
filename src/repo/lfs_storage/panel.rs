//! The LFS storage panel: the disk the local LFS objects take, the files of the checkout that
//! are only pointers (and downloading them), which folders LFS fetches, and pruning.

use egui::{RichText, Ui};

use super::{Panel, StorageAct, StorageState, download_action, prune_action};
use crate::git::{FetchFilters, folder_pattern, human_size, pattern_label};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How many files that are only pointers the panel lists by name.
const LISTED: usize = 200;

impl RepoTab {
    pub(super) fn storage_panel(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(mut panel) = self.lfs_storage.panel.take() else {
            return;
        };
        let state = &self.lfs_storage;
        let idle = self.busy_op.is_none();
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut close = false;
        let resp = egui::Modal::new(egui::Id::new("lfs-storage"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                close = header(ui, &t, acts);
                if let Some(busy) = &self.busy_op {
                    ui.label(
                        RichText::new(format!(
                            "{}…",
                            crate::repo::messages::display_operation(busy)
                        ))
                        .color(t.text_dim),
                    );
                }
                let max = (ctx.content_rect().height() - 160.0).max(240.0);
                egui::ScrollArea::vertical().max_height(max).show(ui, |ui| {
                    usage(ui, &t, &panel);
                    section(
                        ui,
                        crate::i18n::text("repo-workflows-lfs_storage-storage-panel-a99e2b3d"),
                    );
                    checkout(ui, &t, state, idle, acts);
                    section(
                        ui,
                        crate::i18n::text("repo-workflows-lfs_storage-storage-panel-b05f298a"),
                    );
                    filters(ui, &t, &mut panel, acts);
                    section(
                        ui,
                        crate::i18n::text("repo-workflows-lfs_storage-storage-panel-6ce01b6d"),
                    );
                    prune(ui, &t, &mut panel, idle, acts);
                });
            });
        if close || resp.should_close() {
            acts.push(Action::LfsStorage(StorageAct::ClosePanel));
        }
        self.lfs_storage.panel = Some(panel);
    }
}

fn header(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Action>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        icons::inline(ui, "database", 16.0, t.icon);
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-lfs_storage-header-59ef044f",
            ))
            .font(theme::semibold(16.0)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(
                ui,
                "x",
                crate::i18n::text("repo-workflows-lfs_storage-header-961b2a04"),
            )
            .clicked()
            {
                close = true;
            }
            if icons::small_button(
                ui,
                "refresh-cw",
                crate::i18n::text("repo-workflows-lfs_storage-header-d8cb92fa"),
            )
            .clicked()
            {
                acts.push(Action::LfsStorage(StorageAct::Refresh));
            }
        });
    });
    close
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).font(theme::semibold(13.0)));
    ui.add_space(2.0);
}

fn spinner_line(ui: &mut Ui, t: &Tokens, text: &str) {
    ui.horizontal(|ui| {
        ui.add(crate::spinner::Spinner::new());
        ui.label(RichText::new(text).color(t.text_dim));
    });
}

fn error(ui: &mut Ui, t: &Tokens, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(t.danger)).wrap());
}

fn usage(ui: &mut Ui, t: &Tokens, panel: &Panel) {
    ui.add_space(8.0);
    match &panel.usage {
        None => spinner_line(
            ui,
            t,
            crate::i18n::text("repo-workflows-lfs_storage-usage-aca1a392"),
        ),
        Some(Err(e)) if e.is_empty() => spinner_line(
            ui,
            t,
            crate::i18n::text("repo-workflows-lfs_storage-usage-aca1a392"),
        ),
        Some(Err(e)) => error(
            ui,
            t,
            &crate::i18n::message(
                "repo-workflows-lfs_storage-usage-bc295532",
                &[("e", (e).clone().into())],
            ),
        ),
        Some(Ok(u)) => {
            let text = match u.objects {
                0 => crate::i18n::text("repo-workflows-lfs_storage-usage-18b3c0d1").to_owned(),
                1 => crate::i18n::message(
                    "repo-workflows-lfs_storage-usage-9aea26d2",
                    &[("human-size-u-bytes-1", (human_size(u.bytes)).clone().into())],
                ),
                n => crate::i18n::message(
                    "repo-workflows-lfs_storage-usage-a5b8a0cd",
                    &[
                        ("human-size-u-bytes-1", (human_size(u.bytes)).clone().into()),
                        ("n", (n).into()),
                    ],
                ),
            };
            ui.label(RichText::new(text).font(theme::semibold(15.0)));
            ui.add(egui::Label::new(RichText::new(&u.dir).color(t.text_faint).small()).truncate())
                .on_hover_text(&u.dir);
        }
    }
}

/// The files of the checkout in LFS, those that are only pointers, and Download.
fn checkout(ui: &mut Ui, t: &Tokens, state: &StorageState, idle: bool, acts: &mut Vec<Action>) {
    if let Some(e) = &state.files_error {
        error(ui, t, e);
        return;
    }
    if !state.files_read {
        spinner_line(
            ui,
            t,
            crate::i18n::text("repo-workflows-lfs_storage-checkout-f4b00836"),
        );
        return;
    }
    let missing: Vec<_> = state.missing().collect();
    let files = state.files.len();
    let summary = checkout_summary(files, missing.len(), state.missing_bytes);
    ui.label(RichText::new(summary).color(t.text_dim));
    if missing.is_empty() {
        return;
    }
    let paths: Vec<String> = missing.iter().map(|f| f.path.clone()).collect();
    match download_action(&paths) {
        Ok(all) => {
            let r = ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::message(
                        "repo-workflows-lfs_storage-checkout-5f0a6ba0",
                        &[("paths-len-1", (paths.len()).into())],
                    )),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-lfs_storage-checkout-4cd9a422",
                ));
            if r.clicked() {
                acts.push(all);
            }
        }
        Err(e) => {
            let r = ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-lfs_storage-checkout-d6eafe82",
                    )),
                )
                .on_hover_text(crate::i18n::message(
                    "repo-workflows-lfs_storage-checkout-87bcf309",
                    &[("e", (e).clone().into())],
                ));
            if r.clicked()
                && let Ok(pull) = download_action(&[])
            {
                acts.push(pull);
            }
        }
    }
    egui::CollapsingHeader::new(crate::i18n::message(
        "repo-workflows-lfs_storage-checkout-1716d186",
        &[("missing-len-1", (missing.len()).into())],
    ))
    .id_salt("lfs-missing")
    .show(ui, |ui| {
        for f in missing.iter().take(LISTED) {
            ui.horizontal(|ui| {
                icons::inline(ui, "cloud-download", 12.0, t.text_dim);
                let name = RichText::new(&f.path).font(theme::medium(12.0));
                ui.add(egui::Label::new(name).truncate());
                ui.label(
                    RichText::new(human_size(f.size))
                        .color(t.text_faint)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match download_action(std::slice::from_ref(&f.path)) {
                        Ok(one) => {
                            if ui
                                .add_enabled(
                                    idle,
                                    egui::Button::new(crate::i18n::text(
                                        "repo-workflows-lfs_storage-checkout-d6eafe82",
                                    )),
                                )
                                .clicked()
                            {
                                acts.push(one);
                            }
                        }
                        Err(e) => {
                            ui.add_enabled(
                                false,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-lfs_storage-checkout-d6eafe82",
                                )),
                            )
                            .on_disabled_hover_text(e);
                        }
                    }
                });
            });
        }
        if missing.len() > LISTED {
            let more = crate::i18n::message(
                "repo-workflows-lfs_storage-checkout-683768ab",
                &[("missing-len-listed-1", (missing.len() - LISTED).into())],
            );
            ui.label(RichText::new(more).color(t.text_faint).small());
        }
    });
}

/// This repository's `lfs.fetchinclude` and `lfs.fetchexclude` as lists of folders, each taken
/// out with its ×, added to with a field; what its `.lfsconfig` sets, shown.
fn filters(ui: &mut Ui, t: &Tokens, panel: &mut Panel, acts: &mut Vec<Action>) {
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-lfs_storage-filters-9777b8b2",
        ))
        .color(t.text_dim)
        .small(),
    );
    let filters: FetchFilters = match &panel.filters {
        None => {
            ui.add(crate::spinner::Spinner::new());
            return;
        }
        Some(Err(e)) => {
            error(
                ui,
                t,
                &crate::i18n::message(
                    "repo-workflows-lfs_storage-filters-72d4ac36",
                    &[("e", (e).clone().into())],
                ),
            );
            return;
        }
        Some(Ok(f)) => f.clone(),
    };
    let lists = [
        (
            false,
            crate::i18n::text("repo-workflows-lfs_storage-filters-8758c8bb"),
            filters.include,
            filters.shared_include,
        ),
        (
            true,
            crate::i18n::text("repo-workflows-lfs_storage-filters-cd8bf912"),
            filters.exclude,
            filters.shared_exclude,
        ),
    ];
    for (exclude, title, list, shared) in lists {
        ui.add_space(4.0);
        ui.label(RichText::new(title).font(theme::medium(12.5)));
        ui.horizontal_wrapped(|ui| {
            if list.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-lfs_storage-filters-140bedbf",
                    ))
                    .color(t.text_faint),
                );
            }
            for (i, pattern) in list.iter().enumerate() {
                let name = pattern_label(pattern);
                crate::repo::widgets::chip(ui, &name, t.text_dim);
                if icons::small_button(
                    ui,
                    "x",
                    &crate::i18n::message(
                        "repo-workflows-lfs_storage-filters-dc013b99",
                        &[("name", (name).clone().into())],
                    ),
                )
                .clicked()
                {
                    let mut rest = list.clone();
                    rest.remove(i);
                    acts.push(Action::LfsStorage(StorageAct::SetFilter {
                        exclude,
                        list: rest,
                    }));
                }
            }
        });
        if !shared.is_empty() {
            let names: Vec<String> = shared.iter().map(|p| pattern_label(p)).collect();
            let note = if list.is_empty() {
                crate::i18n::message(
                    "repo-workflows-lfs_storage-filters-c621d797",
                    &[("names-join-1", (names.join(", ")).clone().into())],
                )
            } else {
                crate::i18n::message(
                    "repo-workflows-lfs_storage-filters-49e74a0b",
                    &[("names-join-1", (names.join(", ")).clone().into())],
                )
            };
            ui.label(RichText::new(note).color(t.text_faint).small());
        }
        let typed = if exclude {
            &mut panel.new_exclude
        } else {
            &mut panel.new_include
        };
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(typed)
                    .hint_text("art/characters")
                    .desired_width(240.0),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let pattern = folder_pattern(typed);
            let add = ui.add_enabled(
                pattern.is_some(),
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-lfs_storage-filters-9fd728c6",
                )),
            );
            if let Some(pattern) = pattern
                && (add.clicked() || enter)
            {
                let mut list = list.clone();
                if !list.contains(&pattern) {
                    list.push(pattern);
                }
                acts.push(Action::LfsStorage(StorageAct::SetFilter { exclude, list }));
            }
        });
    }
    if let Some(e) = &panel.filter_error {
        error(ui, t, e);
    }
}

/// What pruning would free, then Prune, asked twice.
fn prune(ui: &mut Ui, t: &Tokens, panel: &mut Panel, idle: bool, acts: &mut Vec<Action>) {
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-lfs_storage-prune-8bfaae7c",
        ))
        .color(t.text_dim)
        .small(),
    );
    if panel.checking_plan {
        spinner_line(
            ui,
            t,
            crate::i18n::text("repo-workflows-lfs_storage-prune-f0bcc399"),
        );
        return;
    }
    ui.horizontal(|ui| {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-lfs_storage-prune-ebe86463",
            ))
            .clicked()
        {
            acts.push(Action::LfsStorage(StorageAct::CheckPrune));
        }
        match &panel.plan {
            Some(Ok(plan)) if plan.objects == 0 => {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-lfs_storage-prune-659493e2",
                    ))
                    .color(t.text_dim),
                );
            }
            Some(Ok(plan)) => {
                let objects = if plan.objects == 1 {
                    crate::i18n::text("repo-workflows-lfs_storage-prune-30967fda").to_owned()
                } else {
                    crate::i18n::message(
                        "repo-workflows-lfs_storage-prune-8f8978b1",
                        &[("plan-objects-1", (plan.objects).into())],
                    )
                };
                let label = if panel.confirm_prune {
                    crate::i18n::message(
                        "repo-workflows-lfs_storage-prune-407859ca",
                        &[("objects", (objects).clone().into())],
                    )
                } else {
                    crate::i18n::message(
                        "repo-workflows-lfs_storage-prune-5fc3fdf9",
                        &[
                            ("objects", (objects).clone().into()),
                            (
                                "human-size-plan-bytes-1",
                                (human_size(plan.bytes)).clone().into(),
                            ),
                        ],
                    )
                };
                let button = egui::Button::new(RichText::new(label).color(t.danger));
                if ui.add_enabled(idle, button).clicked() {
                    if panel.confirm_prune {
                        panel.confirm_prune = false;
                        acts.push(prune_action());
                    } else {
                        panel.confirm_prune = true;
                    }
                }
                if panel.confirm_prune
                    && ui
                        .small_button(crate::i18n::text(
                            "repo-workflows-lfs_storage-prune-19766ed6",
                        ))
                        .clicked()
                {
                    panel.confirm_prune = false;
                }
            }
            Some(Err(e)) => error(ui, t, e),
            None => {}
        }
    });
}

fn checkout_summary(files: usize, missing: usize, bytes: u64) -> String {
    crate::i18n::message(
        "repo-workflows-lfs-checkout-summary",
        &[
            ("files", (files).into()),
            ("missing", (missing).into()),
            ("bytes", (human_size(bytes)).into()),
        ],
    )
}
