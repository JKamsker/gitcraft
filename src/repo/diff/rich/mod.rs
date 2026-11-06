//! Readings of a diff that understand the file: renderers shown in the diff pane in place of
//! its lines, each for the kinds of file it knows (prose; more plug in beside it). They read
//! both sides of the file on a background thread; a file they can't read (malformed, too big)
//! falls back to the plain diff with a note saying why. Staging stays in the plain diff.

pub(super) mod prose;
pub(super) mod prose_view;
mod tab;
#[cfg(test)]
mod tests;
mod words;

use std::path::Path;

use egui::Ui;

use super::rendered::sides;
use crate::git;
use crate::repo::{Action, DiffKey};

/// Past this size a side isn't read for a rich view: the plain diff is shown.
const MAX_SIDE_BYTES: usize = 8 * 1024 * 1024;

/// A way of reading a file's diff besides its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RichKind {
    /// Paragraphs wrapped, the words that changed marked.
    Prose,
}

impl RichKind {
    /// The readings `path` can be shown in, in the order their buttons show.
    pub fn of(path: &str) -> Vec<RichKind> {
        let ext = extension(path);
        let mut kinds = Vec::new();
        if prose::EXTENSIONS.contains(&ext.as_str()) {
            kinds.push(RichKind::Prose);
        }
        kinds
    }

    /// The icon of its button in the diff's header.
    pub fn icon(self) -> &'static str {
        match self {
            RichKind::Prose => "book-open",
        }
    }

    /// What its button does.
    pub fn tip(self) -> &'static str {
        match self {
            RichKind::Prose => "Prose: wrapped paragraphs, the words that changed marked",
        }
    }

    /// What it's called in a note.
    pub fn name(self) -> &'static str {
        match self {
            RichKind::Prose => "prose",
        }
    }
}

/// The file's extension, lowercase.
pub(super) fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// The file before and after the change, as text (`None`: not on that side).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sides {
    pub old: Option<String>,
    pub new: Option<String>,
}

/// A reading of the diff, ready to show.
pub enum RichDiff {
    Prose(prose::ProseDiff),
}

/// What reading the diff for a rich view gave.
pub enum Rich {
    Ready(RichDiff),
    /// It can't be read that way: the plain diff shows, with this note saying why.
    Plain(String),
}

/// Reads both sides of `key`'s file. A side that isn't there (a new or deleted file) is
/// `None`; one too big or not text makes the rich view fall back (`Err` holds the note).
pub(super) fn read_sides(repo: &Path, key: &DiffKey) -> Result<Sides, Result<String, String>> {
    let (old, new) = sides(key);
    let read = |side: &Option<(Option<String>, String)>| -> Result<Option<String>, Result<String, String>> {
        let Some((rev, path)) = side else {
            return Ok(None);
        };
        // Not there on that side (added, deleted): only the other one shows.
        let Ok(bytes) = git::file_at(repo, rev.as_deref(), path) else {
            return Ok(None);
        };
        if bytes.len() > MAX_SIDE_BYTES {
            return Err(Ok(format!(
                "The file is over {} MB",
                MAX_SIDE_BYTES / 1024 / 1024
            )));
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| Ok("The file isn't UTF-8 text".to_owned()))
    };
    let sides = Sides {
        old: read(&old)?,
        new: read(&new)?,
    };
    if sides.old.is_none() && sides.new.is_none() {
        return Err(Err(
            "Couldn't read the file on either side of the change".to_owned()
        ));
    }
    Ok(sides)
}

/// Reads `key`'s diff as `kind`, on a background thread.
pub fn load(repo: &Path, key: &DiffKey, kind: RichKind) -> Result<Rich, String> {
    let sides = match read_sides(repo, key) {
        Ok(sides) => sides,
        Err(Ok(note)) => return Ok(Rich::Plain(note)),
        Err(Err(e)) => return Err(e),
    };
    Ok(match kind {
        RichKind::Prose => Rich::Ready(RichDiff::Prose(prose::diff(&sides))),
    })
}

/// What a rich view keeps between frames.
#[derive(Default)]
pub struct RichState {
    /// Runs of unchanged parts opened (by their place in the view).
    pub opened: std::collections::HashSet<usize>,
}

/// Shows `diff`; a click on a part asks for its line in the plain diff.
pub fn show(ui: &mut Ui, diff: &RichDiff, state: &mut RichState, acts: &mut Vec<Action>) {
    match diff {
        RichDiff::Prose(prose) => prose_view::show(ui, prose, state, acts),
    }
}

/// The note above the plain diff when a rich view couldn't read the file.
pub fn fallback_note(ui: &mut Ui, kind: RichKind, note: &str) {
    let t = crate::theme::Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.warning.gamma_multiply(0.12))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                crate::icons::inline(ui, "circle-alert", 14.0, t.warning);
                ui.label(
                    egui::RichText::new(format!(
                        "Can't show this as {}: {note}. Showing the plain diff.",
                        kind.name()
                    ))
                    .color(t.text),
                );
            });
        });
}
