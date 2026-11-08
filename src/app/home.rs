//! The start page (open, create, clone, recent repositories) and the git-missing page.

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

/// A large clickable card on the start page.
fn action_card(ui: &mut Ui, width: f32, icon: &str, title: &str, subtitle: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 122.0), Sense::click());
    widgets::a11y(&resp, WidgetType::Button, || {
        labels::join([title, subtitle])
    });
    let fill = if resp.hovered() { t.hover } else { t.card };
    let stroke = if resp.hovered() || resp.has_focus() {
        t.accent
    } else {
        t.field_border
    };
    let p = ui.painter();
    p.rect(
        rect,
        t.radius_lg,
        fill,
        Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.center().x, rect.top() + 34.0), vec2(28.0, 28.0)),
        icon,
        26.0,
        t.accent,
    );
    p.text(
        pos2(rect.center().x, rect.top() + 68.0),
        Align2::CENTER_CENTER,
        title,
        theme::semibold(14.0),
        t.text,
    );
    // Wrapped (two lines at most, then cut with "…") inside the card, clear of its sides.
    let galley = p.layout_job(subtitle_job(subtitle, width, t.text_dim));
    let top = rect.top() + 82.0;
    let pos = pos2(
        rect.center().x - galley.rect.center().x,
        top - galley.rect.top(),
    );
    p.galley(pos, galley, t.text_dim);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An action card's subtitle, laid out to fit a card `card_width` wide.
fn subtitle_job(subtitle: &str, card_width: f32, color: egui::Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(
        subtitle.to_owned(),
        egui::TextFormat::simple(egui::FontId::proportional(11.5), color),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: subtitle_width(card_width),
        max_rows: 2,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job.halign = egui::Align::Center;
    job
}

/// How wide an action card's subtitle may be.
fn subtitle_width(card_width: f32) -> f32 {
    (card_width - 2.0 * CARD_TEXT_MARGIN).max(1.0)
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
mod tests {
    use super::*;

    /// A card's subtitle keeps clear of the card's sides: wrapped, then cut, never wider.
    #[test]
    fn card_subtitles_stay_inside_their_card() {
        let ctx = egui::Context::default();
        let cases = [
            (151.0, "An existing local repository"),
            (
                90.0,
                "A new repository (git init) with a much longer subtitle",
            ),
            (40.0, "Größe 日本語 repository"),
            (151.0, ""),
        ];
        let mut laid_out = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for (width, text) in cases {
                let job = subtitle_job(text, width, egui::Color32::WHITE);
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                laid_out.push((width, text, galley.rect.width(), galley.rows.len()));
            }
        });
        // Nothing renders here: the font texture is left unapplied on purpose.
        output.textures_delta.clear();
        assert_eq!(laid_out.len(), cases.len());
        for (width, text, wide, rows) in laid_out {
            assert!(
                wide <= subtitle_width(width) + 0.5,
                "{text:?} at {width}: {wide}"
            );
            assert!(rows <= 2, "{text:?}: {rows} rows");
        }
    }
}

#[cfg(test)]
mod retained_error_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn retained_home_errors_paint_en_de_en_from_original_source_without_io() {
        crate::i18n::set_language("en-US");
        let path = Path::new("/opaque/日本語 isn't in a git repository");
        let detail = "native 日本語\n: $(opaque) git init failed: literal";
        let errors = vec![
            AppError::init(detail.into()),
            super::super::dropped::retained_folder_error(path, "FATAL: NOT A GIT REPOSITORY"),
            super::super::dropped::retained_folder_error(path, detail),
            AppError::unsupported("日本語.patch.txt".into()),
            AppError::patch_target(false, "日本語.patch"),
            AppError::patch_target(true, "literal.patch"),
        ];
        let raw: Vec<_> = errors.iter().map(|e| e.raw.clone()).collect();
        let sources: Vec<_> = errors.iter().map(|e| e.presentation.clone()).collect();
        assert_eq!(sources[0], Some(HomeError::Init(detail.into())));
        assert_eq!(
            sources[1],
            Some(HomeError::Folder {
                folder: path.into(),
                error: "FATAL: NOT A GIT REPOSITORY".into(),
                not_repo: true,
            })
        );
        assert_eq!(
            sources[2],
            Some(HomeError::Folder {
                folder: path.into(),
                error: detail.into(),
                not_repo: false,
            })
        );
        assert_eq!(
            sources[3],
            Some(HomeError::Unsupported("日本語.patch.txt".into()))
        );
        assert_eq!(
            sources[4],
            Some(HomeError::PatchTarget {
                has_repo: false,
                name: "日本語.patch".into()
            })
        );
        assert_eq!(
            sources[5],
            Some(HomeError::PatchTarget {
                has_repo: true,
                name: "literal.patch".into()
            })
        );
        assert_eq!(raw, [
            format!("git init failed: {detail}"),
            "日本語 isn't in a git repository isn't in a git repository: drop a repository's folder to open it".into(),
            format!("Could not open 日本語 isn't in a git repository: {detail}"),
            "Can't open 日本語.patch.txt: drop a folder to open it, or a .patch, .diff, .eml or .mbox file to apply it".into(),
            "Applying 日本語.patch needs a repository open first: open one, then drop the patch on it".into(),
            "To apply literal.patch, show its repository's tab, then drop the patch there".into(),
        ]);
        // Only supplied error values reach the exact label function used by App::home.
        let mut h = Harness::new_ui_state(
            |ui, errors: &mut Vec<AppError>| {
                for error in errors {
                    error_label(ui, egui::Color32::RED, error);
                }
            },
            errors,
        );
        h.run();
        for expected in &raw {
            h.get_by_label(expected);
        }
        crate::i18n::set_language("de");
        h.step();
        for expected in [
            format!("git init fehlgeschlagen: {detail}"),
            "日本語 isn't in a git repository liegt nicht in einem Git-Repository: Ziehe einen Repository-Ordner hierher, um ihn zu öffnen".into(),
            format!("日本語 isn't in a git repository konnte nicht geöffnet werden: {detail}"),
            "日本語.patch.txt kann nicht geöffnet werden: Ziehe einen Ordner hierher, um ihn zu öffnen, oder eine .patch-, .diff-, .eml- oder .mbox-Datei, um sie anzuwenden".into(),
            "Um 日本語.patch anzuwenden, muss zuerst ein Repository geöffnet sein: Öffne eines und lege den Patch dort ab".into(),
            "Zeige den Tab des zugehörigen Repositorys und lege literal.patch dort ab, um den Patch anzuwenden".into(),
        ] { h.get_by_label(&expected); }
        let emitted = h.state()[0].delivery_text();
        assert_eq!(emitted, format!("git init fehlgeschlagen: {detail}"));
        for (i, error) in h.state().iter().enumerate() {
            assert_eq!(error.raw, raw[i]);
            assert_eq!(error.presentation, sources[i]);
        }
        crate::i18n::set_language("en-US");
        h.step();
        for expected in &raw {
            h.get_by_label(expected);
        }
        assert_eq!(emitted, format!("git init fehlgeschlagen: {detail}"));
        for (i, error) in h.state().iter().enumerate() {
            assert_eq!(error.raw, raw[i]);
            assert_eq!(error.presentation, sources[i]);
        }
    }

    #[test]
    fn plain_replacement_clearing_and_mismatch_never_recover_source_from_text() -> Result<(), String>
    {
        crate::i18n::set_language("de");
        let original = AppError::init("fixture native error".into());
        let raw = original.raw.clone();
        let mut retained = Some(original);
        assert_eq!(
            retained.as_ref().ok_or("missing test value")?.display(),
            "git init fehlgeschlagen: fixture native error"
        );
        // Identical raw text from an unrelated write has no provenance.
        retained = Some(raw.clone().into());
        assert!(
            retained
                .as_ref()
                .ok_or("missing test value")?
                .presentation
                .is_none()
        );
        assert_eq!(
            retained.as_ref().ok_or("missing test value")?.display(),
            raw
        );
        retained = None;
        assert!(retained.is_none());
        retained = Some(raw.clone().into());
        assert_eq!(retained.as_deref(), Some(raw.as_str()));
        assert_eq!(
            retained.as_ref().ok_or("missing test value")?.display(),
            raw
        );
        for lookalike in [
            "git init fehlgeschlagen: fixture native error",
            "Could not open y: fixture native error",
            "y isn't in a git repository: drop a repository's folder to open it",
            "Can't open y: drop a folder to open it, or a .patch, .diff, .eml or .mbox file to apply it",
            "Applying y needs a repository open first: open one, then drop the patch on it",
            "To apply y, show its repository's tab, then drop the patch there",
            "external 日本語\n: unknown",
        ] {
            let plain: AppError = lookalike.to_owned().into();
            assert!(plain.presentation.is_none());
            assert_eq!(plain.display(), lookalike);
        }
        // The existing finite ordinary-error paint fallback is preserved.
        assert_eq!(
            AppError::from("it is a symbolic link".to_owned()).display(),
            "Dies ist ein symbolischer Link."
        );
        // Production exposes only immutable Deref; simulate impossible stale metadata here.
        let mut mismatched = AppError::init("fixture native error".into());
        mismatched.raw = "it is a symbolic link".into();
        assert_eq!(mismatched.display(), "it is a symbolic link");
        assert_eq!(mismatched.delivery_text(), "it is a symbolic link");
        crate::i18n::set_language("en-US");

        Ok(())
    }
}
