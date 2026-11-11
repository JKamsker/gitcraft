//! Remote target form controls and display-only validation.

use super::*;

/// The machine (WSL distribution or SSH host) and the folder there: whether Browse was clicked.
pub(super) fn form_fields(ui: &mut Ui, form: &mut RemoteForm, t: &Tokens) -> (bool, bool) {
    let mut enter = false;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut form.kind, Kind::Wsl, "WSL");
        ui.selectable_value(&mut form.kind, Kind::Ssh, "SSH");
    });
    ui.add_space(8.0);
    match form.kind {
        Kind::Wsl => distro_field(ui, form, t),
        Kind::Ssh => enter |= host_field(ui, form, t),
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-folder"))
            .color(t.text_dim)
            .small(),
    );
    let mut browse = false;
    ui.horizontal(|ui| {
        let path = ui.add(
            egui::TextEdit::singleline(&mut form.path)
                .hint_text(crate::i18n::text("interaction-forms-path-hint"))
                .desired_width(ui.available_width() - 90.0)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        enter |= super::super::clone::singleline_enter(&path);
        if std::mem::take(&mut form.focus) {
            path.request_focus();
        }
        if path.changed() {
            form.error = None;
        }
        browse = ui
            .add(
                egui::Button::new(crate::i18n::text("interaction-forms-browse"))
                    .min_size(vec2(80.0, 28.0)),
            )
            .on_hover_text(crate::i18n::text("interaction-forms-browse-tip"))
            .clicked();
    });
    (browse, enter)
}

pub(super) fn distro_field(ui: &mut Ui, form: &mut RemoteForm, t: &Tokens) {
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-distro"))
            .color(t.text_dim)
            .small(),
    );
    if form.distros_rx.is_some() {
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(
                RichText::new(crate::i18n::text("interaction-forms-looking-distros"))
                    .color(t.text_faint)
                    .small(),
            );
        });
        return;
    }
    if form.distros.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text("interaction-forms-no-distros"))
                .color(t.text_faint)
                .small(),
        );
        return;
    }
    egui::ComboBox::from_id_salt("remote-distro")
        .selected_text(form.distro.clone())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for d in &form.distros {
                ui.selectable_value(&mut form.distro, d.clone(), d);
            }
        });
}

pub(super) fn host_field(ui: &mut Ui, form: &mut RemoteForm, t: &Tokens) -> bool {
    let mut enter = false;
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-host"))
            .color(t.text_dim)
            .small(),
    );
    ui.horizontal(|ui| {
        let combo_w = if form.hosts.is_empty() { 0.0 } else { 40.0 };
        let host = ui.add(
            egui::TextEdit::singleline(&mut form.host)
                .hint_text(crate::i18n::text("interaction-forms-host-hint"))
                .desired_width(ui.available_width() - combo_w)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        enter |= super::super::clone::singleline_enter(&host);
        if !form.hosts.is_empty() {
            egui::ComboBox::from_id_salt("remote-ssh-hosts")
                .selected_text("")
                .width(24.0)
                .show_ui(ui, |ui| {
                    for h in &form.hosts {
                        if ui.selectable_label(form.host == *h, h).clicked() {
                            form.host = h.clone();
                        }
                    }
                })
                .response
                .on_hover_text(crate::i18n::text("interaction-forms-hosts-tip"));
        }
    });
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-keys"))
            .color(t.text_faint)
            .small(),
    );
    enter
}

/// Known connection presentation only; host/path validation and raw errors stay stable.
pub(in crate::app) fn display_message(raw: &str) -> Option<String> {
    use crate::i18n::{message, text};
    let id = match raw {
        "The folder is a path on that machine: /home/me/repo or ~/repo." => {
            Some("interaction-forms-remote-path")
        }
        "Pick a WSL distribution." => Some("interaction-forms-remote-distro"),
        "The host is what ssh takes: an alias of ~/.ssh/config or user@host." => {
            Some("interaction-forms-remote-host")
        }
        "Connecting failed unexpectedly." => Some("interaction-forms-remote-failed"),
        _ => None,
    };
    if let Some(id) = id {
        return Some(text(id).to_owned());
    }
    if let Some(place) = raw
        .strip_prefix("Connecting to ")
        .and_then(|s| s.strip_suffix('…'))
    {
        return Some(message(
            "interaction-forms-connecting",
            &[("place", place.into())],
        ));
    }
    raw.strip_prefix("could not find the home folder on ")
        .map(|machine| {
            message(
                "interaction-forms-remote-home",
                &[("machine", machine.into())],
            )
        })
}
