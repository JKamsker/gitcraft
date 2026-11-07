//! Opt-in, view-owned SOPS comparison. Plaintext never enters DiffData, Msg, settings,
//! activity, syntax caches, Git patches, or a temporary file.
mod model;
#[cfg(test)]
mod tests;
mod tool;
mod view;

mod reader;
use crate::repo::{DiffKey, RepoTab};
pub(crate) use model::JsonEncoding;
use model::{inspect, recipients};
use reader::read_sides;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
#[cfg(test)]
use std::time::Duration;

pub(super) struct Row {
    kind: super::line_diff::Edit,
    old: Option<usize>,
    new: Option<usize>,
    text: String,
}
pub(super) struct Comparison {
    rows: Vec<Row>,
    content_changed: bool,
    metadata_changed: bool,
    recipients_changed: bool,
}

/// Deliberately no Debug/Serialize/Clone: decrypted text has exactly one UI owner.
#[derive(Default)]
pub(in crate::repo) struct State {
    pub enabled: bool,
    cancel: crate::tasks::Cancellation,
    pub json_encoding: JsonEncoding,
    result: Option<Result<Comparison, String>>,
    receiver: Option<Receiver<Result<Comparison, String>>>,
    active: Option<Arc<AtomicBool>>,
}
impl State {
    pub fn close(&mut self) {
        self.cancel.cancel();
        if let Some(active) = self.active.take() {
            active.store(false, Ordering::Release);
        }
        self.receiver = None;
        self.result = None;
        self.enabled = false;
    }
    fn begin(&mut self) -> (mpsc::Sender<Result<Comparison, String>>, Arc<AtomicBool>) {
        self.close();
        self.enabled = true;
        self.cancel = crate::tasks::cancellation().child();
        let (send, receive) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(true));
        self.receiver = Some(receive);
        self.active = Some(active.clone());
        (send, active)
    }
    fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.result = Some(result);
                self.receiver = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.result = Some(Err(
                    "Decryption stopped. Return to the encrypted patch and try again.".into(),
                ));
                self.receiver = None;
            }
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
pub(super) fn probe(repo: &Path, key: &DiffKey) -> bool {
    probe_checked(repo, key).unwrap_or(false)
}
pub(super) fn probe_checked(repo: &Path, key: &DiffKey) -> Option<bool> {
    read_sides(repo, key).ok().map(|(old, new)| {
        [old, new].iter().any(|text| {
            if !text.contains("sops") || !text.contains("ENC[") {
                return false;
            }
            // Malformed SOPS-looking files still offer an explicit attempt with a safe
            // parser error; ordinary nested metadata never masquerades as an envelope.
            match inspect(key.path(), text) {
                Ok(value) => value.is_some(),
                Err(_) => true,
            }
        })
    })
}

fn load(
    repo: &Path,
    key: &DiffKey,
    encoding: JsonEncoding,
    active: &dyn Fn() -> bool,
) -> Result<Comparison, String> {
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    // Practice Git reads retain its reviewed isolation. Failures are generalized so no
    // custom CLI error or decrypted source can enter app activity/output.
    let (old, new) = private_sides(repo, key, active).map_err(|_| {
        "Could not read both encrypted sides within 2 MiB. Refresh the encrypted patch.".to_owned()
    })?;
    let result =
        compare_with_encoding(key.path(), &old, &new, encoding, active, |format, input| {
            tool::decrypt(format, input, active)
        })?;
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    let current = private_sides(repo, key, active).map_err(|_| {
        "The comparison changed while decrypting. Refresh the encrypted patch.".to_owned()
    })?;
    if current != (old, new) {
        return Err("The comparison changed while decrypting. Refresh the encrypted patch and choose Show decrypted again.".into());
    }
    Ok(result)
}

/// Slow host/Git reads own only ciphertext. Cancellation never leaves decrypted rows
/// retained in a worker blocked on a remote read or filesystem. The normal Practice-aware
/// reader runs independently; this owner stops waiting within 20 ms (15 s overall limit).
fn private_sides(
    repo: &Path,
    key: &DiffKey,
    active: &dyn Fn() -> bool,
) -> Result<(String, String), String> {
    reader::read_sides_active(repo, key, active)
}

#[cfg(test)]
fn await_ciphertext(
    read: impl FnOnce() -> Result<(String, String), String> + Send + 'static,
    active: &dyn Fn() -> bool,
) -> Result<(String, String), String> {
    let (send, receive) = mpsc::channel();
    submit(move || {
        let _ = send.send(read());
    });
    loop {
        if !active() {
            return Err("Decryption cancelled.".into());
        }
        match receive.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(_) => return Err("Encrypted source reader stopped.".into()),
        }
    }
}

#[cfg(test)]
fn compare(
    path: &str,
    old: &str,
    new: &str,
    active: &dyn Fn() -> bool,
    decrypt: impl FnMut(model::Format, &str) -> Result<String, String>,
) -> Result<Comparison, String> {
    compare_with_encoding(path, old, new, JsonEncoding::Ask, active, decrypt)
}

fn compare_with_encoding(
    path: &str,
    old: &str,
    new: &str,
    encoding: JsonEncoding,
    active: &dyn Fn() -> bool,
    mut decrypt: impl FnMut(model::Format, &str) -> Result<String, String>,
) -> Result<Comparison, String> {
    let before = inspect(path, old)?;
    let after = inspect(path, new)?;
    if before.is_none() && after.is_none() {
        return Err("No supported SOPS metadata was found. Use the encrypted patch.".into());
    }
    if encoding == JsonEncoding::Ask
        && [before.as_ref(), after.as_ref()]
            .into_iter()
            .flatten()
            .any(|e| e.ambiguous_json)
    {
        return Err("This data + sops envelope could be a JSON document or binary content. Choose JSON document or Binary envelope above, then retry. The filename cannot identify the format.".into());
    }
    let recipients_changed = recipients(before.as_ref()) != recipients(after.as_ref());
    let metadata_changed =
        before.as_ref().map(|e| &e.metadata) != after.as_ref().map(|e| &e.metadata);
    let mut plain = |text: &str, envelope: &Option<model::Envelope>| {
        if !active() {
            return Err("Decryption cancelled.".into());
        }
        match envelope {
            Some(envelope) => {
                let format = if envelope.ambiguous_json && encoding == JsonEncoding::Binary {
                    model::Format::Binary
                } else {
                    envelope.format
                };
                let decrypted =
                    decrypt(format, text).and_then(|text| tool::plaintext(text.into_bytes()))?;
                // JSON encoding can escape binary NUL bytes. Validate the decoded data
                // string too, rather than accepting those bytes as supported text.
                if envelope.ambiguous_json && format == model::Format::Json {
                    let value: serde_json::Value =
                        serde_json::from_str(&decrypted).map_err(|_| {
                            "SOPS did not return the chosen JSON document format.".to_owned()
                        })?;
                    if let Some(data) = value.get("data").and_then(serde_json::Value::as_str) {
                        tool::plaintext(data.as_bytes().to_vec())?;
                    }
                }
                Ok(decrypted)
            }
            None => tool::plaintext(text.as_bytes().to_vec()),
        }
    };
    let old = plain(old, &before)?;
    let new = plain(new, &after)?;
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    use super::line_diff::{Edit, diff_lines};
    let (mut old_number, mut new_number) = (0, 0);
    let rows = diff_lines(
        &old.split_inclusive('\n').collect::<Vec<_>>(),
        &new.split_inclusive('\n').collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|(kind, text)| {
        let before = if kind != Edit::Add {
            old_number += 1;
            Some(old_number)
        } else {
            None
        };
        let after = if kind != Edit::Del {
            new_number += 1;
            Some(new_number)
        } else {
            None
        };
        Row {
            kind,
            old: before,
            new: after,
            text: text.trim_end_matches('\n').to_owned(),
        }
    })
    .collect();
    Ok(Comparison {
        rows,
        content_changed: old != new,
        metadata_changed,
        recipients_changed,
    })
}

impl RepoTab {
    pub(crate) fn discard_decrypted(&mut self) {
        if let Some(view) = &mut self.diff {
            view.encrypted.close();
        }
    }
    pub(in crate::repo) fn show_diff_decrypted(&mut self, on: bool, ctx: &egui::Context) {
        if on && (self.busy_op.is_some() || self.loading) {
            return;
        }
        let Some(view) = &mut self.diff else { return };
        if !on {
            view.encrypted.close();
            return;
        }
        if view.request != view.data_request || !matches!(&view.data,Some(Ok(data)) if data.sops) {
            return;
        }
        self.diff_rich = None;
        self.diff_as_file = false;
        self.diff_rendered = false;
        view.structural.enabled = false;
        view.chosen.clear();
        view.anchor = None;
        view.text_sel = None;
        let (send, active) = view.encrypted.begin();
        let encoding = view.encrypted.json_encoding;
        let (repo, key, ctx) = (self.path.clone(), view.key.clone(), ctx.clone());
        // Private receiver belongs to this view, not the tab's shared Msg queue. Closing
        // drops it immediately, so even a late completed plaintext cannot be retained there.
        #[cfg(test)]
        let test_program = tool::TEST_PROGRAM.with(|p| p.borrow().clone());
        let cancel = view.encrypted.cancel.clone();
        submit_with(cancel, move || {
            #[cfg(test)]
            tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = test_program);
            let live = || active.load(Ordering::Acquire);
            let result = load(&repo, &key, encoding, &live);
            if live() {
                let _ = send.send(result);
            }
            ctx.request_repaint();
        });
    }
}

pub(super) fn show(ui: &mut egui::Ui, state: &mut State) {
    state.poll();
    view::show(ui, state);
}

#[cfg(test)]
fn submit(work: impl FnOnce() + Send + 'static) {
    submit_with(crate::tasks::cancellation().child(), work);
}
fn submit_with(cancel: crate::tasks::Cancellation, work: impl FnOnce() + Send + 'static) {
    if let Ok(pool) = crate::tasks::pool() {
        let _ = pool.submit(
            cancel,
            crate::tasks::Priority::Interactive,
            None,
            Box::new(work),
        );
    }
}
