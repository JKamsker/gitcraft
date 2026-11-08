use super::{Account, Kind};
use egui::Ui;
#[derive(Clone, Default)]
struct Entry {
    token: String,
    running: bool,
    result: Option<Result<(), String>>,
}
/// Token input is transient egui state. Only the reference is part of profile serialization.
pub(crate) fn account(ui: &mut Ui, profile: u64, kind: Kind, account: &mut Account) {
    ui.label(match kind {
        Kind::Jira => "Jira Cloud: site origin, account email and API token",
        Kind::Linear => "Linear: https://api.linear.app, account email and personal API key",
    });
    ui.horizontal(|ui| {
        ui.label("Origin");
        ui.text_edit_singleline(&mut account.origin);
    });
    ui.horizontal(|ui| {
        ui.label("Email");
        ui.text_edit_singleline(&mut account.user);
    });
    if kind == Kind::Jira {
        ui.horizontal(|ui| {
            ui.label("Criteria field (optional)");
            ui.text_edit_singleline(&mut account.acceptance_field);
        });
    }
    let id = egui::Id::new((
        "tracker-token",
        profile,
        kind,
        account.origin.clone(),
        account.user.clone(),
        account.acceptance_field.clone(),
    ));
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<Entry>(id))
        .unwrap_or_default();
    ui.add_enabled(
        !state.running,
        egui::TextEdit::singleline(&mut state.token)
            .password(true)
            .hint_text("Token — saved only in this computer's credential store"),
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.running && !state.token.is_empty(),
                egui::Button::new("Save token securely"),
            )
            .clicked()
        {
            account.secret_ref = format!("profile-{profile}");
            let account = account.clone();
            let token = std::mem::take(&mut state.token);
            let ctx = ui.ctx().clone();
            state.running = true;
            state.result = None;
            ui.ctx().data_mut(|d| d.insert_temp(id, state.clone()));
            let submitted = crate::tasks::pool().and_then(|p| {
                p.submit(
                    crate::tasks::Cancellation::default(),
                    crate::tasks::Priority::Interactive,
                    None,
                    Box::new(move || {
                        let result = account.store(kind, &token);
                        ctx.data_mut(|d| {
                            d.insert_temp(
                                id,
                                Entry {
                                    result: Some(result),
                                    ..Default::default()
                                },
                            )
                        });
                        ctx.request_repaint();
                    }),
                )
                .map_err(str::to_owned)
            });
            if let Err(e) = submitted {
                state.running = false;
                state.result = Some(Err(e));
            }
        }
        if ui
            .add_enabled(
                !state.running && !account.secret_ref.is_empty(),
                egui::Button::new("Forget token"),
            )
            .clicked()
        {
            let selected = account.clone();
            let ctx = ui.ctx().clone();
            state.running = true;
            state.result = None;
            ui.ctx().data_mut(|d| d.insert_temp(id, state.clone()));
            let result = crate::tasks::pool().and_then(|p| {
                p.submit(
                    crate::tasks::Cancellation::default(),
                    crate::tasks::Priority::Interactive,
                    None,
                    Box::new(move || {
                        let result = if !crate::portable::credential_store() {
                            Err("Credential storage is disabled".into())
                        } else {
                            keyring::Entry::new("gitcraft-tracker", &selected.store_key())
                                .map_err(|_| "Cannot open tracker credential store".into())
                                .and_then(|e| {
                                    e.delete_credential()
                                        .map_err(|_| "Cannot remove tracker credential".into())
                                })
                        };
                        ctx.data_mut(|d| {
                            d.insert_temp(
                                id,
                                Entry {
                                    result: Some(result),
                                    ..Default::default()
                                },
                            )
                        });
                        ctx.request_repaint();
                    }),
                )
                .map_err(str::to_owned)
            });
            if let Err(e) = result {
                state.running = false;
                state.result = Some(Err(e));
            }
        }
    });
    if state.running {
        ui.label("Updating secure store…");
    }
    if let Some(r) = &state.result {
        ui.label(match r {
            Ok(()) => "Secure store updated".to_owned(),
            Err(e) => e.clone(),
        });
    }
    ui.small("Setup export includes account metadata and a reference. Re-enter the token on another computer. Requests use this exact HTTPS origin and refuse redirects. Jira Data Center and Linear OAuth are not configured here.");
    if !state.running {
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
    }
}
