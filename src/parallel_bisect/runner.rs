//! Running one test in one worktree, on whichever machine it is, and stopping it.
//!
//! Everything runs through git (`git -c alias.x='!…' x`), so it goes through the same command
//! layer as the rest of the app: here with git's own `sh` (Git for Windows' on Windows), on WSL
//! and SSH machines in a shell kept open there. A test runs as `git bisect run` runs it:
//! `sh -c <command>` at the top of the worktree, its output to a log file. Its `sh` writes its
//! process to a file first, so that it can be stopped, with everything it started, from
//! another call (on a timeout, a cancel, or by the next bisect after a crash).

use std::path::Path;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use super::verdict::Ended;
use crate::git;
use crate::remote::sh_quote;

/// How much of the end of a test's output is kept, to show.
const KEEP: usize = 8 * 1024;

/// How long past its limit the machine itself lets a test run, should stopping it fail.
const GRACE: Duration = Duration::from_secs(120);

/// How often a test that is to stop is told again until it has.
const STOP_EVERY: Duration = Duration::from_secs(2);

/// What runs a test (`$1` its process file, `$2` its log, `$3` the command): it says its
/// process, runs the command as `git bisect run` would, then prints the end of the log.
const WRAPPER: &str = r#": gitcraft-bisect-test
printf '%s\n' "$$" >"$1"
sh -c "$3" >"$2" 2>&1 </dev/null
r=$?
rm -f "$1"
tail -c 8192 "$2" 2>/dev/null
exit $r"#;

/// Stops the test whose process file is `$1`, and everything it started: only a process that
/// is that test (its command line names the file), never one that took over its number. `-ww`:
/// macOS cuts a command line to the terminal's width otherwise, the file's name with it.
/// Where git's `sh` is Git for Windows', through Windows itself (`taskkill /T`).
pub const STOP_SCRIPT: &str = r#"f=$1
p=$(cat "$f" 2>/dev/null) || exit 0
case $p in ''|*[!0-9]*) exit 0;; esac
if [ -r "/proc/$p/cmdline" ]; then
  tr '\000' ' ' <"/proc/$p/cmdline" | grep -qF -- "$f" || exit 0
else
  { ps -ww -o args= -p "$p" 2>/dev/null || ps -o args= -p "$p" 2>/dev/null; } | grep -qF -- "$f" || exit 0
fi
if [ -r "/proc/$p/winpid" ]; then
  MSYS2_ARG_CONV_EXCL='*' taskkill /T /F /PID "$(cat "/proc/$p/winpid")" >/dev/null 2>&1
else
  all=$(ps -A -o pid= -o ppid= 2>/dev/null | awk -v root="$p" '
    { up[$1] = $2 }
    END { print root; seen[root] = 1; more = 1
      while (more) { more = 0
        for (q in up) if (!(q in seen) && (up[q] in seen)) { seen[q] = 1; print q; more = 1 } } }')
  [ -n "$all" ] || all=$p
  kill -TERM $all 2>/dev/null
  sleep 1
  kill -KILL $all 2>/dev/null
fi
rm -f "$f"
true"#;

/// What a test run came to: how it ended and the end of its output.
#[derive(Clone, Debug)]
pub struct Ran {
    pub ended: Ended,
    pub output: String,
}

/// The arguments that have git run `script` (with `args` as `$1`…) with `sh`, under the
/// alias `name`, at the top of the worktree it is run in.
pub fn sh_alias(name: &str, script: &str, args: &[&str]) -> Vec<String> {
    let mut line = format!("exec sh -c {} {name}", sh_quote(script));
    for arg in args {
        line.push(' ');
        line.push_str(&sh_quote(arg));
    }
    vec![
        "-c".to_owned(),
        format!("alias.{name}=!{line}"),
        name.to_owned(),
    ]
}

/// How long stopping a test, or clearing a folder of worktrees, may take.
const SHELL_FOR: Duration = Duration::from_secs(30 * 60);

/// Runs `script` with `sh` in `dir` (on its machine) and waits for it; its output, or why it
/// failed. Elsewhere in a test's shell, so that it never waits behind the tab's reads.
pub fn shell(dir: &Path, script: &str, args: &[&str]) -> Result<String, String> {
    let words = sh_alias("gitcraft-bisect-sh", script, args);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let exit = git::run_for_test(dir, &words, SHELL_FOR)?;
    if exit.code == Some(0) {
        Ok(String::from_utf8_lossy(&exit.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&exit.stderr).trim().to_owned())
    }
}

/// A test to run: in the worktree `dir`, its process file and log `pid_file` and `log` (paths
/// on its machine).
pub struct Test<'a> {
    pub dir: &'a Path,
    /// A folder of the same machine to stop it from (the worktree may be in use).
    pub control: &'a Path,
    pub pid_file: &'a str,
    pub log: &'a str,
    pub command: &'a str,
    pub limit: Duration,
}

/// Runs `test` until it ends, it runs past its limit, or `stop()` says its answer is no longer
/// wanted; the last two are stopped there (with all they started) and say so.
pub fn run(test: &Test<'_>, stop: &(dyn Fn() -> bool + Sync)) -> Result<Ran, String> {
    let words = sh_alias(
        "gitcraft-bisect-test",
        WRAPPER,
        &[test.pid_file, test.log, test.command],
    );
    let (tx, rx) = channel();
    std::thread::scope(|scope| {
        scope.spawn(crate::connections::inherit(|| {
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let _ = tx.send(git::run_for_test(test.dir, &words, test.limit + GRACE));
        }));
        let deadline = Instant::now() + test.limit;
        let mut why: Option<Ended> = None;
        let mut told: Option<Instant> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => return finish(result, why),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("the test ended without a result".to_owned());
                }
            }
            if why.is_none() {
                if stop() {
                    why = Some(Ended::Stopped);
                } else if Instant::now() >= deadline {
                    why = Some(Ended::TimedOut);
                }
            }
            if why.is_some() && told.is_none_or(|at| at.elapsed() >= STOP_EVERY) {
                told = Some(Instant::now());
                let _ = shell(test.control, STOP_SCRIPT, &[test.pid_file]);
            }
        }
    })
}

/// How a run that came back ended: as it was stopped (`why`), else by its exit code.
fn finish(result: Result<git::Exit, String>, why: Option<Ended>) -> Result<Ran, String> {
    let exit = match (result, why) {
        (Ok(exit), _) => exit,
        // Stopped: the connection may have gone with it.
        (Err(_), Some(why)) => {
            return Ok(Ran {
                ended: why,
                output: String::new(),
            });
        }
        (Err(e), None) => return Err(e),
    };
    let mut output = String::from_utf8_lossy(&exit.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&exit.stderr);
    if !stderr.trim().is_empty() {
        output.push_str(&stderr);
    }
    let output = tail(&output, KEEP);
    let ended = match (why, exit.code) {
        (Some(why), _) => why,
        (None, Some(code)) => Ended::Exited(code),
        (None, None) => return Err(format!("the test was ended by a signal\n{output}")),
    };
    Ok(Ran { ended, output })
}

/// The last `keep` bytes of `s`, from a character boundary.
fn tail(s: &str, keep: usize) -> String {
    let from = s.len().saturating_sub(keep);
    let from = (from..=s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());
    s[from..].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alias_quotes_its_script_and_arguments() {
        let words = sh_alias("gitcraft-x", "echo \"$1\"", &["it's", "a b"]);
        assert_eq!(words[0], "-c");
        assert_eq!(
            words[1],
            "alias.gitcraft-x=!exec sh -c 'echo \"$1\"' gitcraft-x 'it'\\''s' 'a b'"
        );
        assert_eq!(words[2], "gitcraft-x");
    }

    #[test]
    fn the_tail_starts_on_a_character() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("ab", 3), "ab");
        // "é" is two bytes: the cut moves past it rather than into it.
        assert_eq!(tail("aébc", 4), "ébc");
        assert_eq!(tail("aébc", 3), "bc");
    }
}
