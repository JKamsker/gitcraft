//! Running one test in one worktree, on whichever machine it is, and stopping it.
//!
//! Everything runs through git (`git -c alias.x='!…' x`), so it goes through the same command
//! layer as the rest of the app: here with git's own `sh` (Git for Windows' on Windows), on WSL
//! and SSH machines in a shell kept open there. A test runs as `git bisect run` runs it:
//! `sh -c <command>` at the top of the worktree, with output in a bounded-tail log.
//! Its actual command owner publishes a private capability record before running code.
//! Cancellation and crash recovery authenticate that owner and require a stop/reap receipt.

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

/// Runs the command with git's shell; the durable supervisor owns its whole inherited tree.
const WRAPPER: &str = r#": gitcraft-bisect-test
sh -c "$2" >"$1" 2>&1 </dev/null
r=$?
tail -c 8192 "$1" 2>/dev/null
exit $r"#;

// A cancelled queued test may never have opened its log. Only absence is empty;
// a directory, dangling link or failed read must remain a visible failure.
const STOPPED_LOG: &str = r#"cd "${1%/*}" || exit 1
log=${1##*/}
if [ -f "$log" ] && [ ! -L "$log" ]; then
  tail -c 8192 "$log"
elif [ ! -e "$log" ] && [ ! -L "$log" ]; then
  :
else
  printf 'gitcraft: stopped test log is not a regular file\n' >&2
  exit 1
fi"#;

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
    shell_input(dir, script, args, None)
}

pub(super) fn shell_input(
    dir: &Path,
    script: &str,
    args: &[&str],
    input: Option<Vec<u8>>,
) -> Result<String, String> {
    let words = sh_alias("gitcraft-bisect-sh", script, args);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let exit = git::run_for_test_input(dir, &words, SHELL_FOR, input)?;
    if exit.code == Some(0) {
        Ok(String::from_utf8_lossy(&exit.stdout).into_owned())
    } else {
        let said = String::from_utf8_lossy(&exit.stderr).trim().to_owned();
        Err(if said.is_empty() {
            format!("bisect shell ended with status {:?}", exit.code)
        } else {
            said
        })
    }
}

/// A test to run, with a unique durable owner record and log on its machine.
pub struct Test<'a> {
    pub dir: &'a Path,
    /// A folder of the same machine to stop it from (the worktree may be in use).
    pub control: &'a Path,
    pub owner_record: &'a Path,
    pub authority: &'a crate::remote::workspace_authority::WorkspaceAuthority,
    pub log: &'a str,
    pub command: &'a str,
    pub limit: Duration,
}

/// Runs `test` until it ends, it runs past its limit, or `stop()` says its answer is no longer
/// wanted; the last two are stopped there (with all they started) and say so.
pub fn run(test: &Test<'_>, stop: &(dyn Fn() -> bool + Sync)) -> Result<Ran, String> {
    let words = sh_alias("gitcraft-bisect-test", WRAPPER, &[test.log, test.command]);
    let (tx, rx) = channel();
    let cancel = crate::tasks::Cancellation::default();
    let record = test.owner_record;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let _ = tx.send(git::run_owned_test(
                test.dir,
                &words,
                &record,
                test.limit + GRACE,
                cancel.clone(),
                test.authority,
            ));
        });
        let deadline = Instant::now() + test.limit;
        let mut why: Option<Ended> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => {
                    // A pre-dispatch cancellation starts no target and creates no record.
                    if result.as_ref().is_err_and(|e| e == "cancelled") && why.is_some() {
                        return Ok(Ran {
                            ended: why.ok_or("missing stop reason")?,
                            output: String::new(),
                        });
                    }
                    let exit = result?;
                    return recovered_run(
                        git::Exit {
                            code: exit.status.code(),
                            stdout: exit.stdout,
                            stderr: exit.stderr,
                        },
                        exit.stopped.as_deref(),
                        why,
                        || crate::remote::recover_owner(record).map(|_| ()),
                        || shell(test.control, STOPPED_LOG, &[test.log]),
                    );
                }
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
            if why.is_some() {
                cancel.cancel();
            }
        }
    })
}

fn recovered_run(
    exit: git::Exit,
    stopped: Option<&str>,
    why: Option<Ended>,
    recover: impl FnOnce() -> Result<(), String>,
    read_log: impl FnOnce() -> Result<String, String>,
) -> Result<Ran, String> {
    recover().map_err(|e| format!("test cleanup unknown; workspace preserved: {e}"))?;
    if let Some(reason) = stopped
        && reason != "cancelled"
        && reason != "process timed out and was stopped"
    {
        return Err(reason.to_owned());
    }
    let effective = why.or_else(|| stopped.map(|_| Ended::TimedOut));
    let mut ran = finish(Ok(exit), effective)?;
    if effective.is_some() {
        let output = read_log().map_err(|e| format!("could not read stopped test log: {e}"))?;
        ran.output = tail(&output, KEEP);
    }
    Ok(ran)
}

/// How a run that came back ended: as it was stopped (`why`), else by its exit code.
fn finish(result: Result<git::Exit, String>, why: Option<Ended>) -> Result<Ran, String> {
    // Cleanup uncertainty is never converted into a successful stopped verdict.
    let exit = result?;
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

    #[test]
    fn recording_unknown_cleanup_is_not_a_stopped_verdict() {
        assert!(finish(Err("missing owner receipt".into()), Some(Ended::Stopped)).is_err());
        assert!(finish(Err("missing owner receipt".into()), Some(Ended::TimedOut)).is_err());
    }

    #[test]
    fn recording_stopped_log_is_read_only_after_recovery() -> Result<(), String> {
        for ended in [Ended::Stopped, Ended::TimedOut] {
            let order = std::cell::RefCell::new(Vec::new());
            let ran = recovered_run(
                git::Exit {
                    code: Some(125),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
                Some("cancelled"),
                Some(ended),
                || {
                    order.borrow_mut().push("recovered");
                    Ok(())
                },
                || {
                    order.borrow_mut().push("absent log");
                    Ok(String::new())
                },
            )?;
            assert_eq!(ran.ended, ended);
            assert!(ran.output.is_empty());
            assert_eq!(*order.borrow(), ["recovered", "absent log"]);
        }
        // These are the production shell's only empty-log admission checks.
        assert!(STOPPED_LOG.starts_with("cd \"${1%/*}\" || exit 1"));
        assert!(STOPPED_LOG.contains("[ ! -e \"$log\" ] && [ ! -L \"$log\" ]"));
        assert!(!STOPPED_LOG.contains("2>/dev/null"));
        Ok(())
    }

    #[test]
    fn recording_unknown_recovery_never_reads_or_normalizes_log() {
        let mut read = false;
        let result = recovered_run(
            git::Exit {
                code: Some(125),
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
            Some("cancelled"),
            Some(Ended::Stopped),
            || Err("missing terminal proof".into()),
            || {
                read = true;
                Ok(String::new())
            },
        );
        assert!(
            matches!(result, Err(e) if e.contains("test cleanup unknown") && e.contains("missing terminal proof"))
        );
        assert!(!read);
    }

    #[test]
    fn recording_stopped_log_errors_survive_and_available_output_is_kept() -> Result<(), String> {
        for error in ["not a regular file", "Permission denied"] {
            let result = recovered_run(
                git::Exit {
                    code: Some(125),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
                Some("cancelled"),
                Some(Ended::Stopped),
                || Ok(()),
                || Err(error.into()),
            );
            assert!(
                matches!(result, Err(e) if e.contains("could not read stopped test log") && e.contains(error))
            );
        }
        let ran = recovered_run(
            git::Exit {
                code: Some(125),
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
            Some("cancelled"),
            Some(Ended::Stopped),
            || Ok(()),
            || Ok("partial output\n".into()),
        )?;
        assert_eq!(ran.ended, Ended::Stopped);
        assert_eq!(ran.output, "partial output\n");
        Ok(())
    }

    #[test]
    fn recording_normal_completion_does_not_substitute_log_output() -> Result<(), String> {
        let mut read = false;
        let ran = recovered_run(
            git::Exit {
                code: Some(0),
                stdout: b"delivered".to_vec(),
                stderr: Vec::new(),
            },
            None,
            None,
            || Ok(()),
            || {
                read = true;
                Ok("wrong output".into())
            },
        )?;
        assert_eq!(ran.ended, Ended::Exited(0));
        assert_eq!(ran.output, "delivered");
        assert!(!read);
        Ok(())
    }
}
