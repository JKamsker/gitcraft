//! ssh's questions (a password, a key's passphrase, a new host's key), asked in the window
//! for the connection it is making to a repository's machine (see `remote::askpass`).

use egui::{Key, RichText, Stroke, vec2};

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
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let state = &mut self.askpass;
        let mut answer = None;
        let resp = egui::Modal::new(egui::Id::new(("askpass", question.id)))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(480.0);
                let title = match question.kind {
                    Kind::TrustHost => "Trust this host?",
                    _ => "Sign in",
                };
                ui.label(RichText::new(title).font(theme::semibold(15.0)));
                ui.label(RichText::new(&question.host).color(t.text_dim).small());
                ui.add_space(10.0);
                if question.forgotten {
                    ui.colored_label(
                        t.danger,
                        "The saved password wasn't accepted, and is forgotten.",
                    );
                    ui.add_space(6.0);
                }
                ui.label(&question.prompt);
                ui.add_space(10.0);
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                match question.kind {
                    Kind::Secret => {
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut state.typed)
                                .password(true)
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::symmetric(8, 6)),
                        );
                        if std::mem::take(&mut state.focus) {
                            field.request_focus();
                        }
                        ui.add_space(8.0);
                        if crate::portable::credential_store() {
                            ui.checkbox(&mut state.remember, "Remember connection")
                                .on_hover_text(REMEMBER_HINT);
                        } else {
                            // A portable copy set to leave nothing on this computer.
                            state.remember = false;
                            ui.label(
                                RichText::new(
                                    "Asked again after a restart: this portable copy doesn't \
                                     use the credential store (Settings → General).",
                                )
                                .color(t.text_dim)
                                .small(),
                            );
                        }
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if crate::repo::primary_button(ui, "Sign in", true, 90.0).clicked()
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
                                .add(egui::Button::new("Cancel").min_size(vec2(80.0, 30.0)))
                                .clicked()
                            {
                                answer = Some(Answer::Decline);
                            }
                        });
                    }
                    Kind::TrustHost | Kind::Confirm => {
                        let (yes, word) = match question.kind {
                            Kind::TrustHost => ("Trust and connect", "yes"),
                            _ => ("OK", ""),
                        };
                        ui.horizontal(|ui| {
                            if crate::repo::primary_button(ui, yes, true, 140.0).clicked() {
                                answer = Some(Answer::Give(word.to_owned()));
                            }
                            if ui
                                .add(egui::Button::new("Cancel").min_size(vec2(80.0, 30.0)))
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
}

/// Where "Remember connection" keeps it.
const REMEMBER_HINT: &str = if cfg!(windows) {
    concat!(
        "Saved encrypted in Windows Credential Manager, so it isn't asked again after a ",
        "restart. Forget it from the command palette (Ctrl+P): Forget saved SSH sign-ins."
    )
} else if cfg!(target_os = "macos") {
    concat!(
        "Saved encrypted in the macOS Keychain, so it isn't asked again after a restart. ",
        "Forget it from the command palette (Cmd+P): Forget saved SSH sign-ins."
    )
} else {
    concat!(
        "Saved encrypted in your keyring (Secret Service: GNOME Keyring, KWallet), so it ",
        "isn't asked again after a restart. Forget it from the command palette (Ctrl+P): ",
        "Forget saved SSH sign-ins."
    )
};

enum Answer {
    Give(String),
    /// Give it, and keep it for good.
    Remember(String),
    Decline,
}
