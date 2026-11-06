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
}

impl App {
    /// Whether ssh is asking something (the dialog takes the keyboard).
    pub(super) fn askpass_open(&self) -> bool {
        self.askpass.shown.is_some()
    }

    /// Shows ssh's oldest question, if any, and sends the answer back.
    pub(super) fn askpass_dialog(&mut self, ctx: &egui::Context) {
        let Some(question) = askpass::pending() else {
            self.askpass = AskpassState::default();
            return;
        };
        if self.askpass.shown != Some(question.id) {
            self.askpass = AskpassState {
                shown: Some(question.id),
                typed: String::new(),
                focus: true,
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
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if crate::repo::primary_button(ui, "Sign in", true, 90.0).clicked()
                                || enter
                            {
                                answer = Some(Answer::Give(std::mem::take(&mut state.typed)));
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

enum Answer {
    Give(String),
    Decline,
}
