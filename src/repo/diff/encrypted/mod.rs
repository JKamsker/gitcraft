//! Opt-in, view-owned SOPS comparison. Plaintext never enters DiffData, Msg, settings,
//! activity, syntax caches, Git patches, or a temporary file.
mod model;
#[cfg(test)]
mod tests;
mod tool;
mod view;

use super::structural::reader::read_sides;
use crate::repo::{DiffKey, RepoTab};
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

fn load(repo: &Path, key: &DiffKey, active: &dyn Fn() -> bool) -> Result<Comparison, String> {
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    // Practice Git reads retain its reviewed isolation. Failures are generalized so no
    // custom CLI error or decrypted source can enter app activity/output.
    let (old, new) = private_sides(repo, key, active).map_err(|_| {
        "Could not read both encrypted sides within 2 MiB. Refresh the encrypted patch.".to_owned()
    })?;
    let result = compare(key.path(), &old, &new, active, |format, input| {
        tool::decrypt(repo, format, input, active)
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
    std::thread::spawn(move || {
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

fn compare(
    path: &str,
    old: &str,
    new: &str,
    active: &dyn Fn() -> bool,
    mut decrypt: impl FnMut(model::Format, &str) -> Result<String, String>,
) -> Result<Comparison, String> {
    let before = inspect(path, old)?;
    let after = inspect(path, new)?;
    if before.is_none() && after.is_none() {
        return Err("No supported SOPS metadata was found. Use the encrypted patch.".into());
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
                decrypt(envelope.format, text).and_then(|text| tool::plaintext(text.into_bytes()))
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
        let (repo, key, ctx) = (self.path.clone(), view.key.clone(), ctx.clone());
        // Private receiver belongs to this view, not the tab's shared Msg queue. Closing
        // drops it immediately, so even a late completed plaintext cannot be retained there.
        #[cfg(test)]
        let test_program = tool::TEST_PROGRAM.with(|p| p.borrow().clone());
        std::thread::spawn(move || {
            #[cfg(test)]
            tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = test_program);
            let live = || active.load(Ordering::Acquire);
            let result = load(&repo, &key, &live);
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
