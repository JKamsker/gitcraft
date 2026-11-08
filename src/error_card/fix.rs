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
            Fix::RemoveLock(_) => crate::i18n::text("interaction-error-remove-lock"),
            Fix::Config { edit, .. } => match edit.key {
                "safe.directory" => crate::i18n::text("interaction-error-trust-folder"),
                "core.longpaths" => crate::i18n::text("interaction-error-long-paths"),
                "http.sslBackend" => crate::i18n::text("interaction-error-use-certificates"),
                _ => crate::i18n::text("interaction-error-fix"),
            },
        }
    }

    /// What it will do, exactly, asked before it does.
    pub fn confirm(&self) -> String {
        match self {
            Fix::Config { edit, file } if edit.add => crate::i18n::message(
                "interaction-error-confirm-add",
                &[("line", edit.line().into()), ("file", file.as_str().into())],
            ),
            Fix::Config { edit, file } => crate::i18n::message(
                "interaction-error-confirm-set",
                &[
                    ("line", edit.line().into()),
                    ("file", file.as_str().into()),
                    (
                        "previous",
                        edit.previous
                            .as_ref()
                            .map_or_else(
                                || crate::i18n::text("interaction-error-not-set").to_owned(),
                                |v| {
                                    crate::i18n::message(
                                        "interaction-error-current-value",
                                        &[("value", v.as_str().into())],
                                    )
                                },
                            )
                            .into(),
                    ),
                ],
            ),
            Fix::RemoveLock(seen) => crate::i18n::message(
                "interaction-error-confirm-remove",
                &[
                    ("lock", seen.shown.as_str().into()),
                    ("age", text::age(seen.age).into()),
                ],
            ),
        }
    }

    /// Makes the change in `place`: what was done, and the fix that undoes it, if any.
    pub fn apply(&self, place: &Path) -> Result<(String, Option<Fix>), String> {
        match self {
            Fix::Config { edit, file } => {
                config::apply(place, edit)?;
                Ok((
                    crate::i18n::message(
                        if edit.add {
                            "interaction-error-added"
                        } else {
                            "interaction-error-set"
                        },
                        &[("line", edit.line().into()), ("file", file.as_str().into())],
                    ),
                    Some(self.clone()),
                ))
            }
            Fix::RemoveLock(seen) => {
                lock::remove(place, seen)?;
                Ok((
                    crate::i18n::message(
                        "interaction-error-removed",
                        &[("lock", seen.shown.as_str().into())],
                    ),
                    None,
                ))
            }
        }
    }

    /// Takes back what [`Self::apply`] did.
    pub fn undo(&self, place: &Path) -> Result<String, String> {
        match self {
            Fix::Config { edit, file } => {
                config::revert(place, edit)?;
                Ok(match (&edit.previous, edit.add) {
                    (_, true) => crate::i18n::message(
                        "interaction-error-undo-add",
                        &[("line", edit.line().into()), ("file", file.as_str().into())],
                    ),
                    (Some(v), false) => crate::i18n::message(
                        "interaction-error-undo-value",
                        &[
                            ("key", edit.key.into()),
                            ("value", v.as_str().into()),
                            ("file", file.as_str().into()),
                        ],
                    ),
                    (None, false) => crate::i18n::message(
                        "interaction-error-undo-unset",
                        &[("key", edit.key.into()), ("file", file.as_str().into())],
                    ),
                })
            }
            Fix::RemoveLock(_) => {
                Err(crate::i18n::text("interaction-error-cannot-undo").to_owned())
            }
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
            notes: vec![Note::warning(crate::i18n::message(
                "interaction-error-already-trusted",
                &[("file", file.as_str().into())],
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
            notes: vec![Note::plain(crate::i18n::message(
                "interaction-error-lock-gone",
                &[("lock", lock_path.into())],
            ))],
            fix: None,
        });
    };
    let mut notes = vec![Note::plain(crate::i18n::message(
        "interaction-error-lock-age",
        &[
            ("lock", text::file_name(&seen.shown).into()),
            ("age", text::age(seen.age).into()),
        ],
    ))];
    // On another machine, the tab's own reads take a round trip or more.
    let remote = crate::remote::is_remote(dir);
    std::thread::sleep(if remote { SETTLE * 4 } else { SETTLE });
    // Only on this machine, outside Windows, are gitcraft's own git processes told apart.
    let ours_hidden = cfg!(unix) && !remote;
    notes.push(match lock::git_processes(dir) {
        Ok(running) if running.is_empty() => {
            Note::plain(crate::i18n::text("interaction-error-no-git").to_owned())
        }
        Ok(running) => Note::warning(crate::i18n::message(
            if ours_hidden {
                "interaction-error-running"
            } else {
                "interaction-error-running-with-ours"
            },
            &[("processes", running.join("; ").into())],
        )),
        Err(e) => Note::warning(crate::i18n::message(
            "interaction-error-process-check-failed",
            &[("error", e.as_str().into())],
        )),
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
            .unwrap_or_else(|_| crate::i18n::text("interaction-error-repo-config").to_owned());
        return Ok(Found {
            notes: vec![Note::warning(crate::i18n::message(
                "interaction-error-local-wins",
                &[
                    ("key", key.into()),
                    ("value", local.as_str().into()),
                    ("there", there.as_str().into()),
                    ("file", file.as_str().into()),
                ],
            ))],
            fix: None,
        });
    }
    if edit.previous.as_deref() == Some(value) {
        return Ok(Found {
            notes: vec![Note::warning(crate::i18n::message(
                "interaction-error-already-set",
                &[
                    ("key", key.into()),
                    ("value", value.into()),
                    ("file", file.as_str().into()),
                ],
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
        Ok(Some((origin, value))) => crate::i18n::message(
            "interaction-error-autocrlf-value",
            &[
                ("value", value.as_str().into()),
                ("origin", origin.as_str().into()),
            ],
        ),
        Ok(None) => crate::i18n::text("interaction-error-autocrlf-unset").to_owned(),
        Err(e) => crate::i18n::message("interaction-error-autocrlf-error", &[("error", e.into())]),
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
        Ok(set) if set.is_empty() => {
            crate::i18n::message("interaction-error-no-attributes", &[("file", file.into())])
        }
        Ok(set) => crate::i18n::message(
            "interaction-error-attributes-value",
            &[("file", file.into()), ("attributes", set.join(", ").into())],
        ),
        Err(e) => crate::i18n::message(
            "interaction-error-attributes-error",
            &[("file", file.into()), ("error", e.as_str().into())],
        ),
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
