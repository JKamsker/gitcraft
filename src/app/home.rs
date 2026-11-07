//! The start page (open, create, clone, recent repositories) and the git-missing page.

use std::path::Path;

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

impl App {
    pub(super) fn pick_existing(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Open a git repository")
            .pick_folder()
        {
            self.open_repo(&dir, ctx);
        }
    }

    pub(super) fn pick_new(&mut self, ctx: &egui::Context) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Choose a folder for the new repository")
            .pick_folder()
        else {
            return;
        };
        // In WSL: all of it there, in the background (WSL may first have to start).
        if crate::remote::is_remote(&dir) {
            let label = crate::remote::display(&dir);
            self.connect_with(label, None, ctx, move || {
                if !git::toplevel(&dir).is_ok_and(|root| same_path(&root, &dir)) {
                    git::init(&dir).map_err(|e| format!("git init failed: {e}"))?;
                }
                super::repo_root(&dir)
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
            Err(e) => self.error = Some(format!("git init failed: {e}")),
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
                            ("folder-open", "Open", "An existing local repository"),
                            ("folder-plus", "Create", "A new, empty repository"),
                            ("log-in", "Clone", "Copy a remote repository"),
                            ("server", "Remote", "In WSL or over SSH"),
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
                    if ui.link("Scan a folder for repositories…").clicked() {
                        action = Some(4);
                    }

                    self.home_link_field(ui, &t);

                    // The card has git's words itself.
                    if let Some(err) = self.error.as_ref().filter(|_| self.error_card.is_none()) {
                        ui.add_space(12.0);
                        ui.colored_label(t.danger, err);
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
                            ui.label(format!("Connecting to {}…", connecting.label));
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
                .link("Add gitcraft to the applications menu")
                .on_hover_text(appimage.display().to_string());
            if link.clicked() {
                self.error = crate::desktop_entry::install(&appimage).err();
            }
        }
    }

    pub(super) fn git_missing(&mut self, ui: &mut Ui, err: &str) {
        let t = Tokens::get(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.25);
            icons::inline(ui, "circle-alert", 40.0, t.warning);
            ui.add_space(8.0);
            ui.label(RichText::new("Git was not found").font(theme::semibold(24.0)));
            ui.add_space(8.0);
            ui.label("gitcraft uses the git command line. Install git and make sure `git` is on your PATH.");
            ui.add_space(4.0);
            ui.label(RichText::new(err).color(t.text_faint).small());
            ui.add_space(16.0);
            if crate::repo::primary_button(ui, "Check again", true, 140.0).clicked() {
                self.git_version = git::version();
            }
            ui.add_space(8.0);
            ui.hyperlink_to("Download git", "https://git-scm.com/downloads");
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
