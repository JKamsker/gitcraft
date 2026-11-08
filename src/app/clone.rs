//! Cloning: the dialog, the clone in the background, and asking for credentials.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use egui::{Key, RichText, Stroke, Ui, vec2};

use super::{App, Tab, repo_root};
use crate::git;
use crate::theme::{self, Tokens};

pub(super) struct CloneForm {
    options: git::CloneOptions,
    depth: String,
    sparse_folders: String,
    url: String,
    parent: String,
    name: String,
    /// The user typed a name; stop deriving it from the URL.
    name_edited: bool,
    error: Option<String>,
    focus: bool,
    /// The remote asked for a username and password / token.
    auth: bool,
    username: String,
    password: String,
}

/// A clone running in the background.
pub(super) struct Cloning {
    pub(super) dest: PathBuf,
    rx: Receiver<Result<(), String>>,
    /// Its progress, and how to cancel it.
    pub(super) control: Arc<git::OpControl>,
}

impl App {
    pub(super) fn start_clone(&mut self) {
        let parent = self
            .clone_parent
            .clone()
            .or_else(|| {
                self.recent
                    .first()
                    .and_then(|p| p.parent())
                    .map(Path::to_path_buf)
            })
            .or_else(home_dir)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.clone_form = Some(CloneForm {
            options: Default::default(),
            depth: String::new(),
            sparse_folders: String::new(),
            url: String::new(),
            parent,
            name: String::new(),
            name_edited: false,
            error: None,
            focus: true,
            auth: false,
            username: String::new(),
            password: String::new(),
        });
    }

    /// The URL in the clone form, if it is open (for tests).
    #[cfg(test)]
    pub(super) fn clone_url(&self) -> Option<String> {
        self.clone_form.as_ref().map(|f| f.url.clone())
    }

    /// Puts `url` in the open clone form, and the folder name it gives.
    pub(super) fn set_clone_url(&mut self, url: &str) {
        if let Some(form) = &mut self.clone_form {
            form.url = url.to_owned();
            form.name = git::clone_dir_name(&form.url);
        }
    }

    pub(super) fn clone_dialog(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.clone_form else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut submit = false;
        let mut cancel = false;
        let resp = egui::Modal::new(egui::Id::new("clone-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(RichText::new("Clone repository").font(theme::semibold(15.0)));
                ui.add_space(10.0);
                form_fields(ui, form, &t);
                if let Some(err) = &form.error {
                    ui.colored_label(t.danger, err);
                }
                ui.add_space(12.0);
                let valid = !form.url.trim().is_empty()
                    && !form.parent.trim().is_empty()
                    && !form.name.trim().is_empty();
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(Key::Enter));
                    if crate::repo::primary_button(ui, "Clone", valid, 90.0).clicked()
                        || (enter && valid)
                    {
                        submit = true;
                    }
                    if ui
                        .add(egui::Button::new("Cancel").min_size(vec2(80.0, 30.0)))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if resp.should_close() {
            cancel = true;
        }

        if submit {
            self.submit_clone(ctx);
        } else if cancel {
            self.clone_form = None;
        }
    }

    /// Starts cloning what the form says, unless a clone is running or the destination is taken.
    fn submit_clone(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.clone_form else {
            return;
        };
        // One clone at a time: a second would take the first one's place, whose result
        // (and its tab) would never come.
        if let Some(running) = &self.cloning {
            form.error = Some(format!(
                "Wait for the clone into {} to finish, or cancel it on the start page",
                running.dest.display()
            ));
            return;
        }
        let dest = PathBuf::from(form.parent.trim()).join(form.name.trim());
        // Elsewhere, git there refuses a folder that isn't empty, and says so.
        let occupied = !crate::remote::is_remote(&dest)
            && dest.exists()
            && std::fs::read_dir(&dest)
                .map(|mut d| d.next().is_some())
                .unwrap_or(true);
        if occupied {
            form.error = Some(format!("{} already exists and isn't empty", dest.display()));
            return;
        }
        form.options.depth = if form.depth.trim().is_empty() {
            None
        } else {
            match form.depth.trim().parse::<u32>() {
                Ok(n) if n > 0 => Some(n),
                _ => {
                    form.error =
                        Some("Depth must be a positive number, or empty for full history".into());
                    return;
                }
            }
        };
        if form.options.folders.is_some() {
            form.options.folders = Some(
                form.sparse_folders
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
            );
        }
        if let Err(error) = form.options.validate() {
            form.error = Some(error);
            return;
        }
        let options = form.options.clone();
        let url = form.url.trim().to_owned();
        let creds = if form.auth && !form.username.trim().is_empty() {
            match git::Credentials::for_url(
                &url,
                form.username.trim().to_owned(),
                form.password.clone(),
            ) {
                Ok(creds) => Some(creds),
                Err(error) => {
                    form.error = Some(error);
                    return;
                }
            }
        } else {
            None
        };
        self.clone_parent = Some(PathBuf::from(form.parent.trim()));
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        let dest2 = dest.clone();
        let repaint = ctx.clone();
        let control = git::OpControl::new(move || repaint.request_repaint());
        let following = Arc::clone(&control);
        crate::connections::spawn(move || {
            let _ = tx.send(git::clone_with_options(
                &url,
                &dest2,
                creds.as_ref(),
                &options,
                &following,
            ));
            ctx2.request_repaint();
        });
        self.cloning = Some(Cloning { dest, rx, control });
        self.cloning_form = self.clone_form.take();
        self.error = None;
    }

    pub(super) fn poll_clone(&mut self, ctx: &egui::Context) {
        let Some(cloning) = &self.cloning else {
            return;
        };
        let Ok(result) = cloning.rx.try_recv() else {
            return;
        };
        let dest = cloning.dest.clone();
        let cancelled = cloning.control.is_cancelled();
        self.cloning = None;
        match result {
            Err(_) if cancelled => {
                self.clone_failed = false;
                self.error = Some(format!("Clone into {}: cancelled", dest.display()));
            }
            // On another machine, it opens once reached again, the credentials along.
            Ok(()) if crate::remote::is_remote(&dest) => {
                let creds = self
                    .cloning_form
                    .take()
                    .filter(|f| f.auth)
                    .and_then(|form| {
                        git::Credentials::for_url(
                            form.url.trim(),
                            form.username.trim().to_owned(),
                            form.password,
                        )
                        .ok()
                    });
                self.connect(dest, creds, ctx);
            }
            Ok(()) => {
                // Show it in a start page tab if one is active, otherwise in a new tab.
                self.open_repo(&dest, ctx);
                // The credentials that cloned it also fetch and push in its tab, and in no
                // other: when it couldn't be opened, the active tab is another repository's.
                let root = repo_root(&dest).ok();
                if let Some(form) = self.cloning_form.take().filter(|f| f.auth)
                    && let Some(Tab::Repo(repo)) = self.tabs.get_mut(self.active)
                    && root.as_ref() == Some(&repo.path)
                    && let Ok(creds) = git::Credentials::for_url(
                        form.url.trim(),
                        form.username.trim().to_owned(),
                        form.password,
                    )
                {
                    repo.set_credentials(creds);
                }
            }
            // Back to the form, asking for credentials.
            Err(e) if git::is_auth_error(&e) => match self.cloning_form.take() {
                Some(mut form) => {
                    form.error = Some(if form.auth {
                        "Those credentials were refused.".to_owned()
                    } else {
                        "This repository needs you to sign in.".to_owned()
                    });
                    form.auth = true;
                    form.password.clear();
                    self.clone_form = Some(form);
                }
                None => self.error = Some(format!("Clone failed: {e}")),
            },
            Err(e) => {
                self.clone_failed = true;
                self.error = Some(format!("Clone failed: {e}"));
            }
        }
        self.cloning_form = None;
    }
}

/// The clone running, on the start page: where to, how far, and _Cancel_.
pub(super) fn cloning_line(ui: &mut Ui, cloning: &Cloning) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.add(crate::spinner::Spinner::new());
        let cancelled = cloning.control.is_cancelled();
        let progress = cloning.control.progress();
        let text = match &progress {
            _ if cancelled => format!("Cancelling the clone into {}…", cloning.dest.display()),
            Some(p) => format!("Cloning into {}… {}", cloning.dest.display(), p.label()),
            None => format!("Cloning into {}…", cloning.dest.display()),
        };
        ui.label(text);
        if !cancelled
            && ui
                .button("Cancel")
                .on_hover_text("Stop the clone and remove what it cloned so far")
                .clicked()
        {
            cloning.control.cancel();
        }
    });
    if let Some(f) = cloning.control.progress().and_then(|p| p.fraction()) {
        let (rect, _) = ui.allocate_exact_size(
            vec2(ui.available_width().min(460.0), 3.0),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 0.0, t.separator);
        let done = egui::Rect::from_min_size(rect.min, vec2(rect.width() * f, rect.height()));
        ui.painter().rect_filled(done, 0.0, t.accent);
    }
}

/// URL, folder and name, where that puts the clone, and the credentials once asked for.
fn form_fields(ui: &mut Ui, form: &mut CloneForm, t: &Tokens) {
    ui.label(RichText::new("URL").color(t.text_dim).small());
    let url = ui.add(
        egui::TextEdit::singleline(&mut form.url)
            .hint_text("https://github.com/user/repo.git")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    if form.focus {
        url.request_focus();
        form.focus = false;
    }
    if url.changed() && !form.name_edited {
        form.name = git::clone_dir_name(&form.url);
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Into folder").color(t.text_dim).small());
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut form.parent)
                .desired_width(ui.available_width() - 90.0)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        if ui
            .add(egui::Button::new("Browse…").min_size(vec2(80.0, 28.0)))
            .clicked()
            && let Some(dir) = rfd::FileDialog::new().set_title("Clone into").pick_folder()
        {
            form.parent = dir.display().to_string();
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Name").color(t.text_dim).small());
    if ui
        .add(
            egui::TextEdit::singleline(&mut form.name)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 6)),
        )
        .changed()
    {
        form.name_edited = true;
    }
    let dest = PathBuf::from(form.parent.trim()).join(form.name.trim());
    ui.label(
        RichText::new(dest.display().to_string())
            .color(t.text_faint)
            .small(),
    );
    ui.collapsing("Clone options", |ui| {
        ui.horizontal(|ui| { ui.label("Depth (empty = full history)"); ui.text_edit_singleline(&mut form.depth); });
        ui.checkbox(&mut form.options.blobless, "Blobless: download file contents on demand");
        let mut sparse = form.options.folders.is_some();
        if ui.checkbox(&mut sparse, "Check out selected folders (cone mode)").changed() {
            form.options.folders = sparse.then(Vec::new);
        }
        if sparse { ui.label("Repository-relative folders, one per line. Root files are always included."); ui.text_edit_multiline(&mut form.sparse_folders); }
        ui.label("Clone downloads the selected checkout. Later history reads never download automatically; use Graph tools → Sparse checkout and downloads for consent. Passive object reads require Git with --no-lazy-fetch support.");
    });
    if form.auth {
        ui.add_space(6.0);
        ui.label(
            RichText::new("The remote wants to know who you are. Most hosts take a personal access token as the password.")
                .color(t.text_dim)
                .small(),
        );
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut form.username)
                .hint_text("Username")
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        ui.add(
            egui::TextEdit::singleline(&mut form.password)
                .password(true)
                .hint_text("Password or token")
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 6)),
        );
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
