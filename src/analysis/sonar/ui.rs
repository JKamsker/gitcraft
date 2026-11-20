//! Profile → Connections: the SonarQube Server / Cloud section. The token is transient egui
//! state until saved to the credential store; only its reference is part of the profile.

use egui::Ui;

use super::super::{Connected, Failure};
use super::{Account, CLOUD_ORIGINS, Edition};

/// What the last action said; localized only when painted.
#[derive(Clone, Debug)]
enum Outcome {
    Saved,
    Forgotten,
    Checked(Connected),
    Refused(&'static str),
    Failed(Failure),
}

#[derive(Clone, Default)]
struct Entry {
    token: String,
    running: bool,
    outcome: Option<Outcome>,
}

fn t(id: &'static str) -> &'static str {
    crate::i18n::text(id)
}

pub(crate) fn account(ui: &mut Ui, profile: u64, account: &mut Account) {
    ui.label(t("analysis-connection-intro"));
    edition(ui, account);
    match account.edition {
        Edition::Server => {
            ui.horizontal(|ui| {
                ui.label(t("analysis-connection-server-url"));
                ui.add(
                    egui::TextEdit::singleline(&mut account.origin)
                        .hint_text("https://sonarqube.example.com"),
                );
            });
        }
        Edition::Cloud => cloud_fields(ui, profile, account),
    }
    if let Err(id) = account.validate() {
        ui.colored_label(ui.visuals().warn_fg_color, t(id));
    }
    token_row(ui, profile, account);
    ui.small(t("analysis-connection-policy"));
}

fn edition(ui: &mut Ui, account: &mut Account) {
    ui.horizontal(|ui| {
        let before = account.edition;
        ui.radio_value(
            &mut account.edition,
            Edition::Server,
            t("analysis-edition-server"),
        );
        ui.radio_value(
            &mut account.edition,
            Edition::Cloud,
            t("analysis-edition-cloud"),
        );
        if before != account.edition {
            let cloud = CLOUD_ORIGINS.contains(&account.base());
            match account.edition {
                Edition::Cloud if !cloud => account.origin = CLOUD_ORIGINS[0].to_owned(),
                Edition::Server if cloud => account.origin.clear(),
                _ => {}
            }
            if account.edition == Edition::Server {
                account.organization.clear();
            }
        }
    });
}

fn cloud_fields(ui: &mut Ui, profile: u64, account: &mut Account) {
    ui.horizontal(|ui| {
        ui.label(t("analysis-connection-region"));
        egui::ComboBox::from_id_salt(("sonar-region", profile))
            .selected_text(account.base().to_owned())
            .show_ui(ui, |ui| {
                for origin in CLOUD_ORIGINS {
                    ui.selectable_value(&mut account.origin, origin.to_owned(), origin);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label(t("analysis-connection-organization-label"));
        ui.text_edit_singleline(&mut account.organization);
    });
}

fn token_row(ui: &mut Ui, profile: u64, account: &mut Account) {
    let id = egui::Id::new(("sonar-token", profile, account.identity()));
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<Entry>(id))
        .unwrap_or_default();
    ui.add_enabled(
        !state.running,
        egui::TextEdit::singleline(&mut state.token)
            .password(true)
            .hint_text(t("analysis-token-hint")),
    );
    let ready = !state.running && account.configured() && account.validate().is_ok();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                ready && !state.token.is_empty(),
                egui::Button::new(t("analysis-token-save")),
            )
            .clicked()
        {
            account.secret_ref = format!("profile-{profile}");
            let token = std::mem::take(&mut state.token);
            let selected = account.clone();
            run(ui, id, &mut state, move || match selected.store(&token) {
                Ok(()) => Outcome::Saved,
                Err(message) => Outcome::Refused(message),
            });
        }
        let saved = !account.secret_ref.is_empty();
        if ui
            .add_enabled(
                !state.running && saved,
                egui::Button::new(t("analysis-token-forget")),
            )
            .clicked()
        {
            let selected = account.clone();
            run(ui, id, &mut state, move || match selected.forget() {
                Ok(()) => Outcome::Forgotten,
                Err(message) => Outcome::Refused(message),
            });
        }
        if ui
            .add_enabled(
                ready && saved,
                egui::Button::new(t("analysis-connection-check")),
            )
            .clicked()
        {
            let selected = account.clone();
            run(ui, id, &mut state, move || {
                if crate::network::refused().is_some() {
                    return Outcome::Failed(Failure::Offline);
                }
                match super::for_account(&selected).and_then(|mut c| c.check()) {
                    Ok(connected) => Outcome::Checked(connected),
                    Err(failure) => Outcome::Failed(failure),
                }
            });
        }
    });
    paint_outcome(ui, &state);
    if !state.running {
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
    }
}

fn paint_outcome(ui: &mut Ui, state: &Entry) {
    if state.running {
        ui.label(t("analysis-connection-working"));
    }
    let text = match &state.outcome {
        None => return,
        Some(Outcome::Saved) => t("analysis-token-saved").to_owned(),
        Some(Outcome::Forgotten) => t("analysis-token-forgotten").to_owned(),
        Some(Outcome::Checked(c)) => crate::i18n::message(
            "analysis-connection-ok",
            &[("version", c.version.clone().into())],
        ),
        Some(Outcome::Refused(id)) => t(id).to_owned(),
        Some(Outcome::Failed(failure)) => failure.text(),
    };
    ui.label(text);
}

/// Runs `job` off the UI thread and records its outcome under `id`.
fn run(
    ui: &Ui,
    id: egui::Id,
    state: &mut Entry,
    job: impl FnOnce() -> Outcome + std::marker::Send + 'static,
) {
    let ctx = ui.ctx().clone();
    state.running = true;
    state.outcome = None;
    ui.ctx().data_mut(|d| d.insert_temp(id, state.clone()));
    let submitted = crate::tasks::pool().and_then(|pool| {
        pool.submit(
            crate::tasks::Cancellation::default(),
            crate::tasks::Priority::Interactive,
            None,
            Box::new(move || {
                let outcome = job();
                ctx.data_mut(|d| {
                    d.insert_temp(
                        id,
                        Entry {
                            outcome: Some(outcome),
                            ..Entry::default()
                        },
                    )
                });
                ctx.request_repaint();
            }),
        )
        .map_err(str::to_owned)
    });
    if submitted.is_err() {
        state.running = false;
        state.outcome = Some(Outcome::Failed(Failure::Busy));
    }
}
