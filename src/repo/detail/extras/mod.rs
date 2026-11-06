//! More in a commit's details: its signature, the branches and tags it is in, its notes and
//! the later commits linking to it. Each is read in the background once the commit is
//! selected; what comes back for a commit no longer shown is dropped.

mod backlinks_view;
mod contained_view;
mod notes_view;
mod section;
mod signature_view;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::Ui;

use crate::git::{self, Backlink, Containment, Signature};
use crate::repo::{Action, Refresh, RepoTab};

/// How long a commit stays selected before its sections are read.
const SETTLE: Duration = Duration::from_millis(250);

/// The kinds of read, one of each running at a time.
#[derive(Clone, Copy)]
enum Kind {
    Signature,
    Contained,
    Notes,
    Backlinks,
}

/// The reads running, and when to start them for the commit selected.
#[derive(Default)]
struct Jobs {
    /// By [`Kind`]: one is running.
    running: [bool; 4],
    /// By [`Kind`]: another was asked for while it ran.
    again: [bool; 4],
    /// When to start reading the commit selected, once it has stayed so.
    due: Option<Instant>,
}

/// Later commits linking to it, and whether there were more than were kept.
type Backlinks = (Vec<Backlink>, bool);

/// What the sections show for the commit they are about.
#[derive(Default)]
pub(in crate::repo) struct Extras {
    /// The commit the sections are about; `None` for a stash or nothing.
    hash: Option<String>,
    signature: Option<Result<Signature, String>>,
    contained: Option<Result<Containment, String>>,
    backlinks: Option<Result<Backlinks, String>>,
    notes: Notes,
    /// Signatures checked in this session, by commit: checking runs gpg or ssh-keygen, so a
    /// commit selected again shows the verdict it had (Check again asks anew).
    checked: HashMap<String, Signature>,
    /// Notes being written and left unsaved, by (commit, namespace): back in the editor when
    /// that commit is shown again.
    drafts: HashMap<(String, String), String>,
    jobs: Jobs,
}

/// The notes section: the namespace picked, the note in it, and the editor.
#[derive(Default)]
struct Notes {
    /// Every namespace with notes, the default one first.
    namespaces: Vec<String>,
    /// Those that have notes here: the others have nothing to push.
    existing: Vec<String>,
    /// The one shown; `None` until the namespaces are read (the default one then).
    namespace: Option<String>,
    /// The note in it: `Ok(None)` when there is none.
    note: Option<Result<Option<String>, String>>,
    /// The text being written, while the editor is open.
    draft: Option<String>,
    /// A namespace being typed in, to add a note to.
    new_namespace: String,
    /// The remote to fetch notes from and push them to.
    remote: Option<String>,
    /// Waiting for a save or a delete.
    saving: bool,
    /// Asked to delete: the second click deletes.
    confirm_delete: bool,
    /// What went wrong saving or deleting.
    error: Option<String>,
}

/// What the sections ask for, done once they are drawn.
pub(super) enum ExtrasCmd {
    CheckSignature,
    ReloadContained,
    ReloadBacklinks,
    PickNamespace(String),
    SaveNote(String),
    DeleteNote,
    FetchNotes(String),
    PushNotes(String),
    Open(String),
}

impl RepoTab {
    /// Starts reading the sections of `hash` (or clears them for a stash).
    pub(in crate::repo) fn load_extras(&mut self, hash: &str, ctx: &egui::Context) {
        if self.stash_view.is_some() {
            self.drop_extras();
            return;
        }
        if self.extras.hash.as_deref() == Some(hash) {
            return;
        }
        let notes = std::mem::take(&mut self.extras.notes);
        // A note being written is kept for when its commit is shown again.
        if let (Some(old), Some(namespace), Some(draft)) =
            (&self.extras.hash, &notes.namespace, &notes.draft)
            && !draft.trim().is_empty()
        {
            self.extras
                .drafts
                .insert((old.clone(), namespace.clone()), draft.clone());
        }
        let draft = notes
            .namespace
            .as_ref()
            .and_then(|n| self.extras.drafts.remove(&(hash.to_owned(), n.clone())));
        self.extras.hash = Some(hash.to_owned());
        self.extras.signature = None;
        self.extras.contained = None;
        self.extras.backlinks = None;
        // The namespace and remote picked stay from commit to commit.
        self.extras.notes = Notes {
            namespaces: notes.namespaces,
            existing: notes.existing,
            namespace: notes.namespace,
            draft,
            remote: notes.remote,
            ..Notes::default()
        };
        // Read once the selection rests: going down the history with the arrow keys starts
        // nothing for the commits passed on the way.
        self.extras.jobs.due = Some(Instant::now() + SETTLE);
        if let Some(sig) = self.extras.checked.get(hash) {
            self.extras.signature = Some(Ok(sig.clone()));
        }
        ctx.request_repaint_after(SETTLE);
    }

    /// Forgets the commit the sections were about (the changes row is shown), keeping a note
    /// being written for when it is shown again.
    pub(in crate::repo) fn drop_extras(&mut self) {
        if let (Some(hash), Some(namespace), Some(draft)) = (
            self.extras.hash.take(),
            self.extras.notes.namespace.clone(),
            self.extras.notes.draft.take(),
        ) && !draft.trim().is_empty()
        {
            self.extras.drafts.insert((hash, namespace), draft);
        }
        self.extras.jobs.due = None;
    }

    /// A note is being written, which closing the view would lose.
    pub(in crate::repo) fn note_being_written(&self) -> bool {
        self.extras
            .notes
            .draft
            .as_ref()
            .is_some_and(|d| !d.trim().is_empty())
            || !self.extras.drafts.is_empty()
    }

    /// Starts reading the sections once the commit selected has stayed so for a moment.
    /// Called while the commit pane is drawn: hidden, it reads nothing.
    pub(in crate::repo) fn extras_tick(&mut self, ctx: &egui::Context) {
        let Some(due) = self.extras.jobs.due else {
            return;
        };
        let now = Instant::now();
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        self.extras.jobs.due = None;
        if self.extras.signature.is_none() {
            self.want(Kind::Signature, ctx);
        }
        for kind in [Kind::Contained, Kind::Notes, Kind::Backlinks] {
            self.want(kind, ctx);
        }
    }

    /// Reads again what a reload may have changed: the notes always (their refs aren't among
    /// those compared), what contains the commit and links to it when the refs moved.
    pub(in crate::repo) fn extras_after_reload(&mut self, refs_moved: bool, ctx: &egui::Context) {
        // Not read yet (it will be, as it is now), or about a commit no longer shown.
        if self.extras.hash.is_none()
            || self.extras.jobs.due.is_some()
            || self.extras.hash != self.detail_hash
        {
            return;
        }
        self.want(Kind::Notes, ctx);
        if refs_moved {
            self.want(Kind::Contained, ctx);
            self.want(Kind::Backlinks, ctx);
        }
    }

    /// Reads `kind` for the commit shown: now, or once the read of it running ends (one of
    /// each kind at a time, so that the commits passed by don't pile reads up).
    fn want(&mut self, kind: Kind, ctx: &egui::Context) {
        let i = kind as usize;
        if self.extras.jobs.running[i] {
            self.extras.jobs.again[i] = true;
            return;
        }
        let Some(hash) = self.extras.hash.clone() else {
            return;
        };
        self.extras.jobs.running[i] = true;
        match kind {
            Kind::Signature => self.check_signature(hash, ctx),
            Kind::Contained => self.load_contained(hash, ctx),
            Kind::Notes => self.load_notes(hash, ctx),
            Kind::Backlinks => self.load_backlinks(hash, ctx),
        }
    }

    /// A read of `kind` for `hash` ended: whether what it found is to be shown. When another
    /// was asked for meanwhile, it starts now, and this answer (older) is dropped.
    fn read_done(&mut self, kind: Kind, hash: &str, ctx: &egui::Context) -> bool {
        let i = kind as usize;
        self.extras.jobs.running[i] = false;
        if std::mem::take(&mut self.extras.jobs.again[i]) {
            self.want(kind, ctx);
            return false;
        }
        self.extras.hash.as_deref() == Some(hash)
    }

    fn check_signature(&mut self, hash: String, ctx: &egui::Context) {
        let path = self.path.clone();
        let asked = hash.clone();
        self.then(
            ctx,
            move || git::verify_signature(&path, &asked),
            move |tab, result, ctx| {
                // Checked: worth keeping, whichever commit is shown now.
                if let Ok(sig) = &result {
                    tab.extras.checked.insert(hash.clone(), sig.clone());
                }
                if tab.read_done(Kind::Signature, &hash, ctx) {
                    tab.extras.signature = Some(result);
                }
            },
        );
    }

    fn load_contained(&mut self, hash: String, ctx: &egui::Context) {
        let path = self.path.clone();
        let asked = hash.clone();
        self.then(
            ctx,
            move || git::containment(&path, &asked),
            move |tab, result, ctx| {
                if tab.read_done(Kind::Contained, &hash, ctx) {
                    tab.extras.contained = Some(result);
                }
            },
        );
    }

    fn load_backlinks(&mut self, hash: String, ctx: &egui::Context) {
        let path = self.path.clone();
        let asked = hash.clone();
        self.then(
            ctx,
            move || git::backlinks(&path, &asked),
            move |tab, result, ctx| {
                if tab.read_done(Kind::Backlinks, &hash, ctx) {
                    tab.extras.backlinks = Some(result);
                }
            },
        );
    }

    /// Reads the namespaces, then the note in the one picked (the default one at first).
    fn load_notes(&mut self, hash: String, ctx: &egui::Context) {
        let path = self.path.clone();
        let picked = self.extras.notes.namespace.clone();
        let asked = hash.clone();
        self.then(
            ctx,
            move || {
                let namespaces = git::notes_namespaces(&path)?;
                let namespace =
                    picked.unwrap_or_else(|| namespaces.names.first().cloned().unwrap_or_default());
                let note = git::read_note(&path, &namespace, &asked)?;
                Ok::<_, String>((namespaces, namespace, note))
            },
            move |tab, result, ctx| {
                if !tab.read_done(Kind::Notes, &hash, ctx) {
                    return;
                }
                let notes = &mut tab.extras.notes;
                match result {
                    Ok((found, namespace, note)) => {
                        let mut namespaces = found.names;
                        // One picked to write a first note in has no ref yet.
                        if !namespaces.contains(&namespace) {
                            namespaces.push(namespace.clone());
                        }
                        notes.namespaces = namespaces;
                        notes.existing = found.existing;
                        // Picked again meanwhile: that one's note comes next.
                        if notes.namespace.as_ref().is_none_or(|n| *n == namespace) {
                            notes.namespace = Some(namespace);
                            notes.note = Some(Ok(note));
                        }
                    }
                    Err(e) => notes.note = Some(Err(e)),
                }
            },
        );
    }

    /// Saves `text` as the note in the namespace shown (or deletes it, with `None`).
    fn write_note(&mut self, text: Option<String>, ctx: &egui::Context) {
        let (Some(hash), Some(namespace)) = (
            self.extras.hash.clone(),
            self.extras.notes.namespace.clone(),
        ) else {
            return;
        };
        let notes = &mut self.extras.notes;
        notes.saving = true;
        notes.error = None;
        notes.confirm_delete = false;
        let path = self.path.clone();
        let asked = hash.clone();
        self.then(
            ctx,
            move || match text {
                Some(text) => git::write_note(&path, &namespace, &asked, &text),
                None => git::remove_note(&path, &namespace, &asked),
            },
            move |tab, result, ctx| {
                if tab.extras.hash.as_deref() != Some(hash.as_str()) {
                    return;
                }
                let notes = &mut tab.extras.notes;
                notes.saving = false;
                match result {
                    Ok(()) => {
                        notes.draft = None;
                        tab.want(Kind::Notes, ctx);
                    }
                    Err(e) => notes.error = Some(e),
                }
            },
        );
    }

    /// Does what the sections asked for.
    fn extras_cmd(&mut self, cmd: ExtrasCmd, acts: &mut Vec<Action>, ctx: &egui::Context) {
        match cmd {
            ExtrasCmd::CheckSignature => {
                if let Some(hash) = &self.extras.hash {
                    self.extras.checked.remove(hash);
                }
                self.extras.signature = None;
                self.want(Kind::Signature, ctx);
            }
            ExtrasCmd::ReloadContained => {
                self.extras.contained = None;
                self.want(Kind::Contained, ctx);
            }
            ExtrasCmd::ReloadBacklinks => {
                self.extras.backlinks = None;
                self.want(Kind::Backlinks, ctx);
            }
            ExtrasCmd::PickNamespace(name) => {
                let notes = &mut self.extras.notes;
                // Not while a note is being written: it would be lost.
                if notes.draft.is_some() {
                    return;
                }
                if let Err(e) = git::notes_ref(&name) {
                    notes.error = Some(e);
                    return;
                }
                let name = git::short_namespace(name.trim()).to_owned();
                notes.namespace = Some(name);
                notes.note = None;
                notes.draft = None;
                notes.error = None;
                notes.confirm_delete = false;
                notes.new_namespace.clear();
                self.want(Kind::Notes, ctx);
            }
            ExtrasCmd::SaveNote(text) => self.write_note(Some(text), ctx),
            ExtrasCmd::DeleteNote => self.write_note(None, ctx),
            ExtrasCmd::FetchNotes(remote) => self.notes_op(&remote, true, acts),
            ExtrasCmd::PushNotes(remote) => self.notes_op(&remote, false, acts),
            ExtrasCmd::Open(hash) => acts.push(Action::SelectHash(hash)),
        }
    }

    /// Fetches the namespace shown from `remote` (or pushes it there), as an operation: with
    /// the sign-in it may need, and the notes read again once it is done.
    fn notes_op(&mut self, remote: &str, fetch: bool, acts: &mut Vec<Action>) {
        let Some(namespace) = self.extras.notes.namespace.clone() else {
            return;
        };
        let args = if fetch {
            git::fetch_notes_args(remote, &namespace)
        } else {
            git::push_notes_args(remote, &namespace)
        };
        match args {
            Ok(args) => acts.push(Action::Ops {
                label: format!(
                    "{} notes “{namespace}” {} {remote}",
                    if fetch { "Fetch" } else { "Push" },
                    if fetch { "from" } else { "to" },
                ),
                ops: vec![args],
                refresh: Refresh::All,
            }),
            Err(e) => self.extras.notes.error = Some(e),
        }
    }

    /// Does what the sections asked for while drawn.
    pub(super) fn extras_cmds(
        &mut self,
        cmds: Vec<ExtrasCmd>,
        acts: &mut Vec<Action>,
        ctx: &egui::Context,
    ) {
        for cmd in cmds {
            self.extras_cmd(cmd, acts, ctx);
        }
    }
}

impl Extras {
    /// The sections of the commit `hash`, below its hashes, if they are about it. `remotes`:
    /// the names of the remotes, to fetch and push notes; `idle`: no operation is running.
    pub(super) fn ui(
        &mut self,
        ui: &mut Ui,
        hash: &str,
        remotes: &[String],
        idle: bool,
        cmds: &mut Vec<ExtrasCmd>,
    ) {
        if self.hash.as_deref() != Some(hash) {
            return;
        }
        ui.add_space(8.0);
        signature_view::section(ui, self.signature.as_ref(), cmds);
        contained_view::section(ui, self.contained.as_ref(), cmds);
        notes_view::section(ui, &mut self.notes, remotes, idle, cmds);
        backlinks_view::section(ui, self.backlinks.as_ref(), cmds);
    }
}

#[cfg(test)]
mod tests;
