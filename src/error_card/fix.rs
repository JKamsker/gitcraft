//! What the card finds out about a problem (in the background) and the fix it offers: what it
//! changes, as the confirmation says it, how it runs and how it is undone.

use std::path::Path;
use std::time::Duration;

use super::text::{self, Place};
use crate::git::errors::GitProblem;
use crate::git::errors::fix::{self as config, ConfigEdit, Scope};
use crate::git::errors::lock::{self, LockFile};

/// What was found: facts worth showing, and the fix, if there is one to offer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub notes: Vec<Note>,
    pub fix: Option<Fix>,
}

/// A fact about the problem; `warn` when it speaks against the fix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub warn: bool,
}

impl Note {
    fn plain(text: String) -> Self {
        Self { text, warn: false }
    }

    fn warning(text: String) -> Self {
        Self { text, warn: true }
    }
}

/// A change that fixes the problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    /// A config change, to the file `file` (as its machine names it).
    Config { edit: ConfigEdit, file: String },
    /// Removing the lock seen so.
    RemoveLock(LockFile),
}

impl Fix {
    /// Its button.
    pub fn button(&self) -> &'static str {
        match self {
            Fix::RemoveLock(_) => "Remove stale lock",
            Fix::Config { edit, .. } => match edit.key {
                "safe.directory" => "Trust this folder",
                "core.longpaths" => "Enable long paths",
                "http.sslBackend" => "Use Windows' certificates",
                _ => "Fix it",
            },
        }
    }

    /// What it will do, exactly, asked before it does.
    pub fn confirm(&self) -> String {
        match self {
            Fix::Config { edit, file } if edit.add => {
                format!(
                    "Adds {} to {file}. Undo, on this card, takes it out again.",
                    edit.line()
                )
            }
            Fix::Config { edit, file } => format!(
                "Sets {} in {file} ({}). Undo, on this card, puts it back.",
                edit.line(),
                edit.previous.as_ref().map_or_else(
                    || "it isn't set now".to_owned(),
                    |v| format!("it is {v} now")
                )
            ),
            Fix::RemoveLock(seen) => format!(
                "Removes {}, last written {} ago. A lock holds nothing to keep, so there is no \
                 undo; a git still using it would fail.",
                seen.shown,
                text::age(seen.age)
            ),
        }
    }

    /// Makes the change in `place`: what was done, and the fix that undoes it, if any.
    pub fn apply(&self, place: &Path) -> Result<(String, Option<Fix>), String> {
        match self {
            Fix::Config { edit, file } => {
                config::apply(place, edit)?;
                let verb = if edit.add { "Added" } else { "Set" };
                let into = if edit.add { "to" } else { "in" };
                Ok((
                    format!("{verb} {} {into} {file}.", edit.line()),
                    Some(self.clone()),
                ))
            }
            Fix::RemoveLock(seen) => {
                lock::remove(place, seen)?;
                Ok((format!("Removed {}.", seen.shown), None))
            }
        }
    }

    /// Takes back what [`Self::apply`] did.
    pub fn undo(&self, place: &Path) -> Result<String, String> {
        match self {
            Fix::Config { edit, file } => {
                config::revert(place, edit)?;
                Ok(match (&edit.previous, edit.add) {
                    (_, true) => format!("Took {} out of {file} again.", edit.line()),
                    (Some(v), false) => format!("Set {} back to {v} in {file}.", edit.key),
                    (None, false) => format!("Unset {} in {file} again.", edit.key),
                })
            }
            Fix::RemoveLock(_) => Err("Removing a lock can't be undone".to_owned()),
        }
    }
}

/// Which fix a problem gets where git runs, before anything is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    Trust(String),
    RemoveLock(String),
    LongPaths,
    WindowsCertificates,
    /// Facts about line endings, no fix.
    LineEndings(Vec<String>),
    /// Nothing to find out or fix.
    Explain,
}

pub fn plan(problem: &GitProblem, place: Place) -> Plan {
    match problem {
        GitProblem::DubiousOwnership { path } => Plan::Trust(path.clone()),
        GitProblem::StaleLock { lock } => Plan::RemoveLock(lock.clone()),
        GitProblem::NameTooLong if place.git_for_windows() => Plan::LongPaths,
        GitProblem::Certificate { .. } if place.git_for_windows() => Plan::WindowsCertificates,
        GitProblem::LineEndings { files, .. } => Plan::LineEndings(files.clone()),
        _ => Plan::Explain,
    }
}

/// Finds out what the card shows about `problem`, for git run in `dir` (the repository, or
/// the folder it is in). Reads only: git, the lock file, the process list. Off the window's
/// thread.
pub fn gather(problem: &GitProblem, dir: &Path, place: Place) -> Result<Found, String> {
    match plan(problem, place) {
        Plan::Trust(path) => trust(dir, &path),
        Plan::RemoveLock(lock) => stale_lock(dir, &lock),
        Plan::LongPaths => set_once(dir, Scope::Repo, "core.longpaths", "true"),
        Plan::WindowsCertificates => set_once(dir, Scope::Global, "http.sslBackend", "schannel"),
        Plan::LineEndings(files) => Ok(line_endings(dir, &files)),
        Plan::Explain => Ok(Found::default()),
    }
}

fn trust(dir: &Path, path: &str) -> Result<Found, String> {
    let file = config::file_of(dir, &Scope::Global)?;
    let listed = config::get_all(dir, &Scope::Global, "safe.directory")?;
    if listed.iter().any(|p| p == path) {
        return Ok(Found {
            notes: vec![Note::warning(format!(
                "{file} lists this folder in safe.directory already, so git still refusing \
                 means the path git checks is spelled another way (a trailing slash, letter \
                 case, a network share's %(prefix)/ form): compare it with What git said."
            ))],
            fix: None,
        });
    }
    Ok(Found {
        notes: Vec::new(),
        fix: Some(Fix::Config {
            edit: ConfigEdit::add(Scope::Global, "safe.directory", path),
            file,
        }),
    })
}

/// The tab reads the repository again right after an operation fails: its own git would count
/// as "running" for that moment.
const SETTLE: Duration = Duration::from_millis(400);

fn stale_lock(dir: &Path, lock_path: &str) -> Result<Found, String> {
    let Some(seen) = lock::inspect(dir, lock_path)? else {
        return Ok(Found {
            notes: vec![Note::plain(format!(
                "{lock_path} is gone now: the git that held it is done, and nothing is left \
                 to remove."
            ))],
            fix: None,
        });
    };
    let mut notes = vec![Note::plain(format!(
        "{} was last written {} ago.",
        text::file_name(&seen.shown),
        text::age(seen.age)
    ))];
    // On another machine, the tab's own reads take a round trip or more.
    let remote = crate::remote::is_remote(dir);
    std::thread::sleep(if remote { SETTLE * 4 } else { SETTLE });
    // Only on this machine, outside Windows, are gitcraft's own git processes told apart.
    let ours_hidden = cfg!(unix) && !remote;
    notes.push(match lock::git_processes(dir) {
        Ok(running) if running.is_empty() => {
            Note::plain("No other git is running on this machine.".to_owned())
        }
        Ok(running) => Note::warning(format!(
            "git is running on this machine: {}.{} If one is working in this repository, let \
             it finish (or close the editor it opened) instead.",
            running.join("; "),
            if ours_hidden {
                ""
            } else {
                " Some may be gitcraft's own, reading the repository."
            }
        )),
        Err(e) => Note::warning(format!("Whether git is running couldn't be checked: {e}.")),
    });
    Ok(Found {
        notes,
        fix: Some(Fix::RemoveLock(seen)),
    })
}

/// A fix setting `key` to `value` in `scope`, unless it is so already.
fn set_once(dir: &Path, scope: Scope, key: &'static str, value: &str) -> Result<Found, String> {
    let edit = ConfigEdit::set(dir, scope.clone(), key, value)?;
    let file = config::file_of(dir, &scope)?;
    // Set in the repository's own config, which wins over the global one: changing that
    // would change nothing.
    if scope == Scope::Global
        && let Some(local) = config::get(dir, &Scope::Repo, key).ok().flatten()
        && local != value
    {
        let there = config::file_of(dir, &Scope::Repo)
            .unwrap_or_else(|_| "the repository's config".to_owned());
        return Ok(Found {
            notes: vec![Note::warning(format!(
                "{key} is {local} in {there}, which wins over {file}: change it there."
            ))],
            fix: None,
        });
    }
    if edit.previous.as_deref() == Some(value) {
        return Ok(Found {
            notes: vec![Note::warning(format!(
                "{key} is {value} in {file} already, so that isn't it."
            ))],
            fix: None,
        });
    }
    Ok(Found {
        notes: Vec::new(),
        fix: Some(Fix::Config { edit, file }),
    })
}

fn line_endings(dir: &Path, files: &[String]) -> Found {
    let mut notes = vec![Note::plain(autocrlf_note(dir))];
    if let Some(file) = files.first() {
        notes.push(Note::plain(attributes_note(dir, file)));
    }
    Found { notes, fix: None }
}

fn autocrlf_note(dir: &Path) -> String {
    let out = crate::git::run_ok_codes(
        Some(dir),
        &[
            "config",
            "--show-origin",
            "--null",
            "--get",
            "core.autocrlf",
        ],
        &[0, 1],
    );
    match out.as_deref().map(origin_and_value) {
        Ok(Some((origin, value))) => format!("core.autocrlf is {value} here (set in {origin})."),
        Ok(None) => "core.autocrlf isn't set here, which is false: git leaves line endings as \
                     they are unless .gitattributes says otherwise."
            .to_owned(),
        Err(e) => format!("core.autocrlf couldn't be read: {e}."),
    }
}

/// `file:<path>\0<value>\0` → (path, value).
pub fn origin_and_value(out: &str) -> Option<(String, String)> {
    let mut parts = out.split('\0');
    let origin = parts.next()?;
    let value = parts.next()?;
    let origin = origin.strip_prefix("file:").unwrap_or(origin);
    (!value.is_empty()).then(|| (origin.to_owned(), value.trim_end().to_owned()))
}

fn attributes_note(dir: &Path, file: &str) -> String {
    let out = crate::git::run(Some(dir), &["check-attr", "-z", "text", "eol", "--", file]);
    match out.map(|out| attribute_values(&out)) {
        Ok(set) if set.is_empty() => format!("No .gitattributes rule covers {file}."),
        Ok(set) => format!(".gitattributes gives {file}: {}.", set.join(", ")),
        Err(e) => format!("The .gitattributes rules for {file} couldn't be read: {e}."),
    }
}

/// `path\0attribute\0value\0…` of `check-attr -z` → the attributes set, `text=auto`, `eol=lf`.
pub fn attribute_values(out: &str) -> Vec<String> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .chunks(3)
        .filter_map(|chunk| match chunk {
            [_, name, value] if *value != "unspecified" && !name.is_empty() => Some(match *value {
                "set" => (*name).to_owned(),
                "unset" => format!("-{name}"),
                value => format!("{name}={value}"),
            }),
            _ => None,
        })
        .collect()
}
