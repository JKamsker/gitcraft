//! Agent tasks: a worktree on a branch of its own, made for a coding agent to work in, and the
//! agent command line it runs. The repository keeps the list, so every tab of it shows them.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::sh;

/// The file of the repository's git folder the tasks are listed in.
pub(crate) const FILE: &str = "agents.json";

/// An agent command line gitcraft knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentCli {
    pub label: &'static str,
    pub program: &'static str,
    /// How a first prompt is handed to it on the command line (`None`: it takes none and
    /// stays open, so the prompt is left out).
    pub prompt_flag: Option<&'static str>,
}

/// The agents offered, the most used first. `prompt_flag` `""`: the prompt is the first word.
pub(crate) const KNOWN: [AgentCli; 5] = [
    AgentCli {
        label: "Claude Code",
        program: "claude",
        prompt_flag: Some(""),
    },
    AgentCli {
        label: "Codex",
        program: "codex",
        prompt_flag: Some(""),
    },
    AgentCli {
        label: "Gemini CLI",
        program: "gemini",
        prompt_flag: Some("-i"),
    },
    AgentCli {
        label: "Cursor Agent",
        program: "cursor-agent",
        prompt_flag: Some(""),
    },
    AgentCli {
        label: "Aider",
        program: "aider",
        prompt_flag: None,
    },
];

impl AgentCli {
    /// The shell command that starts it with `prompt` as its first message (when it takes one).
    pub fn command(&self, prompt: &str) -> String {
        let prompt = prompt.trim();
        match self.prompt_flag {
            Some(flag) if !prompt.is_empty() => {
                let flag = if flag.is_empty() {
                    String::new()
                } else {
                    format!("{flag} ")
                };
                format!("{} {flag}{}", self.program, sh::quote(prompt))
            }
            _ => self.program.to_owned(),
        }
    }

    /// The known agent a command line starts, if it starts one.
    pub fn of(command: &str) -> Option<&'static AgentCli> {
        let program = command.split_whitespace().next()?;
        KNOWN.iter().find(|a| a.program == program)
    }
}

/// What a command line is called: the known agent's name, else its first word.
pub(crate) fn agent_label(command: &str) -> String {
    match AgentCli::of(command) {
        Some(agent) => agent.label.to_owned(),
        None => command
            .split_whitespace()
            .next()
            .unwrap_or("agent")
            .to_owned(),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentTask {
    pub branch: String,
    /// The worktree's folder, in the form the app gives paths (`ssh://host/…` elsewhere).
    pub path: String,
    /// The command line that starts the agent, without the first prompt.
    pub agent: String,
    /// The first message it was given, if any.
    pub prompt: String,
    /// The commit the branch was made from.
    pub base: String,
    /// When it was started, in seconds since 1970.
    pub started: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct TaskList {
    tasks: Vec<AgentTask>,
}

/// The repository's agent tasks, oldest first.
pub(crate) fn load(repo: &Path) -> Result<Vec<AgentTask>, String> {
    super::read_json::<TaskList>(repo, FILE).map(|list| list.tasks)
}

/// Adds `task` to the repository's list (in place of one in the same folder).
pub(crate) fn add(repo: &Path, task: AgentTask) -> Result<Vec<AgentTask>, String> {
    let mut tasks = load(repo)?;
    tasks.retain(|t| t.path != task.path);
    tasks.push(task);
    save(repo, tasks)
}

/// Takes the task of the worktree at `path` off the list (the worktree stays).
pub(crate) fn remove(repo: &Path, path: &str) -> Result<Vec<AgentTask>, String> {
    let mut tasks = load(repo)?;
    tasks.retain(|t| t.path != path);
    save(repo, tasks)
}

fn save(repo: &Path, tasks: Vec<AgentTask>) -> Result<Vec<AgentTask>, String> {
    let list = TaskList { tasks };
    super::write_json(repo, FILE, &list)?;
    Ok(list.tasks)
}

/// A branch name for a task described as `text`: `agent/` and its words joined by dashes,
/// only letters, digits and dashes, at most 40 characters of it.
pub(crate) fn branch_for(text: &str) -> String {
    let mut slug = String::new();
    for c in text.trim().chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.chars().count() >= 40 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "agent/task".to_owned()
    } else {
        format!("agent/{slug}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_goes_to_the_agent_quoted() {
        let claude = &KNOWN[0];
        assert_eq!(claude.command(""), "claude");
        assert_eq!(
            claude.command(" fix the 'login' bug "),
            r"claude 'fix the '\''login'\'' bug'"
        );
        let gemini = &KNOWN[2];
        assert_eq!(gemini.command("hi"), "gemini -i 'hi'");
        let aider = &KNOWN[4];
        assert_eq!(aider.command("ignored"), "aider");
    }

    #[test]
    fn a_command_line_is_named_after_its_agent() {
        assert_eq!(agent_label("codex --full-auto"), "Codex");
        assert_eq!(agent_label("my-agent --x"), "my-agent");
        assert_eq!(agent_label(""), "agent");
    }

    #[test]
    fn branch_names_come_from_the_task() {
        assert_eq!(branch_for("Fix the login bug!"), "agent/fix-the-login-bug");
        assert_eq!(branch_for("  Größe ändern  "), "agent/größe-ändern");
        assert_eq!(branch_for("***"), "agent/task");
        assert_eq!(branch_for(&"x".repeat(100)).len(), "agent/".len() + 40);
    }
}
