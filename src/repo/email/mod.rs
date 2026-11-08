//! b4 owns mail transport and patch-series conventions. All Git/b4/file work is queued.
mod model;
mod run;
#[cfg(test)]
mod tests;
use super::{Action, RepoTab};
use crate::{connections, tasks};
use std::sync::{Arc, mpsc};

pub(crate) enum Act {
    Open,
    Read,
    Receive,
    Apply,
    Enroll,
    Cover,
    Revision,
    Reroll,
    Preview,
    Send,
    Cancel,
}
enum Reply {
    Read(run::Snapshot, Result<String, String>, String),
    Received(Arc<run::Received>),
    Preview(run::Preview),
    Done(String),
}
struct Pending {
    rx: mpsc::Receiver<Result<Reply, String>>,
    scope: tasks::Scope,
    context: connections::Context,
}
#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    source: String,
    receive_revision: u32,
    forkpoint: String,
    revision: String,
    previous: String,
    cover: String,
    info: String,
    info_failure: Option<(String, String)>,
    to: String,
    cc: String,
    trust: bool,
    confirm: bool,
    snapshot: Option<run::Snapshot>,
    received: Option<Arc<run::Received>>,
    preview: Option<run::Preview>,
    pending: Option<Pending>,
    result: String,
    result_presentation: Option<(String, LocalResult)>,
}
impl RepoTab {
    pub(super) fn email_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
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
    pub(super) fn email_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Open) {
            self.email.open = true;
            return;
        }
        if matches!(act, Act::Cancel) {
            if let Some(p) = &self.email.pending {
                p.scope.0.cancel();
            }
            return;
        }
        if self.email.pending.is_some() || self.busy_op.is_some() {
            return;
        }
        let state = &mut self.email;
        let path = self.path.clone();
        let work: Box<dyn FnOnce() -> Result<Reply, String> + Send> = match act {
            Act::Read => Box::new(move || {
                let snapshot = run::snapshot(&path)?;
                // An unenrolled branch is still readable so it can be enrolled from this UI.
                let info = run::b4(&path, &["prep".into(), "--show-info".into()]);
                let cover =
                    run::b4(&path, &["prep".into(), "--show-cover".into()]).unwrap_or_default();
                Ok(Reply::Read(snapshot, info, cover))
            }),
            Act::Receive => {
                let source = state.source.clone();
                let revision = state.receive_revision;
                Box::new(move || {
                    run::receive(&path, &source, revision).map(|r| Reply::Received(Arc::new(r)))
                })
            }
            Act::Apply => {
                let Some(received) = state.received.clone() else {
                    return;
                };
                Box::new(move || run::apply(&path, &received).map(Reply::Done))
            }
            Act::Preview => {
                let args = model::recipients(&state.to, &state.cc);
                Box::new(move || run::preview(&path, args?).map(Reply::Preview))
            }
            Act::Send => {
                if !state.confirm {
                    return;
                }
                let Some(preview) = state.preview.clone() else {
                    return;
                };
                if model::recipients(&state.to, &state.cc).ok().as_ref()
                    != Some(&preview.recipients)
                {
                    state.remember_result(LocalResult::RecipientsChanged);
                    return;
                }
                state.confirm = false;
                Box::new(move || run::send(&path, &preview).map(Reply::Done))
            }
            Act::Enroll | Act::Cover | Act::Revision | Act::Reroll => {
                if !state.trust {
                    return;
                }
                let Some(snapshot) = state.snapshot.clone() else {
                    return;
                };
                let (mode, value) = match act {
                    Act::Enroll => ("enroll", state.forkpoint.clone()),
                    Act::Cover => ("cover", state.cover.clone()),
                    Act::Revision => ("revision", state.revision.clone()),
                    _ => ("reroll", state.previous.clone()),
                };
                state.trust = false;
                Box::new(move || run::prepare(&path, &snapshot, mode, &value).map(Reply::Done))
            }
            Act::Open | Act::Cancel => return,
        };
        let scope = tasks::Scope(self._jobs.0.child());
        let (tx, rx) = mpsc::channel();
        let cancelled = tx.clone();
        let repaint = ctx.clone();
        let failure_repaint = ctx.clone();
        let submitted = tasks::pool().and_then(|pool| {
            pool.submit_cancellable(
                scope.0.clone(),
                tasks::Priority::Interactive,
                Some(super::messages::mutation_key(&self.path)),
                Some(Box::new(move || {
                    let _ = cancelled.send(Err("Cancelled before starting".into()));
                    failure_repaint.request_repaint();
                })),
                Box::new(move || {
                    let _ = tx.send(work());
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match submitted {
            Ok(()) => {
                self.busy_op = Some("Email patches · b4".into());
                state.pending = Some(Pending {
                    rx,
                    scope,
                    context: connections::current(),
                });
                state.remember_result(LocalResult::Running);
            }
            Err(e) => {
                state.result_presentation = None;
                state.result = e;
            }
        }
    }
}
fn messages(ui: &mut egui::Ui, messages: &[model::Message], salt: &str) {
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

#[derive(Clone)]
enum LocalResult {
    Failed { detail: String },
    ProfileChanged,
    Prepared,
    Received,
    Preview,
    WorkerEnded,
    Running,
    RecipientsChanged,
}
impl LocalResult {
    fn raw(&self) -> String {
        match self {
            Self::Failed { detail } => format!("{detail}\nIf cancelled or partially completed, inspect Git/b4/mail state before retrying. Apply conflicts use the existing Continue/Abort operation controls."),
Self::ProfileChanged => "Profile changed. Operation cancelled; inspect repository and mail state before retrying.".into(),
Self::Prepared => "Prepared branch read".into(),
Self::Received => "Series fetched; review the cover, patches and trailers before applying".into(),
Self::Preview => "Preview generated without sending mail".into(),
Self::WorkerEnded => "Worker ended or was cancelled. Inspect repository/mail state before retrying; mail may have been delivered.".into(),
Self::Running => "Working…".into(),
Self::RecipientsChanged => "Recipients changed; generate a new preview".into(),
}
    }
    fn display(&self) -> String {
        let id = match self {
            Self::Failed { detail } => {
                return crate::i18n::message(
                    "repo-workflows-email-email-window-632693ae",
                    &[("error", detail.as_str().into())],
                );
            }
            Self::ProfileChanged => "repo-workflows-email-email-window-8c1d9c9f",
            Self::Prepared => "repo-workflows-email-email-window-b8c5cd6e",
            Self::Received => "repo-workflows-email-email-window-624d6ba0",
            Self::Preview => "repo-workflows-email-email-window-2a42475c",
            Self::WorkerEnded => "repo-workflows-email-email-window-c6755006",
            Self::Running => "repo-workflows-email-email-act-5474eef8",
            Self::RecipientsChanged => "repo-workflows-email-email-act-3eece223",
        };
        crate::i18n::text(id).into()
    }
}

impl State {
    fn remember_result(&mut self, note: LocalResult) {
        let raw = note.raw();
        self.result = raw.clone();
        self.result_presentation = Some((raw, note));
    }
    fn display_result(&self) -> String {
        match &self.result_presentation {
            Some((raw, note)) if raw == &self.result => note.display(),
            _ => crate::repo::activity::display_workflow_error(&self.result),
        }
    }
}

impl State {
    fn result_ui(&self, ui: &mut egui::Ui) {
        ui.label(self.display_result());
    }
}

#[cfg(test)]
mod fluent_r1_result_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_r1_email_same_retained_state_repaints_without_work_or_consent_change() {
        for note in [
            LocalResult::Prepared,
            LocalResult::Received,
            LocalResult::Preview,
            LocalResult::ProfileChanged,
            LocalResult::WorkerEnded,
            LocalResult::RecipientsChanged,
            LocalResult::Running,
        ] {
            crate::i18n::set_language("de");
            let mut state = State {
                trust: true,
                ..Default::default()
            };
            state.remember_result(note);
            let raw = state.result.clone();
            let mut h = Harness::new_ui_state(|ui, state: &mut State| state.result_ui(ui), state);
            for lang in ["de", "en-US", "de"] {
                crate::i18n::set_language(lang);
                h.run();
                h.get_by_label(&h.state().display_result());
                assert_eq!(h.state().display_result() == raw, lang == "en-US");
                assert_eq!(h.state().result, raw);
                assert!(h.state().trust);
                assert!(h.state().pending.is_none());
            }
            h.state_mut().result_presentation = None;
            for lang in ["de", "en-US"] {
                crate::i18n::set_language(lang);
                assert_eq!(h.state().display_result(), raw);
            }
        }
        crate::i18n::set_language("en-US");
    }
}

#[cfg(test)]
mod fluent_r1_failure_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_r1_email_cached_failure_retains_retry_hint_and_opaque_multiline_details() {
        let detail = "provider;\nIf cancelled or partially completed: {$x} 日本語";
        crate::i18n::set_language("de");
        let mut state = State::default();
        state.remember_result(LocalResult::Failed {
            detail: detail.into(),
        });
        let raw = state.result.clone();
        let mut h = Harness::new_ui_state(|ui, state: &mut State| state.result_ui(ui), state);
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            h.run();
            let shown = h.state().display_result();
            h.get_by_label(&shown);
            assert!(shown.contains(detail));
            assert!(shown.contains(if lang == "de" {
                "Prüfen Sie nach Abbruch"
            } else {
                "If cancelled or partially completed"
            }));
            assert_eq!(h.state().result, raw);
            assert!(h.state().pending.is_none());
            assert!(!h.state().trust && !h.state().confirm);
        }
        crate::i18n::set_language("en-US");
    }
}
