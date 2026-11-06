//! Which AI tools are installed on a repository's machine: here, in WSL or over SSH. Asked
//! there, the way the tool itself would be run (see [`super::run`]).

use std::path::Path;
use std::time::Duration;

use super::run::{self, Cancel, CommandLine, Job};
use super::settings::AiTool;

/// How long the look may take (`gh copilot --help` starts a program).
const LOOK_FOR: Duration = Duration::from_secs(30);

/// What `sh` runs to look: a word for each tool found.
const SH: &str = "\
command -v claude >/dev/null 2>&1 && echo claude
command -v codex >/dev/null 2>&1 && echo codex
command -v gh >/dev/null 2>&1 && gh copilot --help >/dev/null 2>&1 </dev/null && echo copilot
true";

/// The same for `cmd` (Windows).
const CMD: &str = "(where claude >nul 2>nul && echo claude) & \
(where codex >nul 2>nul && echo codex) & \
(where gh >nul 2>nul && gh copilot --help >nul 2>nul <nul && echo copilot) & ver >nul";

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
    let look = CommandLine::Script {
        sh: SH.to_owned(),
        cmd: CMD.to_owned(),
    };
    let out = run::run(&Job {
        repo,
        command: &look,
        input: &[],
        timeout: LOOK_FOR,
        cancel: &Cancel::new(),
    })
    .map_err(|e| format!("Couldn't look for AI tools: {}", e.message()))?;
    Ok(parse(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_tools_are_read() {
        assert_eq!(
            parse("copilot\r\nclaude\n"),
            [AiTool::Claude, AiTool::Copilot]
        );
        assert!(parse("").is_empty());
        assert!(parse("claudex codexy").is_empty());
    }

    /// On a machine whose PATH has a fake `claude` and `gh` (with copilot) but no `codex`, the
    /// look finds those two.
    #[cfg(unix)]
    #[test]
    fn the_tools_on_the_path_are_found() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gitcraft-ai-detect-{}", std::process::id()));
        let bin = dir.join("bin");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        for (name, body) in [
            ("claude", "exit 0"),
            // `gh copilot --help` works.
            ("gh", "[ \"$1\" = copilot ] && exit 0; exit 1"),
        ] {
            let path = bin.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        // Only that folder and the system's, so a real codex on this machine isn't found.
        let look = CommandLine::Script {
            sh: format!("PATH='{}:/usr/bin:/bin'\n{SH}", bin.display()),
            cmd: String::new(),
        };
        let out = run::run(&Job {
            repo: &dir,
            command: &look,
            input: &[],
            timeout: LOOK_FOR,
            cancel: &Cancel::new(),
        });
        let _ = std::fs::remove_dir_all(&dir);
        let found = parse(&out.map_err(|e| e.message())?);
        assert_eq!(found, [AiTool::Claude, AiTool::Copilot]);
        Ok(())
    }

    #[test]
    fn looking_in_a_folder_that_isnt_there_says_so() {
        let found = installed(Path::new("/nonexistent/gitcraft-ai-detect"));
        assert!(
            found
                .as_ref()
                .is_err_and(|e| e.starts_with("Couldn't look for AI tools")),
            "{found:?}"
        );
    }
}
