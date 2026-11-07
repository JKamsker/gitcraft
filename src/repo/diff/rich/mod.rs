//! Readings of a diff that understand the file: renderers shown in the diff pane in place of
//! its lines, each for the kinds of file it knows (prose; more plug in beside it). They read
//! both sides of the file on a background thread; a file they can't read (malformed, too big)
//! falls back to the plain diff with a note saying why. Staging stays in the plain diff.

pub(super) mod api;
#[cfg(test)]
mod api_tests;
mod api_view;
pub(super) mod config;
#[cfg(test)]
mod config_tests;
mod config_view;
pub(super) mod deps;
#[cfg(test)]
mod deps_tests;
mod deps_view;
pub(super) mod dvc;
mod dvc_view;
mod json;
pub(super) mod prose;
pub(super) mod prose_view;
mod tab;
pub(super) mod table;
#[cfg(test)]
mod table_tests;
pub(super) mod table_view;
#[cfg(test)]
mod tests;
mod toml;
mod tree;
mod words;
mod yaml;

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
    /// JSON, YAML and TOML compared key by key.
    Config,
    /// An OpenAPI or Swagger document's breaking changes.
    Api,
    /// A lockfile's packages upgraded, added and removed.
    Deps,
    /// CSV and TSV as a grid.
    Table,
    /// What a `.dvc` file tracks.
    Dvc,
}

impl RichKind {
    /// The readings `path` can be shown in, in the order their buttons show.
    pub fn of(path: &str) -> Vec<RichKind> {
        let ext = extension(path);
        let mut kinds = Vec::new();
        if deps::lockfile(path).is_some() {
            kinds.push(RichKind::Deps);
        }
        if table::separator(path).is_some() {
            kinds.push(RichKind::Table);
        }
        if dvc::is_dvc(path) {
            kinds.push(RichKind::Dvc);
        }
        if prose::EXTENSIONS.contains(&ext.as_str()) {
            kinds.push(RichKind::Prose);
        }
        if config::EXTENSIONS.contains(&ext.as_str()) {
            kinds.push(RichKind::Config);
        }
        // Named like an API document: its button shows; any other JSON or YAML file that is
        // one offers it from the view by key.
        let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
        if RichKind::Api.accepts(path) && (name.contains("openapi") || name.contains("swagger")) {
            kinds.push(RichKind::Api);
        }
        kinds
    }

    /// Whether `path` can be read this way.
    pub fn accepts(self, path: &str) -> bool {
        match self {
            RichKind::Api => matches!(extension(path).as_str(), "json" | "yaml" | "yml"),
            kind => RichKind::of(path).contains(&kind),
        }
    }

    /// The icon of its button in the diff's header.
    pub fn icon(self) -> &'static str {
        match self {
            RichKind::Prose => "book-open",
            RichKind::Config => "list",
            RichKind::Api => "server",
            RichKind::Deps => "package",
            RichKind::Table => "columns-2",
            RichKind::Dvc => "database",
        }
    }

    /// What its button does, while it is off (`on`: while it shows).
    pub fn tip(self, on: bool) -> &'static str {
        match (self, on) {
            (_, true) => "Show the changes as lines",
            (RichKind::Prose, false) => {
                "Show the changes as prose: wrapped paragraphs, the words that changed marked"
            }
            (RichKind::Config, false) => {
                "Show the changes by key: the keys added, removed, changed and moved"
            }
            (RichKind::Table, false) => {
                "Show the changes as a table: rows and cells added, removed and changed"
            }
            (RichKind::Dvc, false) => "Show the data this file tracks, before and after",
            (RichKind::Deps, false) => {
                "Show the dependency changes: packages upgraded, downgraded, added, removed and relicensed"
            }
            (RichKind::Api, false) => {
                "Show the API contract changes: what breaks the API's clients, and what was added"
            }
        }
    }

    /// How it shows a file, in a note: "Can't show this …".
    pub fn how(self) -> &'static str {
        match self {
            RichKind::Prose => "as prose",
            RichKind::Config => "by key",
            RichKind::Api => "as API contract changes",
            RichKind::Deps => "as dependency changes",
            RichKind::Table => "as a table",
            RichKind::Dvc => "as the data it tracks",
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
    Config(config::ConfigDiff),
    Api(api::ApiDiff),
    Deps(deps::DepsDiff),
    Table(table::TableDiff),
    Dvc(dvc::DvcDiff),
}

/// What reading the diff for a rich view gave.
pub enum Rich {
    Ready(RichDiff),
    /// It can't be read that way: the plain diff shows, with this note saying why.
    Plain(String),
}

/// Reads both sides of `key`'s file. A side that isn't there (a new or deleted file) is
/// `None`; one too big or not text makes the rich view fall back (`Err` holds the note).
pub(super) fn read_sides(
    repo: &Path,
    key: &DiffKey,
    old_path: Option<&str>,
) -> Result<Sides, Result<String, String>> {
    let (mut old, new) = sides(key);
    if let (Some(renamed), Some((_, path))) = (old_path, old.as_mut()) {
        *path = renamed.to_owned();
    }
    let read = |side: &Option<(Option<String>, String)>| -> Result<Option<String>, Result<String, String>> {
        let Some((rev, path)) = side else {
            return Ok(None);
        };
        let bytes = match git::file_at(repo, rev.as_deref(), path) {
            Ok(bytes) => bytes,
            // Not there on that side (added, deleted): only the other one shows.
            Err(e) if missing(&e) => return Ok(None),
            Err(e) => return Err(Err(e)),
        };
        if bytes.len() > MAX_SIDE_BYTES {
            return Err(Ok(format!(
                "the file is over {} MB",
                MAX_SIDE_BYTES / 1024 / 1024
            )));
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| Ok("the file isn't UTF-8 text".to_owned()))
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

/// Whether reading a side failed because the file isn't on it (git's words, or the system's
/// for a file of the working tree), rather than for a reason to report.
fn missing(error: &str) -> bool {
    let e = error.to_lowercase();
    [
        "does not exist",
        "exists on disk, but not in",
        "invalid object name",
        "unknown revision",
        "bad revision",
        "not a valid object name",
        "no such file",
        "cannot find",
        "not found",
    ]
    .iter()
    .any(|m| e.contains(m))
}

/// Reads `key`'s diff as `kind`, on a background thread.
pub fn load(
    repo: &Path,
    key: &DiffKey,
    kind: RichKind,
    old_path: Option<&str>,
) -> Result<Rich, String> {
    let sides = match read_sides(repo, key, old_path) {
        Ok(sides) => sides,
        Err(Ok(note)) => return Ok(Rich::Plain(note)),
        Err(Err(e)) => return Err(e),
    };
    Ok(match kind {
        RichKind::Prose => Rich::Ready(RichDiff::Prose(prose::diff(&sides))),
        RichKind::Config => match config::diff(key.path(), &sides) {
            Ok(diff) => Rich::Ready(RichDiff::Config(diff)),
            Err(note) => Rich::Plain(note),
        },
        RichKind::Table => match table::diff(key.path(), &sides) {
            Ok(diff) => Rich::Ready(RichDiff::Table(diff)),
            Err(note) => Rich::Plain(note),
        },
        RichKind::Dvc => match dvc::diff(&sides) {
            Ok(diff) => Rich::Ready(RichDiff::Dvc(diff)),
            Err(note) => Rich::Plain(note),
        },
        RichKind::Deps => match deps::diff(key.path(), &sides) {
            Ok(diff) => Rich::Ready(RichDiff::Deps(diff)),
            Err(note) => Rich::Plain(note),
        },
        RichKind::Api => match api::diff(key.path(), &sides) {
            Ok(diff) => Rich::Ready(RichDiff::Api(diff)),
            Err(note) => Rich::Plain(note),
        },
    })
}

/// What a rich view keeps between frames.
#[derive(Default)]
pub struct RichState {
    /// Runs of unchanged parts opened (by their place in the view).
    pub opened: std::collections::HashSet<usize>,
    /// The height of each thing shown, measured at `width`.
    pub heights: Vec<f32>,
    pub width: f32,
}

/// Shows `diff`; a click on a part asks for its line in the plain diff.
pub fn show(ui: &mut Ui, diff: &RichDiff, state: &mut RichState, acts: &mut Vec<Action>) {
    match diff {
        RichDiff::Prose(prose) => prose_view::show(ui, prose, state, acts),
        RichDiff::Config(config) => config_view::show(ui, config, acts),
        RichDiff::Api(api) => api_view::show(ui, api, acts),
        RichDiff::Deps(deps) => deps_view::show(ui, deps, acts),
        RichDiff::Table(table) => table_view::show(ui, table, state, acts),
        RichDiff::Dvc(dvc) => dvc_view::show(ui, dvc, acts),
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
                        "Can't show this {}: {note}. Showing the plain diff.",
                        kind.how()
                    ))
                    .color(t.text),
                );
            });
        });
}
