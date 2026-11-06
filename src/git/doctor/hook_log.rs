//! What git printed while running commands that run hooks (commit, merge, push…), kept in
//! memory for the doctor's Hooks section: a hook's output is otherwise lost once the command
//! succeeds, since only failures are shown. Nothing is written to disk.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Most runs kept per repository, and in all.
const KEPT: usize = 12;
const KEPT_ALL: usize = 60;
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
    let text = clean(&String::from_utf8_lossy(stderr));
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let output = tail(text, MAX_OUTPUT);
    if let Ok(mut runs) = RUNS.lock() {
        // The oldest of this repository's, so one busy repository doesn't push out the
        // others'; and the oldest of all past the overall limit.
        if runs.iter().filter(|r| r.repo == dir).count() >= KEPT
            && let Some(at) = runs.iter().position(|r| r.repo == dir)
        {
            runs.remove(at);
        }
        if runs.len() >= KEPT_ALL {
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

/// `text` as it is shown: colour codes taken out, and the password or token of any
/// `scheme://user:secret@host` hidden (a hook may echo the remote's URL as configured).
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        // CSI sequences: ESC [ … final byte.
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    hide_credentials(&out)
}

/// `scheme://user:secret@` → `scheme://user:***@`, `scheme://token@` → `scheme://***@`.
fn hide_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (before, after) = rest.split_at(at + 3);
        out.push_str(before);
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '"' | '\'' | '<' | '>'))
            .unwrap_or(after.len());
        let authority = &after[..end];
        match authority.rfind('@') {
            Some(at) => {
                let user = &authority[..at];
                match user.split_once(':') {
                    Some((name, _)) => out.push_str(&format!("{name}:***")),
                    None => out.push_str("***"),
                }
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
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
    fn colours_and_secrets_are_taken_out() {
        assert_eq!(clean("\x1b[31merror\x1b[0m: x"), "error: x");
        assert_eq!(
            clean("To https://ana:s3cret@github.com/o/r.git\n"),
            "To https://ana:***@github.com/o/r.git\n"
        );
        assert_eq!(
            clean("remote https://ghp_token@host/r and ssh://git@host:22/r"),
            "remote https://***@host/r and ssh://***@host:22/r"
        );
        assert_eq!(clean("no url here"), "no url here");
        assert_eq!(clean("://@"), "://***@");
    }

    #[test]
    fn one_repository_does_not_push_out_the_others() {
        let busy = PathBuf::from(format!("/hook-log-busy-{}", std::process::id()));
        let quiet = PathBuf::from(format!("/hook-log-quiet-{}", std::process::id()));
        record(Some(&quiet), &["commit"], true, b"kept");
        for i in 0..(KEPT + 5) {
            record(Some(&busy), &["push"], true, format!("run {i}").as_bytes());
        }
        assert_eq!(runs(&busy).len(), KEPT);
        assert_eq!(runs(&busy)[0].output, format!("run {}", KEPT + 4));
        assert_eq!(runs(&quiet).len(), 1);
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
