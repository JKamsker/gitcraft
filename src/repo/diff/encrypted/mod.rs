//! Opt-in, view-owned SOPS comparison. Plaintext never enters DiffData, Msg, settings,
//! activity, syntax caches, Git patches, or a temporary file.
mod model;
#[cfg(test)]
mod tests;
mod tool;
mod view;

use super::structural::reader::read_sides;
use crate::repo::{DiffKey, RepoTab};
pub(crate) use model::JsonEncoding;
use model::{inspect, recipients};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::time::Duration;

pub(super) struct Row {
    kind: similar::ChangeTag,
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
    pub json_encoding: JsonEncoding,
    result: Option<Result<Comparison, String>>,
    receiver: Option<Receiver<Result<Comparison, String>>>,
    active: Option<Arc<AtomicBool>>,
}
impl State {
    pub fn close(&mut self) {
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

pub(super) fn probe(repo: &Path, key: &DiffKey) -> bool {
    read_sides(repo, key).is_ok_and(|(old, new)| {
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
    let (repo, key) = (repo.to_owned(), key.clone());
    await_ciphertext(move || read_sides(&repo, &key), active)
}

fn await_ciphertext(
    read: impl FnOnce() -> Result<(String, String), String> + Send + 'static,
    active: &dyn Fn() -> bool,
) -> Result<(String, String), String> {
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    let (send, receive) = mpsc::channel();
    crate::connections::spawn(move || {
        let _ = send.send(read());
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if !active() {
            return Err("Decryption cancelled.".into());
        }
        if std::time::Instant::now() >= deadline {
            return Err("Encrypted source read timed out.".into());
        }
        match receive.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
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
    let diff = similar::TextDiff::configure()
        .algorithm(similar::Algorithm::Patience)
        .timeout(Duration::from_millis(250))
        .diff_lines(&old, &new);
    let rows = diff
        .iter_all_changes()
        .map(|change| Row {
            kind: change.tag(),
            old: change.old_index().map(|n| n + 1),
            new: change.new_index().map(|n| n + 1),
            text: change.value().trim_end_matches('\n').to_owned(),
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
        if on && (self.busy_op.is_some() || self.loading || self.work_pending != 0) {
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
        crate::connections::spawn(move || {
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
