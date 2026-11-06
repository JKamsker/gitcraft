//! A repository's own custom commands, committed in [`FILE`]. Anyone who can push to the
//! repository writes them, so none is offered until the user has read the file and trusted it,
//! and again whenever it changes: what was trusted is the file's exact text.

use serde::{Deserialize, Serialize};

use super::CustomCommand;

/// Where a repository keeps its commands, relative to its folder.
pub const FILE: &str = ".gitcraft/commands.json";

/// A larger file isn't read: no list of commands is that long.
pub const MAX_BYTES: usize = 256 * 1024;

/// A repository's file as trusted: which repository (its path as gitcraft opens it) and the
/// file's text when the user trusted it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrustedFile {
    pub repo: String,
    pub text: String,
}

/// What the file says: `{ "commands": [ … ] }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contents {
    commands: Vec<CustomCommand>,
}

/// The repository's file as read: its text, and the commands in it or why there are none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoFile {
    pub text: String,
    pub commands: Result<Vec<CustomCommand>, String>,
}

impl RepoFile {
    pub fn new(text: String) -> Self {
        let commands = parse_repo_file(&text);
        Self { text, commands }
    }
}

/// The commands in a repository's file, or what is wrong with it.
pub fn parse_repo_file(text: &str) -> Result<Vec<CustomCommand>, String> {
    if text.len() > MAX_BYTES {
        return Err(format!(
            "{FILE} is larger than {} KiB, so it isn't read",
            MAX_BYTES / 1024
        ));
    }
    let contents: Contents =
        serde_json::from_str(text).map_err(|e| format!("{FILE} can't be read: {e}"))?;
    for (i, command) in contents.commands.iter().enumerate() {
        if let Some(problem) = command.problem() {
            let name = command.name.trim();
            let which = if name.is_empty() {
                format!("Command {}", i + 1)
            } else {
                format!("“{name}”")
            };
            return Err(format!("{FILE}: {which}: {problem}"));
        }
    }
    Ok(contents.commands)
}

/// Whether the file of `repo` reading `text` is the one the user trusted there.
pub fn is_trusted(trusted: &[TrustedFile], repo: &str, text: &str) -> bool {
    trusted.iter().any(|t| t.repo == repo && t.text == text)
}

/// `trusted` with `repo`'s file trusted as `text`, in place of what was trusted there before.
pub fn trust(trusted: &mut Vec<TrustedFile>, repo: &str, text: &str) {
    trusted.retain(|t| t.repo != repo);
    trusted.push(TrustedFile {
        repo: repo.to_owned(),
        text: text.to_owned(),
    });
}
