//! What git printed while running commands that run hooks (commit, merge, push…), kept in
//! memory for the doctor's Hooks section: a hook's output is otherwise lost once the command
//! succeeds, since only failures are shown. Nothing is written to disk.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Most runs kept, in all repositories together.
const KEPT: usize = 40;
/// Most bytes of output kept per run (the end of it, where a hook's verdict is).
const MAX_OUTPUT: usize = 16 * 1024;

/// One run of a command that runs hooks.
#[derive(Clone, Debug)]
pub struct Run {
    pub repo: PathBuf,
    /// The git subcommand (`commit`, `push`…).
    pub command: String,
    pub when: SystemTime,
    pub ok: bool,
    /// What it printed on its error output (where hooks print), trimmed; cut to its end.
    pub output: String,
}

static RUNS: Mutex<VecDeque<Run>> = Mutex::new(VecDeque::new());

/// The subcommands that run hooks of the repository (client-side hooks).
pub const HOOK_COMMANDS: [&str; 11] = [
    "commit",
    "merge",
    "pull",
    "am",
    "rebase",
    "cherry-pick",
    "revert",
    "push",
    "checkout",
    "switch",
    "worktree",
];

/// Records a git run in `dir` with `args`, when its subcommand runs hooks and it printed
/// anything on its error output.
pub(crate) fn record(dir: Option<&Path>, args: &[&str], ok: bool, stderr: &[u8]) {
    let (Some(dir), Some(command)) = (dir, subcommand(args)) else {
        return;
    };
    if !HOOK_COMMANDS.contains(&command) {
        return;
    }
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let output = tail(text, MAX_OUTPUT);
    if let Ok(mut runs) = RUNS.lock() {
        if runs.len() == KEPT {
            runs.pop_front();
        }
        runs.push_back(Run {
            repo: dir.to_path_buf(),
            command: command.to_owned(),
            when: SystemTime::now(),
            ok,
            output,
        });
    }
}

/// The runs recorded for the repository at `repo`, newest first.
pub fn runs(repo: &Path) -> Vec<Run> {
    RUNS.lock()
        .map(|runs| {
            runs.iter()
                .rev()
                .filter(|r| r.repo == repo)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// The subcommand of `args`, past the options before it (`-c x=y`, `-C dir`…).
fn subcommand<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut rest = args;
    loop {
        match rest {
            ["-c" | "-C", _, tail @ ..] => rest = tail,
            [word, tail @ ..] if word.starts_with('-') => rest = tail,
            [word, ..] => return Some(word),
            [] => return None,
        }
    }
}

/// The last `max` bytes of `text`, on a character boundary, marked as cut.
fn tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &text[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_commands_that_run_hooks_and_print_are_kept() {
        let repo = PathBuf::from(format!("/hook-log-test-{}", std::process::id()));
        record(Some(&repo), &["status"], true, b"noise");
        record(
            Some(&repo),
            &["-c", "x=y", "commit", "-m", "m"],
            true,
            b"  ",
        );
        record(
            Some(&repo),
            &["-c", "x=y", "commit", "-m", "m"],
            false,
            b"pre-commit: lint failed\n",
        );
        record(None, &["push"], true, b"x");
        let runs = runs(&repo);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].command, "commit");
        assert!(!runs[0].ok);
        assert_eq!(runs[0].output, "pre-commit: lint failed");
    }

    #[test]
    fn long_output_keeps_its_end() {
        let text = format!("{}é end", "x".repeat(MAX_OUTPUT));
        let cut = tail(&text, MAX_OUTPUT);
        assert!(cut.starts_with('…') && cut.ends_with("é end"));
        assert!(cut.len() <= MAX_OUTPUT + '…'.len_utf8());
        assert_eq!(tail("short", 10), "short");
        assert_eq!(subcommand(&["--literal-pathspecs", "push"]), Some("push"));
        assert_eq!(subcommand(&["-C", "/r"]), None);
    }
}
