//! Which AI tools are installed on a repository's machine: here, in WSL or over SSH. Asked
//! there, the way the tool itself would be run (see [`super::run`]).
//!
//! The look only finds programs; it runs none of them. `gh copilot --help` used to stand in for
//! "Copilot is there", but since gh 2.86 `copilot` is a command of gh's own: gh prints its help
//! whether the Copilot CLI is installed or not, gh 2.104 starts a telemetry process for each
//! call, and the first real `gh copilot` downloads the Copilot CLI (in CI) or asks to (on a
//! terminal). So Copilot counts as installed when `gh` is on the PATH and the Copilot CLI is
//! where `gh copilot` looks for it: `copilot` on the PATH, or the copy gh downloads into its
//! data folder (`$XDG_DATA_HOME/gh`, else `%LOCALAPPDATA%\GitHub CLI` on Windows and
//! `~/.local/share/gh` elsewhere), as `copilot/copilot`.

use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use super::run::{self, Cancel, CommandLine, Job};
use super::settings::AiTool;

/// How long the look may take. It only searches the PATH and checks two files, so this is far
/// more than it needs.
const LOOK_FOR: Duration = Duration::from_secs(30);

/// What `sh` runs to look: a word for each tool found.
///
/// Copilot: this assumes gh 2.86 or newer, whose built-in `copilot` command runs the Copilot
/// CLI. An older gh beside a `copilot` from npm is taken for Copilot, and `gh copilot -p … -s`
/// then fails with gh's "unknown command"; finding that out would mean running gh.
const SH: &str = "\
command -v claude >/dev/null 2>&1 && echo claude
command -v codex >/dev/null 2>&1 && echo codex
command -v gh >/dev/null 2>&1 && { command -v copilot >/dev/null 2>&1 || \
{ c=\"${XDG_DATA_HOME:-$HOME/.local/share}/gh/copilot/copilot\"; [ -f \"$c\" ] && [ -x \"$c\" ]; }; \
} && echo copilot
true";

/// The same for `cmd` (Windows), with the same assumption about gh's version.
/// `$PATH:`: only the PATH, never the folder it runs in. `if not exist "…\"`: a file, not a
/// folder of that name.
const CMD: &str = "(where $PATH:claude >nul 2>nul && echo claude) & \
(where $PATH:codex >nul 2>nul && echo codex) & \
(where $PATH:gh >nul 2>nul && (where $PATH:copilot >nul 2>nul && echo copilot || \
if defined XDG_DATA_HOME (if exist \"%XDG_DATA_HOME%\\gh\\copilot\\copilot.exe\" \
if not exist \"%XDG_DATA_HOME%\\gh\\copilot\\copilot.exe\\\" echo copilot) \
else if exist \"%LOCALAPPDATA%\\GitHub CLI\\copilot\\copilot.exe\" \
if not exist \"%LOCALAPPDATA%\\GitHub CLI\\copilot\\copilot.exe\\\" echo copilot)) & ver >nul";

/// Where looking for the tools of a machine stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Look {
    #[default]
    NotYet,
    Looking,
    Found(Vec<AiTool>),
    Failed(String),
}

impl Look {
    /// Whether `tool` was found; `None` while that isn't known. A custom command always is.
    pub fn has(&self, tool: AiTool) -> Option<bool> {
        match self {
            _ if tool == AiTool::Custom => Some(true),
            Look::Found(found) => Some(found.contains(&tool)),
            _ => None,
        }
    }
}

/// The tools named in what the look printed, in the order they are offered.
pub fn parse(out: &str) -> Vec<AiTool> {
    let words: Vec<&str> = out.split_whitespace().collect();
    AiTool::CHOICES
        .into_iter()
        .filter(|tool| {
            let word = match tool {
                AiTool::Claude => "claude",
                AiTool::Codex => "codex",
                AiTool::Copilot => "copilot",
                AiTool::Off | AiTool::Custom => return false,
            };
            words.contains(&word)
        })
        .collect()
}

/// The tools installed on the machine `repo` is on (a custom command is never looked for: it
/// is whatever the user wrote). Blocking: from a background thread.
pub fn installed(repo: &Path) -> Result<Vec<AiTool>, String> {
    look(&look_from(repo), &look_script())
}

/// The look, for each shell.
fn look_script() -> CommandLine {
    CommandLine::Script {
        sh: SH.to_owned(),
        cmd: CMD.to_owned(),
    }
}

/// Where the look runs for `repo`: on another machine, in its folder there; here, in a folder
/// of no repository's, so nothing of the repository's is taken for a tool.
fn look_from(repo: &Path) -> Cow<'_, Path> {
    if crate::remote::is_remote(repo) {
        Cow::Borrowed(repo)
    } else {
        Cow::Owned(std::env::temp_dir())
    }
}

/// Runs `look` from `folder` (here, or on the machine it names) and reads what it found.
fn look(folder: &Path, look: &CommandLine) -> Result<Vec<AiTool>, String> {
    let out = run::run(&Job {
        repo: folder,
        command: look,
        input: &[],
        timeout: LOOK_FOR,
        cancel: &Cancel::new(),
    })
    .map_err(|e| format!("Couldn't look for AI tools: {}", e.message()))?;
    Ok(parse(&out))
}

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;
