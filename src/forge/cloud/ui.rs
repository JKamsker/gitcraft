use super::{Account, Service};
#[derive(Clone, Default)]
struct State {
    token: String,
    running: bool,
    result: Option<Result<(), String>>,
}
pub(crate) fn show(ui: &mut egui::Ui, profile: u64, s: Service, a: &mut Account) {
    ui.horizontal(|ui| {
        ui.label("Server");
        ui.text_edit_singleline(&mut a.server);
    });
    ui.horizontal(|ui| {
        ui.label("Stable native user ID");
        ui.text_edit_singleline(&mut a.user);
    });
    if s == Service::Azure {
        ui.checkbox(
            &mut a.pat,
            "Use a saved Azure PAT (otherwise installed az login)",
        );
        if !a.pat {
            ui.horizontal(|ui| {
                ui.label("Azure CLI configuration folder (optional)");
                ui.text_edit_singleline(&mut a.config_dir);
            });
            ui.small("The selected az login is verified against this Azure user GUID. No CLI global account is switched. CLI tokens must include expires_on.");
            return;
        }
    } else {
        ui.horizontal(|ui| {
            ui.label("Atlassian account email");
            ui.text_edit_singleline(&mut a.credential_user);
        });
    }
    let id = egui::Id::new((
        "native-forge-secret",
        profile,
        s,
        a.server.clone(),
        a.user.clone(),
        a.credential_user.clone(),
    ));
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<State>(id))
        .unwrap_or_default();
    ui.add_enabled(
        !state.running,
        egui::TextEdit::singleline(&mut state.token)
            .password(true)
            .hint_text("API token — secure store only"),
    );
    let mut work = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.running && !state.token.is_empty(),
                egui::Button::new("Save token securely"),
            )
            .clicked()
        {
            a.secret_ref = format!("profile-{profile}");
            work = Some(Some(std::mem::take(&mut state.token)));
        }
        if ui
            .add_enabled(
                !state.running && !a.secret_ref.is_empty(),
                egui::Button::new("Forget token"),
            )
            .clicked()
        {
            work = Some(None);
        }
    });
    if let Some(token) = work {
        state.running = true;
        state.result = None;
        ui.ctx().data_mut(|d| d.insert_temp(id, state.clone()));
        let selected = a.clone();
        let ctx = ui.ctx().clone();
        let result = crate::tasks::pool().and_then(|p| {
            p.submit(
                crate::tasks::Cancellation::default(),
                crate::tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let result = match token {
                        Some(token) => selected.store(s, &token),
                        None => selected.forget(s),
                    };
                    ctx.data_mut(|d| {
                        d.insert_temp(
                            id,
                            State {
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
    if state.running {
        ui.label("Updating secure store…");
    }
    if let Some(r) = &state.result {
        ui.label(match r {
            Ok(()) => "Secure store updated".into(),
            Err(e) => e.clone(),
        });
    }
    ui.small("Use the native account GUID/UUID from your provider profile API. Tokens remain on this computer; setup export carries only a credential reference. Bitbucket app passwords are not supported.");
    if !state.running {
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
    }
}
