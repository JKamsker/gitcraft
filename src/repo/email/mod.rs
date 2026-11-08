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
    Read(run::Snapshot, String, String),
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
    to: String,
    cc: String,
    trust: bool,
    confirm: bool,
    snapshot: Option<run::Snapshot>,
    received: Option<Arc<run::Received>>,
    preview: Option<run::Preview>,
    pending: Option<Pending>,
    result: String,
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
                    state.pending = None;
                    self.busy_op = None;
                    state.trust = false;
                    state.confirm = false;
                    if stale {
                        state.result = "Profile changed. Operation cancelled; inspect repository and mail state before retrying.".into();
                    } else {
                        match reply {
                            Ok(Reply::Read(snapshot, info, cover)) => {
                                state.snapshot = Some(snapshot);
                                state.info = info;
                                state.cover = cover;
                                state.result = "Prepared branch read".into();
                            }
                            Ok(Reply::Received(received)) => {
                                state.received = Some(received);
                                state.result = "Series fetched; review the cover, patches and trailers before applying".into();
                            }
                            Ok(Reply::Preview(preview)) => {
                                state.preview = Some(preview);
                                state.result = "Preview generated without sending mail".into();
                            }
                            Ok(Reply::Done(text)) => {
                                state.result = text;
                                state.snapshot = None;
                                state.preview = None;
                            }
                            Err(error) => {
                                state.result = format!(
                                    "{error}\nIf cancelled or partially completed, inspect Git/b4/mail state before retrying. Apply conflicts use the existing Continue/Abort operation controls."
                                );
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
                    state.result = "Worker ended or was cancelled. Inspect repository/mail state before retrying; mail may have been delivered.".into();
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
        egui::Window::new("Email patches · b4").id(egui::Id::new(("email-patches", &self.path))).open(&mut open).default_width(760.0).vscroll(true).show(ctx, |ui| {
            ui.label("Requires b4 ≥ 0.17 on this machine. Uses b4's configured identity, public-inbox and sending transport. No credentials are exported by gitcraft.");
            if state.pending.is_some() { ui.add(crate::spinner::Spinner::new()); if ui.button("Cancel operation").clicked() { acts.push(Action::Email(Act::Cancel)); } }
            ui.label(&state.result);
            ui.add_enabled_ui(state.pending.is_none() && self.busy_op.is_none(), |ui| {
                ui.heading("Receive and review");
                ui.label("Public-inbox HTTPS URL or Message-ID"); ui.text_edit_singleline(&mut state.source);
                ui.horizontal(|ui| { ui.label("Revision (0 = latest)"); ui.add(egui::DragValue::new(&mut state.receive_revision).range(0..=9999)); if ui.button("Fetch series with b4").clicked() { acts.push(Action::Email(Act::Receive)); } });
                if let Some(received) = &state.received {
                    ui.label(format!("Fetched for HEAD {} · {} patches", received.snapshot.head, received.messages.len()));
                    egui::CollapsingHeader::new("Cover letter").default_open(true).show(ui, |ui| { ui.label(&received.cover); });
                    messages(ui, &received.messages, "received");
                    if ui.button("Apply reviewed series (git am --3way)").clicked() { acts.push(Action::Email(Act::Apply)); }
                }
                ui.separator(); ui.heading("Author and reroll");
                if ui.button("Read prepared branch and cover").clicked() { acts.push(Action::Email(Act::Read)); }
                ui.label(&state.info);
                ui.checkbox(&mut state.trust, "I understand b4 preparation can rewrite this branch and change its metadata");
                ui.horizontal(|ui| { ui.label("Fork point"); ui.text_edit_singleline(&mut state.forkpoint); if ui.add_enabled(state.trust && state.snapshot.is_some(), egui::Button::new("Enroll current branch")).clicked() { acts.push(Action::Email(Act::Enroll)); } });
                ui.add(egui::TextEdit::multiline(&mut state.cover).desired_rows(8).desired_width(f32::INFINITY));
                if ui.add_enabled(state.trust && state.snapshot.is_some(), egui::Button::new("Save cover with b4")).clicked() { acts.push(Action::Email(Act::Cover)); }
                ui.horizontal(|ui| { ui.label("Revision"); ui.text_edit_singleline(&mut state.revision); if ui.add_enabled(state.trust && state.snapshot.is_some(), egui::Button::new("Set numbered revision")).clicked() { acts.push(Action::Email(Act::Revision)); } });
                ui.horizontal(|ui| { ui.label("Previous sent Message-ID"); ui.text_edit_singleline(&mut state.previous); if ui.add_enabled(state.trust && state.snapshot.is_some(), egui::Button::new("Reroll from sent series")).clicked() { acts.push(Action::Email(Act::Reroll)); } });
                ui.separator(); ui.heading("Preview and send");
                ui.label("Additional recipients. b4 also derives recipients from its cover/configuration and review trailers; inspect every generated message below.");
                ui.horizontal(|ui| { ui.label("To"); ui.text_edit_singleline(&mut state.to); });
                ui.horizontal(|ui| { ui.label("Cc"); ui.text_edit_singleline(&mut state.cc); });
                if ui.button("Generate outgoing preview (no mail)").clicked() { acts.push(Action::Email(Act::Preview)); }
                if let Some(preview) = &state.preview {
                    messages(ui, &preview.messages, "outgoing");
                    ui.label("Sending regenerates volatile Date, Message-ID and signature headers. Recipients, subjects and decoded MIME content are checked against this preview. b4's successful-send tagging/reroll behavior applies.");
                    ui.checkbox(&mut state.confirm, "I reviewed every recipient and message and want to send real mail");
                    if ui.add_enabled(state.confirm, egui::Button::new("Send reviewed series through b4")).clicked() { acts.push(Action::Email(Act::Send)); }
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
                let info = run::b4(&path, &["prep".into(), "--show-info".into()])
                    .unwrap_or_else(|e| format!("Not enrolled or b4 unavailable: {e}"));
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
                    state.result = "Recipients changed; generate a new preview".into();
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
                state.result = "Working…".into();
            }
            Err(e) => state.result = e,
        }
    }
}
fn messages(ui: &mut egui::Ui, messages: &[model::Message], salt: &str) {
    for (index, message) in messages.iter().enumerate() {
        egui::CollapsingHeader::new(format!(
            "Message {} · {}",
            index + 1,
            message
                .headers
                .lines()
                .find(|l| l.starts_with("Subject:"))
                .unwrap_or("Mail")
        ))
        .id_salt((salt, index))
        .show(ui, |ui| {
            ui.monospace(&message.headers);
            ui.add(egui::Label::new(&message.body).selectable(true));
            egui::CollapsingHeader::new("Raw RFC822 message")
                .id_salt((salt, index, "raw"))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(&message.raw).selectable(true));
                });
        });
    }
}
