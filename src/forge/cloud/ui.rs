use super::{Account, Service};
#[derive(Clone, Default)]
struct State {
    token: String,
    running: bool,
    result: Option<Result<(), String>>,
}
pub(crate) fn show(ui: &mut egui::Ui, profile: u64, s: Service, a: &mut Account) {
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text("backend-label-server"));
        ui.text_edit_singleline(&mut a.server);
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text("backend-stable-native-user-id-bbb58d"));
        ui.text_edit_singleline(&mut a.user);
    });
    if s == Service::Azure {
        ui.checkbox(
            &mut a.pat,
            crate::i18n::text("backend-use-a-saved-azure-pat-otherwise-installed-az-login-c351de"),
        );
        if !a.pat {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text(
                    "backend-azure-cli-configuration-folder-optional-b6c5f7",
                ));
                ui.text_edit_singleline(&mut a.config_dir);
            });
            ui.small(crate::i18n::text(
                "backend-the-selected-az-login-is-verified-against-this-azure-user-guid-no-0d4ad9",
            ));
            return;
        }
    } else {
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text("backend-atlassian-account-email-c47881"));
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
            .hint_text(crate::i18n::text(
                "backend-api-token-secure-store-only-e98d63",
            )),
    );
    let mut work = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.running && !state.token.is_empty(),
                egui::Button::new(crate::i18n::text("backend-save-token-securely-4d8aea")),
            )
            .clicked()
        {
            a.secret_ref = format!("profile-{profile}");
            work = Some(Some(std::mem::take(&mut state.token)));
        }
        if ui
            .add_enabled(
                !state.running && !a.secret_ref.is_empty(),
                egui::Button::new(crate::i18n::text("backend-forget-token-ef4db6")),
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
        ui.label(crate::i18n::text("backend-updating-secure-store-c11579"));
    }
    if let Some(r) = &state.result {
        ui.label(match r {
            Ok(()) => crate::i18n::text("backend-secure-store-updated-9ffa60").into(),
            Err(e) => e.clone(),
        });
    }
    ui.small(crate::i18n::text(
        "backend-use-the-native-account-guid-uuid-from-your-provider-profile-api-t-ecee3a",
    ));
    if !state.running {
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
    }
}
