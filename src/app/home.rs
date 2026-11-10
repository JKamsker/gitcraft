//! The start page (open, create, clone, recent repositories) and the git-missing page.

mod cards;
use cards::*;

use std::path::{Path, PathBuf};

use egui::{Align2, Rect, RichText, Sense, Stroke, Ui, WidgetType, pos2, vec2};

use super::App;
use super::home_recent::RecentClick;
use crate::a11y::labels;
use crate::git;
use crate::icons;
use crate::repo::widgets;
use crate::theme::{self, Tokens};

/// The space kept between an action card's subtitle and its sides.
const CARD_TEXT_MARGIN: f32 = 12.0;

/// A retained Home error owns both its original English envelope and, only when supplied by
/// the actual source, the finite arguments needed to paint it in the current language.
/// Plain replacement always discards presentation metadata, even for identical raw text.
#[derive(Clone, Debug)]
pub(super) struct AppError {
    raw: String,
    presentation: Option<HomeError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HomeError {
    Init(String),
    Folder {
        folder: PathBuf,
        error: String,
        not_repo: bool,
    },
    Unsupported(String),
    PatchTarget {
        has_repo: bool,
        name: String,
    },
}

impl From<String> for AppError {
    fn from(raw: String) -> Self {
        Self {
            raw,
            presentation: None,
        }
    }
}

impl std::ops::Deref for AppError {
    type Target = str;

    fn deref(&self) -> &str {
        &self.raw
    }
}

impl AppError {
    fn sourced(source: HomeError) -> Self {
        Self {
            raw: source.raw(),
            presentation: Some(source),
        }
    }

    pub(super) fn init(error: String) -> Self {
        Self::sourced(HomeError::Init(error))
    }

    pub(super) fn folder(folder: &Path, error: &str, not_repo: bool) -> Self {
        Self::sourced(HomeError::Folder {
            folder: folder.to_owned(),
            error: error.to_owned(),
            not_repo,
        })
    }

    pub(super) fn unsupported(name: String) -> Self {
        Self::sourced(HomeError::Unsupported(name))
    }

    pub(super) fn patch_target(has_repo: bool, name: &str) -> Self {
        Self::sourced(HomeError::PatchTarget {
            has_repo,
            name: name.to_owned(),
        })
    }

    /// Historical toasts keep their emission language. Ordinary errors retain their original
    /// raw toast transport and its existing finite paint fallback.
    pub(super) fn delivery_text(&self) -> String {
        match &self.presentation {
            Some(source) if self.raw == source.raw() => source.display(),
            _ => self.raw.clone(),
        }
    }

    fn display(&self) -> String {
        match &self.presentation {
            Some(source) if self.raw == source.raw() => source.display(),
            Some(_) => self.raw.clone(),
            None => super::panels::display_app_message(&self.raw),
        }
    }
}

impl HomeError {
    fn raw(&self) -> String {
        match self {
            Self::Init(error) => format!("git init failed: {error}"),
            Self::Folder {
                folder,
                error,
                not_repo,
            } => {
                let name = super::dropped::name_of(folder);
                if *not_repo {
                    format!(
                        "{name} isn't in a git repository: drop a repository's folder to open it"
                    )
                } else {
                    format!("Could not open {name}: {error}")
                }
            }
            Self::Unsupported(name) => format!(
                "Can't open {name}: drop a folder to open it, or a .patch, .diff, .eml or .mbox file to apply it"
            ),
            Self::PatchTarget {
                has_repo: true,
                name,
            } => format!("To apply {name}, show its repository's tab, then drop the patch there"),
            Self::PatchTarget {
                has_repo: false,
                name,
            } => format!(
                "Applying {name} needs a repository open first: open one, then drop the patch on it"
            ),
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Init(error) => crate::i18n::message(
                "interaction-home-init-error",
                &[("error", error.as_str().into())],
            ),
            Self::Folder {
                folder,
                error,
                not_repo,
            } => {
                let name = super::dropped::name_of(folder);
                if *not_repo {
                    crate::i18n::message(
                        "interaction-drop-folder-not-repo",
                        &[("name", name.into())],
                    )
                } else {
                    crate::i18n::message(
                        "interaction-drop-folder-error",
                        &[
                            ("name", name.into()),
                            ("error", crate::git::display_error(error).into()),
                        ],
                    )
                }
            }
            Self::Unsupported(name) => crate::i18n::message(
                "interaction-drop-unsupported",
                &[("name", name.as_str().into())],
            ),
            Self::PatchTarget { has_repo, name } => crate::i18n::message(
                if *has_repo {
                    "interaction-drop-show-repo"
                } else {
                    "interaction-drop-open-repo"
                },
                &[("name", name.as_str().into())],
            ),
        }
    }
}

/// The actual Home error painter, also usable with supplied inert state in headless tests.
fn error_label(ui: &mut Ui, color: egui::Color32, error: &AppError) {
    ui.colored_label(color, error.display());
}

impl App {
    pub(super) fn pick_existing(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title(crate::i18n::text("interaction-home-pick-existing"))
            .pick_folder()
        {
            self.open_repo(&dir, ctx);
        }
    }

    pub(super) fn pick_new(&mut self, ctx: &egui::Context) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title(crate::i18n::text("interaction-home-pick-new"))
            .pick_folder()
        else {
            return;
        };
        // In WSL: all of it there, in the background (WSL may first have to start).
        if crate::remote::is_remote(&dir) {
            let label = crate::remote::display(&dir);
            self.connect_with(label, None, ctx, move || {
                if !git::toplevel(&dir).is_ok_and(|root| same_path(&root, &dir)) {
                    git::init(&dir).map_err(AppError::init)?;
                }
                super::repo_root(&dir).map_err(Into::into)
            });
            return;
        }
        // Already the root of a repository: just open it rather than re-initialising.
        if let Ok(root) = git::toplevel(&dir)
            && same_path(&root, &dir)
        {
            self.open_repo(&dir, ctx);
            return;
        }
        match git::init(&dir) {
            Ok(()) => self.open_repo(&dir, ctx),
            Err(e) => self.error = Some(AppError::init(e)),
        }
    }

    pub(super) fn home(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let mut open = None;
        let mut forget = None;
        let mut action = None;

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(640.0);
                    ui.add_space((ui.available_height() * 0.10).max(32.0));
                    self.home_header(ui, &t);
                    ui.add_space(26.0);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let w = (ui.available_width() - 36.0) / 4.0;
                        for (i, (icon, title, sub)) in [
                            (
                                "folder-open",
                                crate::i18n::text("interaction-home-open"),
                                crate::i18n::text("interaction-home-open-description"),
                            ),
                            (
                                "folder-plus",
                                crate::i18n::text("interaction-home-create"),
                                crate::i18n::text("interaction-home-create-description"),
                            ),
                            (
                                "log-in",
                                crate::i18n::text("interaction-home-clone"),
                                crate::i18n::text("interaction-home-clone-description"),
                            ),
                            (
                                "server",
                                crate::i18n::text("interaction-home-remote"),
                                crate::i18n::text("interaction-home-remote-description"),
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            if action_card(ui, w, icon, title, sub).clicked() {
                                action = Some(i);
                            }
                        }
                    });
                    ui.add_space(10.0);
                    if ui
                        .link(crate::i18n::text("interaction-home-scan"))
                        .clicked()
                    {
                        action = Some(4);
                    }

                    if ui
                        .button(crate::i18n::text("interaction-home-practice"))
                        .on_hover_text(crate::i18n::text("interaction-home-practice-tip"))
                        .clicked()
                    {
                        self.practice_open = true;
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .button(crate::i18n::text("interaction-home-export-setup"))
                            .clicked()
                        {
                            self.show_setup(false, ui.ctx());
                        }
                        if ui
                            .button(crate::i18n::text("interaction-home-import-setup"))
                            .clicked()
                        {
                            self.show_setup(true, ui.ctx());
                        }
                    });
                    self.home_link_field(ui, &t);

                    // The card has git's words itself.
                    if let Some(err) = self.error.as_ref().filter(|_| self.error_card.is_none()) {
                        ui.add_space(12.0);
                        error_label(ui, t.danger, err);
                    }
                    self.error_card_ui(ui);
                    if let Some(cloning) = &self.cloning {
                        ui.add_space(12.0);
                        super::clone::cloning_line(ui, cloning);
                    }
                    for connecting in &self.connecting {
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.add(crate::spinner::Spinner::new());
                            ui.label(crate::i18n::message(
                                "interaction-home-connecting",
                                &[("place", connecting.label.as_str().into())],
                            ));
                        });
                    }

                    ui.add_space(30.0);
                    self.workspaces_section(ui);
                    ui.add_space(24.0);
                    self.inbox_section(ui);

                    if !self.recent.is_empty() {
                        ui.add_space(24.0);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            match self.recent_section(ui, &t) {
                                Some(RecentClick::Open(path)) => open = Some(path),
                                Some(RecentClick::Forget(i)) => forget = Some(i),
                                None => {}
                            }
                        });
                    }
                });
            });

        match action {
            Some(0) => self.pick_existing(&ctx),
            Some(1) => self.pick_new(&ctx),
            Some(2) => self.start_clone(),
            Some(3) => self.start_open_remote(&ctx),
            Some(_) => self.start_repo_scan(&ctx),
            None => {}
        }
        if let Some(path) = open {
            self.open_repo(&path, &ctx);
        }
        if let Some(i) = forget {
            self.recent.remove(i);
        }
    }

    /// The name, the git version found, and the offer to add an AppImage to the menu.
    fn home_header(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.horizontal(|ui| {
            icons::inline(ui, "git-merge", 34.0, t.accent);
            ui.label(RichText::new("gitcraft").font(theme::semibold(30.0)));
        });
        ui.add_space(4.0);
        if let Ok(v) = &self.git_version {
            ui.horizontal(|ui| {
                icons::inline(ui, "check", 14.0, t.success);
                ui.label(RichText::new(v).color(t.text_dim));
            });
        }
        if let Some(appimage) = crate::desktop_entry::appimage()
            && !crate::portable::asked()
            && !crate::desktop_entry::installed()
        {
            ui.add_space(4.0);
            let link = ui
                .link(crate::i18n::text("interaction-home-app-menu"))
                .on_hover_text(appimage.display().to_string());
            if link.clicked() {
                self.error = crate::desktop_entry::install(&appimage)
                    .err()
                    .map(Into::into);
            }
        }
    }

    pub(super) fn git_missing(&mut self, ui: &mut Ui, err: &str) {
        let t = Tokens::get(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.25);
            icons::inline(ui, "circle-alert", 40.0, t.warning);
            ui.add_space(8.0);
            ui.label(
                RichText::new(crate::i18n::text("interaction-home-git-missing"))
                    .font(theme::semibold(24.0)),
            );
            ui.add_space(8.0);
            ui.label(crate::i18n::text("interaction-home-git-install"));
            ui.add_space(4.0);
            ui.label(RichText::new(err).color(t.text_faint).small());
            ui.add_space(16.0);
            if crate::repo::primary_button(
                ui,
                crate::i18n::text("interaction-home-git-recheck"),
                true,
                140.0,
            )
            .clicked()
            {
                self.git_version = git::version();
            }
            ui.add_space(8.0);
            ui.hyperlink_to(
                crate::i18n::text("interaction-home-git-download"),
                "https://git-scm.com/downloads",
            );
        });
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    crate::remote::same_dir(a, b)
}

impl App {
    /// The card explaining why a repository couldn't be opened, when gitcraft knows why; its
    /// Try again opens the folder again.
    fn error_card_ui(&mut self, ui: &mut Ui) {
        let Some(card) = &mut self.error_card else {
            return;
        };
        ui.add_space(10.0);
        let event = card.ui(ui, true);
        let dir = card.dir().to_owned();
        match event {
            Some(crate::error_card::CardEvent::Close) => self.error_card = None,
            Some(crate::error_card::CardEvent::Retry) => {
                let ctx = ui.ctx().clone();
                self.open_repo(&dir, &ctx);
            }
            None => {}
        }
    }
}

#[cfg(test)]
#[path = "home/retained_error_tests.rs"]
mod retained_error_tests;
#[cfg(test)]
#[path = "home/tests.rs"]
mod tests;
