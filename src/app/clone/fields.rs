//! Clone form controls and display-only error translation.

use super::*;

/// The clone running, on the start page: where to, how far, and _Cancel_.
pub(in crate::app) fn cloning_line(ui: &mut Ui, cloning: &Cloning) {
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
pub(super) fn form_fields(ui: &mut Ui, form: &mut CloneForm, t: &Tokens) -> bool {
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
            && let Some(dir) = pick_clone_folder()
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

pub(in crate::app) use crate::a11y::singleline_enter;

pub(in crate::app) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Known clone presentation only; credential/error classification still reads the raw value.
pub(in crate::app) fn display_message(raw: &str) -> Option<String> {
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
