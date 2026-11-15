//! Run the lane's command and retain bounded diagnostics even when streaming fails.
use super::KEPT_LINES;
use crate::git::{self, Stopper};
use std::{collections::VecDeque, path::Path};

/// The alias git runs the user's command with, at the top of the worktree, in its own `sh`
/// (so none is needed on PATH, on Windows either). What git sets for an alias run in a linked
/// worktree (`GIT_DIR`…) is unset: a test suite that makes repositories of its own would
/// otherwise have them all be this one.
pub fn command_alias(command: &str) -> String {
    format!(
        "alias.gitcraft-test=!unset GIT_DIR GIT_WORK_TREE GIT_PREFIX GIT_INDEX_FILE \
         GIT_CONFIG_PARAMETERS GIT_OPTIONAL_LOCKS; {command}"
    )
}

/// Runs `command` in the worktree `dir` until it ends or `stopper` stops it: its exit code
/// (`None`: a signal ended it) and its last [`KEPT_LINES`] lines, or why it couldn't run.
pub fn run_command(
    dir: &Path,
    command: &str,
    stopper: &Stopper,
) -> Result<(git::Ended, Vec<String>), String> {
    let (ended, lines) = run_command_timed(dir, command, stopper, None);
    Ok((ended?, lines))
}

pub(in crate::git) fn run_command_timed(
    dir: &Path,
    command: &str,
    stopper: &Stopper,
    timeout: Option<std::time::Duration>,
) -> (Result<git::Ended, String>, Vec<String>) {
    let alias = command_alias(command);
    collect(|line| {
        git::stream_timed(
            dir,
            &["-c", &alias, "gitcraft-test"],
            stopper,
            timeout,
            line,
        )
    })
}

fn collect(
    run: impl FnOnce(&mut dyn FnMut(String)) -> Result<git::Ended, String>,
) -> (Result<git::Ended, String>, Vec<String>) {
    let mut lines = VecDeque::new();
    let ended = run(&mut |line| {
        if lines.len() >= KEPT_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    });
    (ended, lines.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_failure_retains_last_bounded_lines() {
        let (ended, lines) = collect(|line| {
            for n in 0..500 {
                line(format!("diagnostic {n}"));
            }
            Err("command timed out".into())
        });
        assert_eq!(ended, Err("command timed out".into()));
        assert_eq!(lines.len(), KEPT_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("diagnostic 300"));
        assert_eq!(lines.last().map(String::as_str), Some("diagnostic 499"));
    }
}
