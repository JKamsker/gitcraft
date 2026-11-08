//! What the AI features are set to: the tool, the prompts and how much of a diff is sent.

use serde::{Deserialize, Serialize};

/// The command-line tool that writes commit messages and reviews. Off until the user picks one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AiTool {
    #[default]
    Off,
    /// Claude Code: `claude -p`.
    Claude,
    /// OpenAI's Codex CLI: `codex exec -`.
    Codex,
    /// GitHub Copilot through the GitHub CLI: `gh copilot -p … -s`.
    Copilot,
    /// A command line of the user's (`ollama run llama3`, `llm -m gpt-4o`…).
    Custom,
}

impl AiTool {
    /// The tools there are to pick, in the order they are offered.
    pub const CHOICES: [AiTool; 4] = [
        AiTool::Claude,
        AiTool::Codex,
        AiTool::Copilot,
        AiTool::Custom,
    ];

    /// How it is offered in a list.
    pub fn label(self) -> &'static str {
        match self {
            AiTool::Off => crate::i18n::text("interaction-tools-off"),
            AiTool::Claude => "Claude Code (claude -p)",
            AiTool::Codex => "Codex (codex exec)",
            AiTool::Copilot => "GitHub Copilot (gh copilot)",
            AiTool::Custom => crate::i18n::text("interaction-tools-custom"),
        }
    }

    /// How a message names it: "Writing a message with claude…".
    pub fn short(self) -> &'static str {
        match self {
            AiTool::Off => crate::i18n::text("interaction-tools-no-tool"),
            AiTool::Claude => "claude",
            AiTool::Codex => "codex",
            AiTool::Copilot => "gh copilot",
            AiTool::Custom => crate::i18n::text("interaction-tools-your-command"),
        }
    }
}

/// The prompt a commit message is asked for with. `{conventions}`, `{left_out}` and `{diff}`
/// are filled in (see [`super::prompt::fill`]).
pub const DEFAULT_COMMIT_PROMPT: &str = "\
Write a git commit message for the staged changes below.
{conventions}
Answer with the commit message only: a summary line of at most 72 characters, then, if the \
change needs explaining, a blank line and a short body wrapped at 72 characters. Say what \
changed and why, not how. No code fences, no quotes, no commentary before or after it.

{left_out}
Staged changes:
{diff}";

/// The prompt a branch is reviewed with. `{branch}`, `{base}`, `{commits}`, `{left_out}` and
/// `{diff}` are filled in.
pub const DEFAULT_REVIEW_PROMPT: &str = "\
Review the changes branch {branch} adds to {base}, as a careful reviewer of a pull request \
would. Point out bugs, risky changes, missing tests and unclear code, most important first, \
each with the file (and the line, where you can) it is about. Say briefly what looks good too. \
Answer in Markdown.

Commits:
{commits}

{left_out}
Changes:
{diff}";

/// The most of a diff sent by default, in KB.
pub const DEFAULT_MAX_DIFF_KB: u32 = 64;
/// The range the setting takes, in KB.
pub const MAX_DIFF_KB_RANGE: std::ops::RangeInclusive<u32> = 4..=1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub tool: AiTool,
    /// What [`AiTool::Custom`] runs, as `sh` reads it (`cmd` on Windows); the prompt comes on
    /// its input.
    pub custom_command: String,
    pub commit_prompt: String,
    pub review_prompt: String,
    /// The most of a diff sent, in KB; the rest is listed as left out.
    pub max_diff_kb: u32,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            tool: AiTool::Off,
            custom_command: String::new(),
            commit_prompt: DEFAULT_COMMIT_PROMPT.to_owned(),
            review_prompt: DEFAULT_REVIEW_PROMPT.to_owned(),
            max_diff_kb: DEFAULT_MAX_DIFF_KB,
        }
    }
}

impl AiSettings {
    /// Whether a tool is picked (and, for a custom one, given a command).
    pub fn enabled(&self) -> bool {
        match self.tool {
            AiTool::Off => false,
            AiTool::Custom => !self.custom_command.trim().is_empty(),
            _ => true,
        }
    }

    /// The most of a diff sent, in bytes, within the range the setting takes.
    pub fn max_diff_bytes(&self) -> usize {
        let kb = self
            .max_diff_kb
            .clamp(*MAX_DIFF_KB_RANGE.start(), *MAX_DIFF_KB_RANGE.end());
        kb as usize * 1024
    }

    /// The commit prompt, the default one when the setting was emptied.
    pub fn commit_template(&self) -> &str {
        non_empty_or(&self.commit_prompt, DEFAULT_COMMIT_PROMPT)
    }

    /// The review prompt, the default one when the setting was emptied.
    pub fn review_template(&self) -> &str {
        non_empty_or(&self.review_prompt, DEFAULT_REVIEW_PROMPT)
    }
}

fn non_empty_or<'a>(text: &'a str, default: &'a str) -> &'a str {
    if text.trim().is_empty() {
        default
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_off_until_a_tool_is_picked() {
        let mut s = AiSettings::default();
        assert!(!s.enabled());
        s.tool = AiTool::Claude;
        assert!(s.enabled());
        // A custom tool needs its command.
        s.tool = AiTool::Custom;
        s.custom_command = "   ".to_owned();
        assert!(!s.enabled());
        s.custom_command = "ollama run llama3".to_owned();
        assert!(s.enabled());
    }

    #[test]
    fn the_diff_size_stays_in_range() {
        let size = |kb| {
            AiSettings {
                max_diff_kb: kb,
                ..AiSettings::default()
            }
            .max_diff_bytes()
        };
        assert_eq!(size(0), 4 * 1024);
        assert_eq!(size(64), 64 * 1024);
        assert_eq!(size(u32::MAX), 1024 * 1024);
    }

    #[test]
    fn an_emptied_prompt_is_the_default_one() {
        let s = AiSettings {
            commit_prompt: " \n".to_owned(),
            review_prompt: "Mine {diff}".to_owned(),
            ..AiSettings::default()
        };
        assert_eq!(s.commit_template(), DEFAULT_COMMIT_PROMPT);
        assert_eq!(s.review_template(), "Mine {diff}");
    }

    /// Settings saved before this existed, or with fields missing, still load.
    #[test]
    fn old_settings_load() -> Result<(), String> {
        let s: AiSettings =
            serde_json::from_str(r#"{"tool":"Codex"}"#).map_err(|e| e.to_string())?;
        assert_eq!(s.tool, AiTool::Codex);
        assert_eq!(s.max_diff_kb, DEFAULT_MAX_DIFF_KB);
        assert_eq!(s.commit_prompt, DEFAULT_COMMIT_PROMPT);
        Ok(())
    }
}
