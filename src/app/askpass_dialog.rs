//! ssh's questions (a password, a key's passphrase, a new host's key), asked in the window
//! for the connection it is making to a repository's machine (see `remote::askpass`).

use egui::{RichText, Stroke, vec2};

use super::App;
use super::panels::Toast;
use crate::remote::askpass::{self, Kind, Question};
use crate::theme::{self, Tokens};

/// What the dialog keeps for the question shown.
#[derive(Default)]
pub(super) struct AskpassState {
    /// The question shown (a new one clears what was typed).
    shown: Option<u64>,
    typed: String,
    focus: bool,
    /// "Remember connection": keep the secret in the system's credential store.
    remember: bool,
}

impl App {
    pub(super) fn wake_askpass_owner(&self, ctx: &egui::Context) {
        if let Some(question) = askpass::pending()
            && self.askpass.shown != Some(question.id)
        {
            for command in [
                egui::ViewportCommand::Visible(true),
                egui::ViewportCommand::Minimized(false),
                egui::ViewportCommand::Focus,
            ] {
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, command);
            }
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    /// Whether ssh is asking something (the dialog takes the keyboard).
    pub(super) fn askpass_open(&self) -> bool {
        self.askpass.shown.is_some() || askpass::pending().is_some()
    }

    /// Shows ssh's oldest question, if any, and sends the answer back.
    pub(super) fn askpass_dialog(&mut self, ctx: &egui::Context) {
        // Authentication is deliberately application-modal: its shared SSH queue
        // has no viewport identity. Only ROOT presents it; all shells block commands
        // through askpass_open(), rather than racing to edit the same secret.
        if self.windows.current != egui::ViewportId::ROOT {
            return;
        }
        for notice in askpass::take_notices() {
            self.toasts.push(Toast::new(notice, false));
        }
        let Some(question) = askpass::pending() else {
            self.askpass = AskpassState::default();
            return;
        };
        if self.askpass.shown != Some(question.id) {
            self.askpass = AskpassState {
                shown: Some(question.id),
                typed: String::new(),
                focus: true,
                remember: false,
            };
        }
        // Nothing to answer (touch a security key): said, and let go.
        if question.kind == Kind::Notice {
            self.toasts.push(Toast::new(
                format!("{}: {}", question.host, question.prompt),
                false,
            ));
            askpass::answer(question.id, Some(String::new()));
            return;
        }
        match self.askpass_modal(ctx, &question) {
            Some(Answer::Give(text)) => {
                askpass::answer(question.id, Some(text));
                self.askpass = AskpassState::default();
            }
            Some(Answer::Remember(secret)) => {
                askpass::answer_and_remember(question.id, secret);
                self.askpass = AskpassState::default();
            }
            Some(Answer::Decline) => {
                askpass::answer(question.id, None);
                self.askpass = AskpassState::default();
            }
            None => {}
        }
    }

    fn askpass_modal(&mut self, ctx: &egui::Context, question: &Question) -> Option<Answer> {
        askpass_modal(ctx, &mut self.askpass, question)
    }
}

/// The actual modal, separate from its OS question/answer queue.
fn askpass_modal(
    ctx: &egui::Context,
    state: &mut AskpassState,
    question: &Question,
) -> Option<Answer> {
    askpass_modal_with_store(ctx, state, question, crate::portable::credential_store())
}

fn askpass_modal_with_store(
    ctx: &egui::Context,
    state: &mut AskpassState,
    question: &Question,
    credential_store: bool,
) -> Option<Answer> {
    let t = Tokens::get(ctx);
    let frame = egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::same(18))
        .shadow(ctx.global_style().visuals.window_shadow);
    let mut answer = None;
    let resp = egui::Modal::new(egui::Id::new(("askpass", question.id)))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(110))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            let title = match question.kind {
                Kind::TrustHost => crate::i18n::text("interaction-dialogs-trust-host"),
                _ => crate::i18n::text("interaction-dialogs-sign-in"),
            };
            ui.label(RichText::new(title).font(theme::semibold(15.0)));
            ui.label(RichText::new(&question.host).color(t.text_dim).small());
            ui.add_space(10.0);
            if question.forgotten {
                ui.colored_label(t.danger, crate::i18n::text("interaction-dialogs-forgotten"));
                ui.add_space(6.0);
            }
            ui.label(&question.prompt);
            ui.add_space(10.0);
            match question.kind {
                Kind::Secret => {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut state.typed)
                            .password(true)
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(8, 6)),
                    );
                    let enter = crate::a11y::singleline_enter(&field);
                    if std::mem::take(&mut state.focus) {
                        field.request_focus();
                    }
                    ui.add_space(8.0);
                    if credential_store {
                        ui.checkbox(
                            &mut state.remember,
                            crate::i18n::text("interaction-dialogs-remember"),
                        )
                        .on_hover_text(remember_hint(ui.ctx()));
                    } else {
                        // A portable copy set to leave nothing on this computer.
                        state.remember = false;
                        ui.label(
                            RichText::new(crate::i18n::text("interaction-dialogs-portable"))
                                .color(t.text_dim)
                                .small(),
                        );
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if crate::repo::primary_button(
                            ui,
                            crate::i18n::text("interaction-dialogs-sign-in"),
                            true,
                            90.0,
                        )
                        .clicked()
                            || enter
                        {
                            let typed = std::mem::take(&mut state.typed);
                            answer = Some(if state.remember {
                                Answer::Remember(typed)
                            } else {
                                Answer::Give(typed)
                            });
                        }
                        if ui
                            .add(
                                egui::Button::new(crate::i18n::text("interaction-dialogs-cancel"))
                                    .min_size(vec2(80.0, 30.0)),
                            )
                            .clicked()
                        {
                            answer = Some(Answer::Decline);
                        }
                    });
                }
                Kind::TrustHost | Kind::Confirm => {
                    let (yes, word) = match question.kind {
                        Kind::TrustHost => (crate::i18n::text("interaction-dialogs-trust"), "yes"),
                        _ => (crate::i18n::text("interaction-dialogs-ok"), ""),
                    };
                    ui.horizontal(|ui| {
                        if crate::repo::primary_button(ui, yes, true, 140.0).clicked() {
                            answer = Some(Answer::Give(word.to_owned()));
                        }
                        if ui
                            .add(
                                egui::Button::new(crate::i18n::text("interaction-dialogs-cancel"))
                                    .min_size(vec2(80.0, 30.0)),
                            )
                            .clicked()
                        {
                            answer = Some(if question.kind == Kind::TrustHost {
                                Answer::Give("no".to_owned())
                            } else {
                                Answer::Decline
                            });
                        }
                    });
                }
                Kind::Notice => {}
            }
        });
    if answer.is_none() && resp.should_close() {
        answer = Some(match question.kind {
            Kind::TrustHost => Answer::Give("no".to_owned()),
            _ => Answer::Decline,
        });
    }
    answer
}

/// Resolve platform prose and the configured palette shortcut when the help is shown.
fn remember_hint(ctx: &egui::Context) -> String {
    let store = crate::i18n::text(if cfg!(windows) {
        "interaction-dialogs-store-win"
    } else if cfg!(target_os = "macos") {
        "interaction-dialogs-store-mac"
    } else {
        "interaction-dialogs-store-linux"
    });
    crate::i18n::message(
        "interaction-dialogs-remember-hint",
        &[
            ("store", store.into()),
            (
                "palette",
                crate::keymap::tip(
                    ctx,
                    crate::i18n::text("interaction-presentation-palette"),
                    crate::keymap::Command::Palette,
                )
                .into(),
            ),
        ],
    )
}

enum Answer {
    Give(String),
    /// Give it, and keep it for good.
    Remember(String),
    Decline,
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui::{Key, accesskit::Role};
    use egui_kittest::{Harness, kittest::Queryable};

    fn secret_harness(remember: bool, store: bool) -> Harness<'static, (AskpassState, Option<u8>)> {
        crate::i18n::set_language("en-US");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (AskpassState, Option<u8>)| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                if state.1.is_some() {
                    return;
                }
                let question = Question {
                    id: 18,
                    host: "inert fixture".into(),
                    prompt: "Inert prompt".into(),
                    kind: Kind::Secret,
                    forgotten: false,
                };
                state.1 = askpass_modal_with_store(ui.ctx(), &mut state.0, &question, store).map(
                    |answer| match answer {
                        // Record only the variant; never display or forward a payload.
                        Answer::Remember(_) => 1,
                        Answer::Give(_) => 2,
                        Answer::Decline => 3,
                    },
                );
            },
            (
                AskpassState {
                    typed: "inert-input".into(),
                    remember,
                    ..Default::default()
                },
                None,
            ),
        );
        h.run_steps(3);
        h
    }

    #[test]
    fn remember_checkbox_enter_does_not_answer_and_password_enter_does() {
        for remember in [false, true] {
            let mut h = secret_harness(remember, true);
            h.get_by_role_and_label(Role::CheckBox, "Remember connection")
                .focus();
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().0.remember, !remember);
            assert_eq!(h.state().1, None);
            h.get_by_role(Role::PasswordInput).focus();
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().1, Some(if remember { 2 } else { 1 }));
        }
    }

    #[test]
    fn secret_modal_explicit_buttons_keep_their_actions() {
        for (label, expected) in [("Sign in", 2), ("Cancel", 3)] {
            let mut h = secret_harness(false, true);
            h.get_by_role_and_label(Role::Button, label).focus();
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().1, Some(expected));
        }
    }

    #[test]
    fn storage_disabled_secret_modal_never_requests_remembering() {
        let mut h = secret_harness(true, false);
        assert!(
            h.query_by_role_and_label(Role::CheckBox, "Remember connection")
                .is_none()
        );
        assert!(!h.state().0.remember);
        h.get_by_role(Role::PasswordInput).focus();
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().1, Some(2));
    }

    #[test]
    fn actual_host_trust_dialog_localizes_buttons_but_answers_literal_ssh_protocol() {
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (AskpassState, Option<String>)| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let question = Question {
                    id: 17,
                    host: "ssh 日本語".into(),
                    prompt: "external SSH fingerprint SHA256:abc".into(),
                    kind: Kind::TrustHost,
                    forgotten: false,
                };
                if let Some(Answer::Give(answer)) = askpass_modal(ui.ctx(), &mut state.0, &question)
                {
                    state.1 = Some(answer);
                }
            },
            (AskpassState::default(), None),
        );
        h.run();
        h.get_by_label("Diesem Host vertrauen?");
        h.get_by_label("external SSH fingerprint SHA256:abc");
        h.get_by_label("Vertrauen und verbinden").click();
        h.run();
        assert_eq!(h.state().1.as_deref(), Some("yes"));
        h.get_by_label("Abbrechen").click();
        h.run();
        assert_eq!(h.state().1.as_deref(), Some("no"));
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Trust this host?");
    }
}
