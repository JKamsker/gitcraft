//! The LFS storage panel: the disk the local LFS objects take, the files of the checkout that
//! are only pointers (and downloading them), which folders LFS fetches, and pruning.

use egui::{RichText, Ui};

use super::{Panel, StorageAct, StorageState, download_action, prune_action};
use crate::git::{folder_pattern, human_size};
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
                    ui.label(RichText::new(format!("{busy}…")).color(t.text_dim));
                }
                let max = (ctx.content_rect().height() - 160.0).max(240.0);
                egui::ScrollArea::vertical().max_height(max).show(ui, |ui| {
                    usage(ui, &t, &panel);
                    section(ui, "This checkout");
                    checkout(ui, &t, state, idle, acts);
                    section(ui, "What LFS fetches");
                    filters(ui, &t, &mut panel, acts);
                    section(ui, "Old objects");
                    prune(ui, &t, &panel, idle, acts);
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
        ui.label(RichText::new("LFS storage").font(theme::semibold(16.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                close = true;
            }
            if icons::small_button(ui, "refresh-cw", "Measure again").clicked() {
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

fn usage(ui: &mut Ui, t: &Tokens, panel: &Panel) {
    ui.add_space(8.0);
    match &panel.usage {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(RichText::new("Measuring the LFS objects…").color(t.text_dim));
            });
        }
        Some(Err(e)) => {
            ui.add(
                egui::Label::new(
                    RichText::new(format!("Could not measure the LFS objects: {e}"))
                        .color(t.danger),
                )
                .wrap(),
            );
        }
        Some(Ok(u)) => {
            let objects = if u.objects == 1 { "object" } else { "objects" };
            ui.label(
                RichText::new(format!(
                    "{} in {} LFS {objects} on disk",
                    human_size(u.bytes),
                    u.objects
                ))
                .font(theme::semibold(15.0)),
            );
            ui.add(egui::Label::new(RichText::new(&u.dir).color(t.text_faint).small()).truncate())
                .on_hover_text(&u.dir);
        }
    }
}

/// The files of the checkout in LFS, those that are only pointers, and Download.
fn checkout(ui: &mut Ui, t: &Tokens, state: &StorageState, idle: bool, acts: &mut Vec<Action>) {
    if let Some(e) = &state.files_error {
        ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
        return;
    }
    let missing: Vec<_> = state.missing().collect();
    let total: u64 = missing.iter().map(|f| f.size).sum();
    let files = state.files.len();
    let summary = match (files, missing.len()) {
        (0, _) => "No file of the checkout is kept in LFS.".to_owned(),
        (n, 0) => format!("{n} {} in LFS, all downloaded.", plural(n, "file", "files")),
        (n, m) => format!(
            "{n} {} in LFS; {m} only {} ({} never downloaded).",
            plural(n, "file", "files"),
            plural(m, "a pointer", "pointers"),
            human_size(total)
        ),
    };
    ui.label(RichText::new(summary).color(t.text_dim));
    if missing.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        if let Ok(all) = download_action(&[]) {
            let r = ui
                .add_enabled(idle, egui::Button::new("Download"))
                .on_hover_text("git lfs pull: every file, as far as the folders below allow");
            if r.clicked() {
                acts.push(all);
            }
        }
    });
    egui::CollapsingHeader::new(format!("Only pointers ({})", missing.len()))
        .id_salt("lfs-missing")
        .show(ui, |ui| {
            for f in missing.iter().take(LISTED) {
                ui.horizontal(|ui| {
                    icons::inline(ui, "cloud-download", 12.0, t.text_dim);
                    ui.add(
                        egui::Label::new(RichText::new(&f.path).font(theme::medium(12.0)))
                            .truncate(),
                    );
                    ui.label(
                        RichText::new(human_size(f.size))
                            .color(t.text_faint)
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match download_action(std::slice::from_ref(&f.path)) {
                            Ok(one) => {
                                if ui
                                    .add_enabled(idle, egui::Button::new("Download"))
                                    .clicked()
                                {
                                    acts.push(one);
                                }
                            }
                            Err(e) => {
                                ui.add_enabled(false, egui::Button::new("Download"))
                                    .on_disabled_hover_text(e);
                            }
                        }
                    });
                });
            }
            if missing.len() > LISTED {
                ui.label(
                    RichText::new(format!("and {} more", missing.len() - LISTED))
                        .color(t.text_faint)
                        .small(),
                );
            }
        });
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// `lfs.fetchinclude` and `lfs.fetchexclude` as lists of folders, each taken out with its ×,
/// added to with a field.
fn filters(ui: &mut Ui, t: &Tokens, panel: &mut Panel, acts: &mut Vec<Action>) {
    ui.label(
        RichText::new(
            "LFS: Fetch and Pull download only the folders included (every one when none is), \
             leaving out those excluded. Set for this repository only.",
        )
        .color(t.text_dim)
        .small(),
    );
    let Some((include, exclude)) = panel.filters.clone() else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    for (exclude_list, title, list, typed) in [
        (false, "Only these folders", include, &mut panel.new_include),
        (true, "Never these folders", exclude, &mut panel.new_exclude),
    ] {
        ui.add_space(4.0);
        ui.label(RichText::new(title).font(theme::medium(12.5)));
        ui.horizontal_wrapped(|ui| {
            if list.is_empty() {
                ui.label(RichText::new("none").color(t.text_faint));
            }
            for (i, folder) in list.iter().enumerate() {
                crate::repo::widgets::chip(ui, folder, t.text_dim);
                if icons::small_button(ui, "x", &format!("Take {folder} out")).clicked() {
                    let mut rest = list.clone();
                    rest.remove(i);
                    acts.push(Action::LfsStorage(StorageAct::SetFilter {
                        exclude: exclude_list,
                        list: rest,
                    }));
                }
            }
        });
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(typed)
                    .hint_text("art/characters")
                    .desired_width(240.0),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let folder = folder_pattern(typed);
            let add = ui.add_enabled(folder.is_some(), egui::Button::new("Add"));
            if let Some(folder) = folder
                && (add.clicked() || enter)
            {
                let mut list = list.clone();
                if !list.contains(&folder) {
                    list.push(folder);
                }
                typed.clear();
                acts.push(Action::LfsStorage(StorageAct::SetFilter {
                    exclude: exclude_list,
                    list,
                }));
            }
        });
    }
    if let Some(e) = &panel.filter_error {
        ui.colored_label(t.danger, e);
    }
}

/// What pruning would free, then Prune.
fn prune(ui: &mut Ui, t: &Tokens, panel: &Panel, idle: bool, acts: &mut Vec<Action>) {
    ui.label(
        RichText::new(
            "Pruning deletes local LFS objects that no recent commit, the checkout or unpushed \
             work needs; they can be downloaded again.",
        )
        .color(t.text_dim)
        .small(),
    );
    ui.horizontal(|ui| {
        if panel.checking_plan {
            ui.add(crate::spinner::Spinner::new());
            ui.label(RichText::new("Working out what pruning frees…").color(t.text_dim));
            return;
        }
        if ui.button("Check what pruning frees").clicked() {
            acts.push(Action::LfsStorage(StorageAct::CheckPrune));
        }
        match &panel.plan {
            Some(Ok(plan)) if plan.objects == 0 => {
                ui.label(RichText::new("Nothing to prune.").color(t.text_dim));
            }
            Some(Ok(plan)) => {
                let text = format!(
                    "Frees {} in {} {}.",
                    human_size(plan.bytes),
                    plan.objects,
                    if plan.objects == 1 {
                        "object"
                    } else {
                        "objects"
                    }
                );
                ui.label(RichText::new(text).color(t.text));
                if ui.add_enabled(idle, egui::Button::new("Prune")).clicked() {
                    acts.push(prune_action());
                }
            }
            Some(Err(e)) => {
                ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
            }
            None => {}
        }
    });
}
