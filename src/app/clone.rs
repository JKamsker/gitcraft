//! Cloning: the dialog, the clone in the background, and asking for credentials.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Stroke, Ui, vec2};

use super::{App, repo_root};
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
    owner: egui::ViewportId,
    pub(super) dest: PathBuf,
    rx: Receiver<Result<PathBuf, String>>,
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
                ui.label(
                    RichText::new(crate::i18n::text("interaction-forms-clone-title"))
                        .font(theme::semibold(15.0)),
                );
                ui.add_space(10.0);
                let enter = form_fields(ui, form, &t);
                if let Some(err) = &form.error {
                    ui.colored_label(
                        t.danger,
                        display_message(err).unwrap_or_else(|| git::display_error(err)),
                    );
                }
                ui.add_space(12.0);
                let valid = !form.url.trim().is_empty()
                    && !form.parent.trim().is_empty()
                    && !form.name.trim().is_empty();
                ui.horizontal(|ui| {
                    if crate::repo::primary_button(
                        ui,
                        crate::i18n::text("interaction-forms-clone"),
                        valid,
                        90.0,
                    )
                    .clicked()
                        || (enter && valid)
                    {
                        submit = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(crate::i18n::text("interaction-forms-cancel"))
                                .min_size(vec2(80.0, 30.0)),
                        )
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if resp.should_close() {
            cancel = true;
        }

        if cancel {
            self.clone_form = None;
        } else if submit {
            self.submit_clone(ctx);
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
            let result =
                git::clone_with_options(&url, &dest2, creds.as_ref(), &options, &following)
                    .and_then(|()| {
                        if crate::remote::is_remote(&dest2) {
                            Ok(dest2.clone())
                        } else {
                            repo_root(&dest2)
                        }
                    });
            let _ = tx.send(result);
            ctx2.request_repaint();
        });
        self.cloning = Some(Cloning {
            owner: self.windows.current,
            dest,
            rx,
            control,
        });
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
        let owner = cloning.owner;
        let cancelled = cloning.control.is_cancelled();
        self.cloning = None;
        self.in_window(owner, |app| app.finish_clone(dest, cancelled, result, ctx));
    }

    fn finish_clone(
        &mut self,
        dest: PathBuf,
        cancelled: bool,
        result: Result<PathBuf, String>,
        ctx: &egui::Context,
    ) {
        match result {
            Err(_) if cancelled => {
                self.clone_failed = false;
                self.error = Some(format!("Clone into {}: cancelled", dest.display()).into());
            }
            // On another machine, it opens once reached again, the credentials along.
            Ok(_) if crate::remote::is_remote(&dest) => {
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
            Ok(root) => {
                // Show it in a start page tab if one is active, otherwise in a new tab.
                self.open_root(root.clone(), ctx);
                // The credentials that cloned it also fetch and push in its tab, and in no
                // other: when it couldn't be opened, the active tab is another repository's.
                if let Some(form) = self.cloning_form.take().filter(|f| f.auth)
                    && let Ok(creds) = git::Credentials::for_url(
                        form.url.trim(),
                        form.username.trim().to_owned(),
                        form.password,
                    )
                {
                    self.credentials_for(&root, creds);
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
                None => self.error = Some(format!("Clone failed: {e}").into()),
            },
            Err(e) => {
                self.clone_failed = true;
                self.error = Some(format!("Clone failed: {e}").into());
            }
        }
        self.cloning_form = None;
    }

    #[cfg(test)]
    pub(super) fn recorded_clone_completion(&mut self, owner: egui::ViewportId, root: PathBuf) {
        self.start_clone();
        if let Some(form) = &mut self.clone_form {
            form.auth = true;
            form.url = "https://example.invalid/recorded/repository.git".into();
            form.username = "recorded-user".into();
            form.password = "recorded-secret".into();
        }
        self.cloning_form = self.clone_form.take();
        let (tx, rx) = channel();
        let _ = tx.send(Ok(root.clone()));
        self.cloning = Some(Cloning {
            owner,
            dest: root,
            rx,
            control: git::OpControl::new(|| {}),
        });
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
            _ if cancelled => crate::i18n::message(
                "interaction-forms-cancel-progress",
                &[("destination", cloning.dest.display().to_string().into())],
            ),
            Some(p) => crate::i18n::message(
                "interaction-forms-clone-progress",
                &[
                    ("destination", cloning.dest.display().to_string().into()),
                    ("progress", p.label().into()),
                ],
            ),
            None => crate::i18n::message(
                "interaction-forms-cloning",
                &[("destination", cloning.dest.display().to_string().into())],
            ),
        };
        ui.label(text);
        if !cancelled
            && ui
                .button(crate::i18n::text("interaction-forms-cancel"))
                .on_hover_text(crate::i18n::text("interaction-forms-cancel-tip"))
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
fn form_fields(ui: &mut Ui, form: &mut CloneForm, t: &Tokens) -> bool {
    let mut enter = false;
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-url"))
            .color(t.text_dim)
            .small(),
    );
    let url = ui.add(
        egui::TextEdit::singleline(&mut form.url)
            .hint_text("https://github.com/user/repo.git")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    enter |= singleline_enter(&url);
    if form.focus {
        url.request_focus();
        form.focus = false;
    }
    if url.changed() && !form.name_edited {
        form.name = git::clone_dir_name(&form.url);
    }
    ui.add_space(6.0);
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-parent"))
            .color(t.text_dim)
            .small(),
    );
    ui.horizontal(|ui| {
        let parent = ui.add(
            egui::TextEdit::singleline(&mut form.parent)
                .desired_width(ui.available_width() - 90.0)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        enter |= singleline_enter(&parent);
        if ui
            .add(
                egui::Button::new(crate::i18n::text("interaction-forms-browse"))
                    .min_size(vec2(80.0, 28.0)),
            )
            .clicked()
            && let Some(dir) = rfd::FileDialog::new()
                .set_title(crate::i18n::text("interaction-forms-clone-into"))
                .pick_folder()
        {
            form.parent = dir.display().to_string();
        }
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-name"))
            .color(t.text_dim)
            .small(),
    );
    let name = ui.add(
        egui::TextEdit::singleline(&mut form.name)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    enter |= singleline_enter(&name);
    if name.changed() {
        form.name_edited = true;
    }
    let dest = PathBuf::from(form.parent.trim()).join(form.name.trim());
    ui.label(
        RichText::new(dest.display().to_string())
            .color(t.text_faint)
            .small(),
    );
    egui::CollapsingHeader::new(crate::i18n::text("interaction-forms-options"))
        .id_salt("Clone options")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text("interaction-forms-depth"));
                enter |= singleline_enter(&ui.text_edit_singleline(&mut form.depth));
            });
            ui.checkbox(
                &mut form.options.blobless,
                crate::i18n::text("interaction-forms-blobless"),
            );
            let mut sparse = form.options.folders.is_some();
            if ui
                .checkbox(&mut sparse, crate::i18n::text("interaction-forms-cone"))
                .changed()
            {
                form.options.folders = sparse.then(Vec::new);
            }
            if sparse {
                ui.label(crate::i18n::text("interaction-forms-folders"));
                ui.text_edit_multiline(&mut form.sparse_folders);
            }
            ui.label(crate::i18n::text("interaction-forms-download-policy"));
        });
    if form.auth {
        ui.add_space(6.0);
        ui.label(
            RichText::new(crate::i18n::text("interaction-forms-credentials"))
                .color(t.text_dim)
                .small(),
        );
        ui.add_space(4.0);
        let username = ui.add(
            egui::TextEdit::singleline(&mut form.username)
                .hint_text(crate::i18n::text("interaction-forms-username"))
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        enter |= singleline_enter(&username);
        let password = ui.add(
            egui::TextEdit::singleline(&mut form.password)
                .password(true)
                .hint_text(crate::i18n::text("interaction-forms-password"))
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        enter |= singleline_enter(&password);
    }
    enter
}

pub(super) use crate::a11y::singleline_enter;

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Known clone presentation only; credential/error classification still reads the raw value.
pub(super) fn display_message(raw: &str) -> Option<String> {
    use crate::i18n::{message, text};
    let id = match raw {
        "Depth must be a positive number, or empty for full history" => {
            Some("interaction-forms-depth-error")
        }
        "Those credentials were refused." => Some("interaction-forms-refused"),
        "This repository needs you to sign in." => Some("interaction-forms-signin"),
        _ => None,
    };
    if let Some(id) = id {
        return Some(text(id).to_owned());
    }
    if let Some(destination) = raw
        .strip_prefix("Wait for the clone into ")
        .and_then(|s| s.strip_suffix(" to finish, or cancel it on the start page"))
    {
        return Some(message(
            "interaction-forms-wait",
            &[("destination", destination.into())],
        ));
    }
    if let Some(destination) = raw.strip_suffix(" already exists and isn't empty") {
        return Some(message(
            "interaction-forms-occupied",
            &[("destination", destination.into())],
        ));
    }
    if let Some(destination) = raw
        .strip_prefix("Clone into ")
        .and_then(|s| s.strip_suffix(": cancelled"))
    {
        return Some(message(
            "interaction-forms-cancelled",
            &[("destination", destination.into())],
        ));
    }
    raw.strip_prefix("Clone failed: ").map(|reason| {
        message(
            "interaction-forms-failed",
            &[("reason", git::display_error(reason).into())],
        )
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)] // Missing rendered fixture widgets must fail the test.
mod localization_tests {
    use super::*;
    use egui::Key;
    use egui_kittest::{Harness, kittest::Queryable};

    #[derive(Default)]
    struct EnterForm {
        parent: String,
        parent_id: Option<egui::Id>,
        name: String,
        name_id: Option<egui::Id>,
        browse: usize,
        submit: usize,
        name_lost_on_enter: bool,
        browse_focused_on_enter: bool,
    }

    fn enter_form() -> Harness<'static, EnterForm> {
        let h = Harness::new_ui_state(
            |ui, form: &mut EnterForm| {
                let parent = ui.text_edit_singleline(&mut form.parent);
                form.parent_id = Some(parent.id);
                if singleline_enter(&parent) {
                    form.submit += 1;
                }
                // Record intents only: canceling this simulated picker has no side effects.
                let browse = ui.button("Browse");
                if browse.clicked() {
                    form.browse += 1;
                }
                let name = ui.text_edit_singleline(&mut form.name);
                form.name_id = Some(name.id);
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    form.name_lost_on_enter = name.lost_focus();
                    form.browse_focused_on_enter = browse.has_focus();
                }
                if singleline_enter(&name) {
                    form.submit += 1;
                }
            },
            EnterForm {
                parent: "/inert".into(),
                name: "repo".into(),
                ..Default::default()
            },
        );
        // Keep the focus transition and its observations in one rendering pass.
        h.ctx
            .options_mut(|o| o.max_passes = std::num::NonZeroUsize::MIN);
        h
    }

    #[test]
    fn coalesced_tab_and_enter_does_not_submit() {
        for (empty, focus_name) in [(true, false), (false, false), (false, true)] {
            let mut h = enter_form();
            if empty {
                h.state_mut().parent.clear();
                h.state_mut().name.clear();
                h.step();
            }
            let field = if focus_name {
                h.state().name_id
            } else {
                h.state().parent_id
            }
            .expect("field rendered");
            h.ctx.memory_mut(|m| m.request_focus(field));
            h.step();
            // RawInput keeps both keys in one frame; Harness::key_press would split them.
            h.input_mut()
                .events
                .extend(
                    [Key::Tab, Key::Enter]
                        .into_iter()
                        .map(|key| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }),
                );
            h.step();
            // TextEdit handles Enter before registering Tab traversal and retains focus.
            // It must still not treat a traversal frame as a submission request.
            assert_eq!(h.ctx.memory(|m| m.focused()), Some(field));
            assert_eq!(h.state().browse, 0);
            assert_eq!(h.state().submit, 0);
        }
    }

    #[test]
    fn enter_during_backward_focus_transition_only_activates_browse() {
        let mut h = enter_form();
        let name = h.state().name_id.expect("name rendered");
        h.ctx.memory_mut(|m| m.request_focus(name));
        h.step();
        h.key_down_modifiers(egui::Modifiers::SHIFT, Key::Tab);
        h.step();
        // Backward navigation takes effect in the next frame, alongside this Enter.
        h.key_down(Key::Enter);
        h.step();
        assert!(h.state().name_lost_on_enter);
        assert!(h.state().browse_focused_on_enter);
        assert_eq!(h.state().browse, 1);
        assert_eq!(h.state().submit, 0);
        h.key_up(Key::Enter);
        h.key_up(Key::Tab);
        h.run_steps(3);
        assert_eq!(h.state().submit, 0, "picker cancellation must not submit");
    }

    #[test]
    fn enter_in_singleline_field_submits_after_surrendering_focus() {
        let mut h = enter_form();
        let name = h.state().name_id.expect("name rendered");
        h.ctx.memory_mut(|m| m.request_focus(name));
        h.step();
        h.key_press(Key::Enter);
        h.step();
        assert_eq!(h.state().browse, 0);
        assert_eq!(h.state().submit, 1);
        assert!(h.ctx.memory(|m| m.focused().is_none()));
    }

    #[test]
    fn enter_on_clone_cancel_closes_without_submitting() {
        crate::i18n::set_language("en-US");
        let mut h = Harness::new_ui_state(
            |ui, state: &mut Option<App>| {
                if state.is_none() {
                    let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                    let mut app = App::launched_using(&cc, None, || {
                        Ok("git version 2.43.0 (recorded)".into())
                    });
                    app.start_clone();
                    if let Some(form) = &mut app.clone_form {
                        form.url = "https://example.invalid/o/r".into();
                        form.parent = "/inert".into();
                        form.name = "repo".into();
                        form.depth = "invalid".into();
                        form.focus = false;
                    }
                    *state = Some(app);
                    return;
                }
                if let Some(app) = state {
                    app.clone_dialog(ui.ctx());
                }
            },
            None,
        );
        h.run_steps(3);
        h.get_by_label("Cancel").focus();
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(
            h.state()
                .as_ref()
                .is_some_and(|app| app.clone_form.is_none() && app.cloning.is_none())
        );
    }

    #[test]
    fn actual_clone_fields_translate_options_without_rewriting_url_path_or_credentials() {
        crate::i18n::set_language("de");
        let form = CloneForm {
            options: Default::default(),
            depth: "0010".into(),
            sparse_folders: "src/日本語\ndocs".into(),
            url: "https://example.invalid/user/repo.git".into(),
            parent: "/opaque/日本語".into(),
            name: "repo".into(),
            name_edited: true,
            error: None,
            focus: false,
            auth: true,
            username: "literal-user".into(),
            password: "inert-secret".into(),
        };
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, form: &mut CloneForm| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                form_fields(ui, form, &Tokens::get(ui.ctx()));
            },
            form,
        );
        h.run();
        h.get_by_label("Im Ordner");
        h.get_by_label("Klonoptionen").click();
        h.run();
        h.get_by_label("Ohne Blobs: Dateiinhalte bei Bedarf herunterladen")
            .click();
        h.run();
        assert!(h.state().options.blobless);
        h.get_by_label("Ausgewählte Ordner auschecken (Cone-Modus)")
            .click();
        h.run();
        assert_eq!(h.state().options.folders, Some(vec![]));
        h.get_by_label("Ordner relativ zum Repository, einer pro Zeile. Dateien im Stammverzeichnis sind immer enthalten.");
        assert_eq!(h.state().url, "https://example.invalid/user/repo.git");
        assert_eq!(h.state().parent, "/opaque/日本語");
        assert_eq!(h.state().depth, "0010");
        assert_eq!(h.state().sparse_folders, "src/日本語\ndocs");
        assert_eq!(h.state().username, "literal-user");
        assert_eq!(h.state().password, "inert-secret");
        crate::i18n::set_language("en-US");
        h.run();
        // The same expanded section retains its original persistent identity across locale changes.
        h.get_by_label("Depth (empty = full history)");
        h.get_by_label("Check out selected folders (cone mode)");
        assert!(h.state().options.blobless && h.state().options.folders.is_some());
    }

    #[test]
    fn clone_messages_keep_raw_auth_and_cancellation_identities_and_opaque_details() {
        crate::i18n::set_language("de");
        for (raw, expected) in [
            (
                "Those credentials were refused.",
                "Diese Zugangsdaten wurden abgelehnt.",
            ),
            (
                "This repository needs you to sign in.",
                "Für dieses Repository musst du dich anmelden.",
            ),
            (
                "Depth must be a positive number, or empty for full history",
                "Die Tiefe muss eine positive Zahl sein oder für die vollständige Historie leer bleiben",
            ),
            (
                "Wait for the clone into /opaque/日本語 to finish, or cancel it on the start page",
                "Warte, bis das Klonen nach /opaque/日本語 abgeschlossen ist, oder brich es auf der Startseite ab",
            ),
            (
                "/opaque/日本語 already exists and isn't empty",
                "/opaque/日本語 existiert bereits und ist nicht leer",
            ),
            (
                "Clone into /opaque/日本語: cancelled",
                "Klonen nach /opaque/日本語: abgebrochen",
            ),
            (
                "Clone failed: External Git diagnostic 日本語",
                "Klonen fehlgeschlagen: External Git diagnostic 日本語",
            ),
        ] {
            assert_eq!(display_message(raw).as_deref(), Some(expected));
        }
        assert!(display_message("External Git diagnostic 日本語").is_none());
        assert_eq!(
            super::super::panels::display_app_message("External Git diagnostic 日本語"),
            "External Git diagnostic 日本語"
        );
        crate::i18n::set_language("en-US");
        assert_eq!(
            display_message("Clone into /opaque/日本語: cancelled").as_deref(),
            Some("Clone into /opaque/日本語: cancelled")
        );
    }
}
