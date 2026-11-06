//! What the user typed and hasn't used yet, kept on disk per repository so that it survives a
//! crash or a restart: the commit message, unsent pull-request comments, reviews and replies, and
//! an interactive-rebase list being edited. Also the marker that tells an unclean exit apart
//! from a normal close.
//!
//! Only text typed into those boxes is kept: never a password, a token or any other credential.
//! The files live in gitcraft's data folder (never in the repository, which may be on another
//! machine), one per repository, each written whole to a temporary file and then renamed over
//! the old one, so a crash mid-write leaves the previous version.

mod keeper;
mod limits;
mod session;
mod store;
#[cfg(test)]
mod tests;
mod writer;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub(crate) use keeper::Keeper;
pub(crate) use session::Session;
pub(crate) use writer::Notice;

/// The version of the files' format; a file of another version is left alone (until something
/// new is typed for its repository).
const FORMAT: u32 = 1;

/// Everything kept for one repository.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RepoDrafts {
    pub commit: CommitDraft,
    /// Unsent text on pull requests, by number.
    pub pull_requests: BTreeMap<u64, PrText>,
    pub rebase: Option<RebaseDraft>,
}

impl RepoDrafts {
    /// Nothing worth keeping: the file is removed.
    pub fn is_empty(&self) -> bool {
        self.commit.is_empty()
            && self.pull_requests.values().all(PrText::is_empty)
            && self.rebase.is_none()
    }
}

/// The message in the commit box.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CommitDraft {
    pub summary: String,
    pub body: String,
    /// "Amend" was ticked.
    pub amend: bool,
}

impl CommitDraft {
    pub fn is_empty(&self) -> bool {
        self.summary.trim().is_empty() && self.body.trim().is_empty()
    }
}

/// What was typed on one pull request and not sent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PrText {
    /// The comment box on the page.
    pub comment: String,
    /// The review's summary.
    pub review: String,
    /// A comment on a line, or a reply in a line's thread.
    pub line_text: String,
    /// The line it is on, while the pull request still has the commits it was picked in.
    pub line: Option<LineDraft>,
}

impl PrText {
    pub fn is_empty(&self) -> bool {
        [&self.comment, &self.review, &self.line_text]
            .iter()
            .all(|text| text.trim().is_empty())
    }
}

/// The line of a pull request a comment is written on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct LineDraft {
    pub path: String,
    pub line: u64,
    /// The new version of the file, else the old one.
    pub new_side: bool,
    /// The pull request's latest commit when the line was picked: once it has moved, the line
    /// number may be another line's, and the line is let go of (the text stays).
    pub head: String,
}

/// An interactive rebase being set up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RebaseDraft {
    pub onto: String,
    pub onto_label: String,
    /// HEAD when the commits were read; a branch moved since is refused when started.
    pub head: String,
    /// In the editor's order, oldest first.
    pub entries: Vec<RebaseStep>,
}

/// One commit of the rebase list, and what is to become of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RebaseStep {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub summary: String,
    /// The commit's own message.
    pub original: String,
    pub merge: bool,
    /// `pick`, `reword`, `squash`, `fixup` or `drop`.
    pub step: String,
    /// The new message, for rewords.
    pub message: String,
}

/// The file's contents: the drafts and the repository they belong to.
#[derive(Serialize, Deserialize)]
struct DraftFile {
    format: u32,
    repo: PathBuf,
    drafts: RepoDrafts,
}

/// The drafts kept for `repo`, from what was last saved for it in this run, else from its file.
/// `Err` says why a file that is there couldn't be read.
pub(crate) fn load(repo: &Path) -> Result<RepoDrafts, String> {
    writer::shared().load(repo)
}

/// Keeps `drafts` as `repo`'s, written in the background (removed when empty).
pub(crate) fn save(repo: &Path, drafts: RepoDrafts) {
    writer::shared().save(repo, drafts);
}

/// What to say about saving drafts (a failure, or drafts cut down), once.
pub(crate) fn take_notice() -> Option<Notice> {
    writer::shared().take_notice()
}

/// Waits (up to `timeout`) for the drafts given to [`save`] to be on disk: at exit.
pub(crate) fn flush(timeout: std::time::Duration) {
    writer::shared().flush(timeout);
}

/// The drafts in `repo`'s file, whatever this run was given since: what the next start finds.
#[cfg(test)]
pub(crate) fn from_disk(repo: &Path) -> RepoDrafts {
    data_dir()
        .map(|d| store::file_of(&store::folder(&d), repo))
        .and_then(|file| store::read(&file, repo).ok())
        .unwrap_or_default()
}

/// Set by [`turn_off`]: this process keeps nothing.
static OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Keeps no drafts and no session marker in this process, from now on (before the window is
/// made): for tools driving the window, which mustn't read or write the user's.
pub(crate) fn turn_off() {
    OFF.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// gitcraft's data folder, where the drafts and the session markers are kept. A folder of its
/// own per test run, so tests never touch the user's.
fn data_dir() -> Option<PathBuf> {
    if OFF.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    if cfg!(test) {
        static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        return Some(
            DIR.get_or_init(|| {
                std::env::temp_dir().join(format!("gitcraft-drafts-test-{}", std::process::id()))
            })
            .clone(),
        );
    }
    eframe::storage_dir("gitcraft")
}
