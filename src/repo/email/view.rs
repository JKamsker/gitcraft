//! Mail workflow window and patch-series presentation.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn email_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let state = &mut self.email;
        if let Some(pending) = &state.pending {
            let stale = pending.context != connections::current();
            if stale {
                pending.scope.0.cancel();
            }
            match pending.rx.try_recv() {
                Ok(reply) => {
                    state.result_presentation = None;
                    state.pending = None;
                    self.busy_op = None;
                    state.trust = false;
                    state.confirm = false;
                    if stale {
                        state.remember_result(LocalResult::ProfileChanged);
                    } else {
                        match reply {
                            Ok(Reply::Read(snapshot, info, cover)) => {
                                state.snapshot = Some(snapshot);
                                state.info_failure = None;
                                state.info = match info {
                                    Ok(text) => text,
                                    Err(detail) => {
                                        let raw =
                                            format!("Not enrolled or b4 unavailable: {detail}");
                                        state.info_failure = Some((raw.clone(), detail));
                                        raw
                                    }
                                };
                                state.cover = cover;
                                state.remember_result(LocalResult::Prepared);
                            }
                            Ok(Reply::Received(received)) => {
                                state.received = Some(received);
                                state.remember_result(LocalResult::Received);
                            }
                            Ok(Reply::Preview(preview)) => {
                                state.preview = Some(preview);
                                state.remember_result(LocalResult::Preview);
                            }
                            Ok(Reply::Done(text)) => {
                                state.result = text;
                                state.snapshot = None;
                                state.preview = None;
                            }
                            Err(error) => {
                                state.remember_result(LocalResult::Failed { detail: error });
                                state.snapshot = None;
                                state.preview = None;
                            }
                        }
                    }
                    self.reload(ctx);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    state.pending = None;
                    self.busy_op = None;
                    state.remember_result(LocalResult::WorkerEnded);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
            }
        }
        let state = &mut self.email;
        if !state.open {
            return;
        }
        let mut open = true;
        egui::Window::new(crate::i18n::text(
            "repo-workflows-email-email-window-c19c9f05",
        ))
        .id(egui::Id::new(("email-patches", &self.path)))
        .open(&mut open)
        .default_width(760.0)
        .vscroll(true)
        .show(ctx, |ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-email-email-window-8c3ad598",
            ));
            if state.pending.is_some() {
                ui.add(crate::spinner::Spinner::new());
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-email-email-window-b64a0d4f",
                    ))
                    .clicked()
                {
                    acts.push(Action::Email(Act::Cancel));
                }
            }
            state.result_ui(ui);
            ui.add_enabled_ui(state.pending.is_none() && self.busy_op.is_none(), |ui| {
                ui.heading(crate::i18n::text(
                    "repo-workflows-email-email-window-03712771",
                ));
                ui.label(crate::i18n::text(
                    "repo-workflows-email-email-window-179e0bb7",
                ));
                ui.text_edit_singleline(&mut state.source);
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-email-email-window-49077514",
                    ));
                    ui.add(egui::DragValue::new(&mut state.receive_revision).range(0..=9999));
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-email-email-window-15173718",
                        ))
                        .clicked()
                    {
                        acts.push(Action::Email(Act::Receive));
                    }
                });
                if let Some(received) = &state.received {
                    ui.label(crate::i18n::message(
                        "repo-workflows-email-email-window-9cb42639",
                        &[
                            (
                                "received-snapshot-head-1",
                                (received.snapshot.head).clone().into(),
                            ),
                            ("received-messages-len-2", (received.messages.len()).into()),
                        ],
                    ));
                    egui::CollapsingHeader::new(crate::i18n::text(
                        "repo-workflows-email-email-window-d9adba22",
                    ))
                    .id_salt("email-cover")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(&received.cover);
                    });
                    messages(ui, &received.messages, "received");
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-email-email-window-e70c5648",
                        ))
                        .clicked()
                    {
                        acts.push(Action::Email(Act::Apply));
                    }
                }
                ui.separator();
                ui.heading(crate::i18n::text(
                    "repo-workflows-email-email-window-381b0db7",
                ));
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-email-email-window-1e3943ad",
                    ))
                    .clicked()
                {
                    acts.push(Action::Email(Act::Read));
                }
                ui.label(match &state.info_failure {
                    Some((raw, detail)) if raw == &state.info => crate::i18n::message(
                        "repo-workflows-email-email-act-fa043d04",
                        &[("e", detail.as_str().into())],
                    ),
                    _ => state.info.clone(),
                });
                ui.checkbox(
                    &mut state.trust,
                    crate::i18n::text("repo-workflows-email-email-window-54194499"),
                );
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-email-email-window-c962be80",
                    ));
                    ui.text_edit_singleline(&mut state.forkpoint);
                    if ui
                        .add_enabled(
                            state.trust && state.snapshot.is_some(),
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-email-email-window-bc346e4f",
                            )),
                        )
                        .clicked()
                    {
                        acts.push(Action::Email(Act::Enroll));
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut state.cover)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
                if ui
                    .add_enabled(
                        state.trust && state.snapshot.is_some(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-email-email-window-582e1946",
                        )),
                    )
                    .clicked()
                {
                    acts.push(Action::Email(Act::Cover));
                }
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-email-email-window-2e516d68",
                    ));
                    ui.text_edit_singleline(&mut state.revision);
                    if ui
                        .add_enabled(
                            state.trust && state.snapshot.is_some(),
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-email-email-window-1219b3d1",
                            )),
                        )
                        .clicked()
                    {
                        acts.push(Action::Email(Act::Revision));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-email-email-window-035719bf",
                    ));
                    ui.text_edit_singleline(&mut state.previous);
                    if ui
                        .add_enabled(
                            state.trust && state.snapshot.is_some(),
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-email-email-window-e6ec08b2",
                            )),
                        )
                        .clicked()
                    {
                        acts.push(Action::Email(Act::Reroll));
                    }
                });
                ui.separator();
                ui.heading(crate::i18n::text(
                    "repo-workflows-email-email-window-54cdddfe",
                ));
                ui.label(crate::i18n::text(
                    "repo-workflows-email-email-window-b0fcf938",
                ));
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-email-email-window-f4b06ef6",
                    ));
                    ui.text_edit_singleline(&mut state.to);
                });
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-email-email-window-b28037e9",
                    ));
                    ui.text_edit_singleline(&mut state.cc);
                });
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-email-email-window-3d343b32",
                    ))
                    .clicked()
                {
                    acts.push(Action::Email(Act::Preview));
                }
                if let Some(preview) = &state.preview {
                    messages(ui, &preview.messages, "outgoing");
                    ui.label(crate::i18n::text(
                        "repo-workflows-email-email-window-99fe2a7b",
                    ));
                    ui.checkbox(
                        &mut state.confirm,
                        crate::i18n::text("repo-workflows-email-email-window-6ad695fc"),
                    );
                    if ui
                        .add_enabled(
                            state.confirm,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-email-email-window-e6884e31",
                            )),
                        )
                        .clicked()
                    {
                        acts.push(Action::Email(Act::Send));
                    }
                }
            });
        });
        self.email.open = open;
    }
}

pub(super) fn messages(ui: &mut egui::Ui, messages: &[model::Message], salt: &str) {
    for (index, message) in messages.iter().enumerate() {
        egui::CollapsingHeader::new(crate::i18n::message(
            "repo-workflows-email-messages-8e0d8018",
            &[
                ("index-1-1", (index + 1).into()),
                (
                    "message-headers-lines-find-l-l-s-2",
                    (message
                        .headers
                        .lines()
                        .find(|l| l.starts_with("Subject:"))
                        .unwrap_or(crate::i18n::text("repo-workflows-email-messages-07ed6ccf")))
                    .into(),
                ),
            ],
        ))
        .id_salt((salt, index))
        .show(ui, |ui| {
            ui.monospace(&message.headers);
            ui.add(egui::Label::new(&message.body).selectable(true));
            egui::CollapsingHeader::new(crate::i18n::text(
                "repo-workflows-email-messages-a6252664",
            ))
            .id_salt((salt, index, "raw"))
            .show(ui, |ui| {
                ui.add(egui::Label::new(&message.raw).selectable(true));
            });
        });
    }
}
