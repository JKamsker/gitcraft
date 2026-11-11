//! A folder browser for another machine (WSL, SSH): its folders listed by a script run there,
//! in the background, so a slow connection never holds the window up. Git repositories are
//! marked; a click picks a folder, a double click opens it.

use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Sense, Ui, vec2};

use crate::icons;
use crate::remote::{self, Host};
use crate::theme::Tokens;

/// The most folders listed of one folder (a few thousand rows are still quick to draw).
const MOST: usize = 5000;

/// A folder of the one listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    /// Holds a repository (`.git` in it).
    pub repo: bool,
}

/// What a folder holds, as listed there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The folder, as an absolute path there (`~` and links resolved by `cd`).
    pub dir: String,
    /// The folder itself holds a repository.
    pub repo: bool,
    pub folders: Vec<Folder>,
    /// More than [`MOST`] folders: the rest left out.
    pub cut: bool,
}

/// What came of the browser this frame.
pub enum Outcome {
    /// This folder (a path there) was picked.
    Picked(String),
    Cancelled,
}

pub struct FolderBrowser {
    host: Host,
    /// The folder shown, or being listed.
    listing: Option<Listing>,
    /// The folder asked for, while it is listed; what came of it.
    loading: Option<(String, Receiver<Result<Listing, String>>)>,
    /// Original requested directory and raw failure; neither is translated.
    error: Option<(String, String)>,
    /// The folder of the listing clicked, by its index.
    selected: Option<usize>,
    hidden: bool,
}

impl FolderBrowser {
    /// A browser of `host`'s folders, listing `start` (a path there; `~` when empty or not one).
    pub fn new(host: Host, start: &str, ctx: &egui::Context) -> Self {
        let start = start.trim();
        let start = if start.starts_with('/') || start == "~" || start.starts_with("~/") {
            start
        } else {
            "~"
        };
        let mut browser = Self {
            host,
            listing: None,
            loading: None,
            error: None,
            selected: None,
            hidden: false,
        };
        browser.go(start.to_owned(), ctx);
        browser
    }

    /// Lists `dir` in the background; the folder shown stays until it comes.
    fn go(&mut self, dir: String, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let host = self.host.clone();
        let ctx = ctx.clone();
        let asked = dir.clone();
        std::thread::spawn(move || {
            let _ = tx.send(list(&host, &asked));
            ctx.request_repaint();
        });
        self.loading = Some((dir, rx));
        self.error = None;
    }

    /// The listing asked for, once it came.
    fn take_listing(&mut self) {
        let Some((asked, rx)) = &self.loading else {
            return;
        };
        let Ok(result) = rx.try_recv() else { return };
        let asked = asked.clone();
        self.loading = None;
        match result {
            Ok(listing) => {
                self.listing = Some(listing);
                self.selected = None;
            }
            Err(e) => self.error = Some((asked, e)),
        }
    }

    /// The folder "Select" picks: the one clicked, else the one shown.
    fn choice(&self) -> Option<String> {
        let listing = self.listing.as_ref()?;
        Some(match self.selected.and_then(|i| listing.folders.get(i)) {
            Some(folder) => child(&listing.dir, &folder.name),
            None => listing.dir.clone(),
        })
    }

    /// Draws the browser: where it is, the folders there, Select and Cancel.
    pub fn ui(&mut self, ui: &mut Ui) -> Option<Outcome> {
        self.take_listing();
        let t = Tokens::get(ui.ctx());
        let mut go_to = None;
        let mut outcome = None;
        self.bar(ui, &t, &mut go_to);
        ui.add_space(6.0);
        if let Some((dir, e)) = &self.error {
            ui.colored_label(t.danger, display_list_error(e, dir, &self.host));
            ui.add_space(4.0);
        }
        let frame = egui::Frame::new()
            .fill(t.field)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_sm)
            .inner_margin(egui::Margin::same(4));
        frame.show(ui, |ui| {
            ui.set_min_height(280.0);
            ui.set_width(ui.available_width());
            self.rows(ui, &t, &mut go_to, &mut outcome);
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let choice = self.choice();
            let label = match (&choice, self.selected) {
                (Some(_), Some(_)) => crate::i18n::text("interaction-browser-select"),
                _ => crate::i18n::text("interaction-browser-select-current"),
            };
            if crate::repo::primary_button(ui, label, choice.is_some(), 140.0).clicked()
                && let Some(choice) = choice
            {
                outcome = Some(Outcome::Picked(choice));
            }
            if ui
                .add(
                    egui::Button::new(crate::i18n::text("interaction-browser-back"))
                        .min_size(vec2(80.0, 30.0)),
                )
                .clicked()
            {
                outcome = Some(Outcome::Cancelled);
            }
            ui.checkbox(
                &mut self.hidden,
                crate::i18n::text("interaction-browser-hidden"),
            );
        });
        if let Some(dir) = go_to {
            self.go(dir, ui.ctx());
        }
        outcome
    }

    /// Up, home, the folder's path (each part a link), reload.
    fn bar(&mut self, ui: &mut Ui, t: &Tokens, go_to: &mut Option<String>) {
        let dir = self.listing.as_ref().map(|l| l.dir.clone());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let up = dir.as_deref().filter(|d| *d != "/");
            if ui
                .add_enabled_ui(up.is_some(), |ui| {
                    icons::small_button(ui, "arrow-up", crate::i18n::text("interaction-browser-up"))
                })
                .inner
                .clicked()
                && let Some(d) = up
            {
                *go_to = Some(remote::parent_there(d).to_owned());
            }
            if icons::small_button(
                ui,
                "user-round",
                crate::i18n::text("interaction-browser-home"),
            )
            .clicked()
            {
                *go_to = Some("~".to_owned());
            }
            if icons::small_button(
                ui,
                "refresh-cw",
                crate::i18n::text("interaction-browser-reload"),
            )
            .clicked()
                && let Some(d) = &dir
            {
                *go_to = Some(d.clone());
            }
            ui.add_space(4.0);
            match &dir {
                Some(d) => {
                    for (n, (label, path)) in crumbs(d).into_iter().enumerate() {
                        // `/ home / me`: the root, then a slash between the others.
                        if n > 1 {
                            ui.label(RichText::new("/").color(t.text_faint));
                        }
                        if ui
                            .add(
                                egui::Label::new(RichText::new(label).color(t.text_dim))
                                    .sense(Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            *go_to = Some(path);
                        }
                    }
                }
                None => {
                    ui.label(RichText::new(self.host.label()).color(t.text_dim));
                }
            }
            if self.loading.is_some() {
                ui.add(crate::spinner::Spinner::new());
            }
        });
    }

    /// The folders, a repository's marked.
    fn rows(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        go_to: &mut Option<String>,
        outcome: &mut Option<Outcome>,
    ) {
        let Some(listing) = &self.listing else {
            if self.loading.is_some() {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "interaction-browser-connecting",
                        &[("host", self.host.label().into())],
                    ))
                    .color(t.text_faint),
                );
            }
            return;
        };
        if listing.repo {
            ui.horizontal(|ui| {
                icons::inline(ui, "folder-git-2", 14.0, t.accent_fg);
                ui.label(
                    RichText::new(crate::i18n::text("interaction-browser-is-repo"))
                        .color(t.accent_fg)
                        .small(),
                );
            });
        }
        let shown: Vec<(usize, &Folder)> = listing
            .folders
            .iter()
            .enumerate()
            .filter(|(_, f)| self.hidden || !f.name.starts_with('.'))
            .collect();
        if shown.is_empty() {
            ui.label(
                RichText::new(crate::i18n::text("interaction-browser-empty"))
                    .color(t.text_faint)
                    .small(),
            );
        }
        let row_h = 24.0;
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .auto_shrink([false, true])
            .show_rows(ui, row_h, shown.len(), |ui, range| {
                for &(i, folder) in &shown[range] {
                    let selected = self.selected == Some(i);
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                    crate::repo::widgets::a11y_selected(
                        &resp,
                        || {
                            crate::i18n::message(
                                if folder.repo {
                                    "interaction-browser-repo-label"
                                } else {
                                    "interaction-browser-folder-label"
                                },
                                &[("name", folder.name.as_str().into())],
                            )
                        },
                        selected,
                    );
                    if selected || resp.hovered() {
                        let fill = if selected { t.row_selected } else { t.hover };
                        ui.painter().rect_filled(rect, t.radius_sm, fill);
                    }
                    let (icon, tint) = if folder.repo {
                        ("folder-git-2", t.accent_fg)
                    } else {
                        ("folder-open", t.text_faint)
                    };
                    let icon_rect = egui::Rect::from_min_size(
                        rect.left_center() + vec2(6.0, -7.0),
                        vec2(14.0, 14.0),
                    );
                    icons::paint(ui, icon_rect, icon, 14.0, tint);
                    ui.painter().text(
                        rect.left_center() + vec2(26.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &folder.name,
                        egui::FontId::proportional(13.0),
                        t.text,
                    );
                    if folder.repo {
                        ui.painter().text(
                            rect.right_center() - vec2(8.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            crate::i18n::text("interaction-browser-repository"),
                            egui::FontId::proportional(11.0),
                            t.accent_fg,
                        );
                    }
                    if resp.double_clicked() {
                        *go_to = Some(child(&listing.dir, &folder.name));
                    } else if resp.clicked() {
                        self.selected = Some(i);
                    }
                }
            });
        if listing.cut {
            ui.label(
                RichText::new(crate::i18n::message(
                    "interaction-browser-limit",
                    &[("max", MOST.into())],
                ))
                .color(t.text_faint)
                .small(),
            );
        }
        // A repository double-clicked is picked, rather than opened as a folder.
        if let Some(dir) = go_to.as_ref()
            && let Some(f) = listing
                .folders
                .iter()
                .find(|f| f.repo && child(&listing.dir, &f.name) == *dir)
        {
            *outcome = Some(Outcome::Picked(child(&listing.dir, &f.name)));
            *go_to = None;
        }
    }
}

mod listing;
use listing::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod localization_tests;
