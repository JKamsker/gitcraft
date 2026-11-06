//! git calls that read an input and may not take longer than a limit: a hook gitcraft runs on
//! its own (`prepare-commit-msg`), or a filter a message goes through (`interpret-trailers`).
//! Here or on the repository's machine, as every other call.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use super::command::{Extra, git_cmd, remote_request, with_profile};
use crate::remote;

/// How often a call running here is looked at while it is waited for.
const POLL: Duration = Duration::from_millis(15);

/// Runs git with `args` in `dir`, `input` on its standard input, and stops it (with what it
/// started) once `limit` has passed. Returns stdout, or what went wrong in words.
pub fn run_timed(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    limit: Duration,
) -> Result<String, String> {
    let what = format!("git {}", args.first().copied().unwrap_or_default());
    let (code, stdout, stderr) = match remote::parse(dir) {
        Some(target) => {
            let mut req = remote_request(&target, &Extra::default(), args);
            req.stdin = input.map(<[u8]>::to_vec);
            req.timeout = Some(limit);
            let out = remote::exec(&target.host, &req).map_err(|e| {
                if e.contains("didn't finish within") {
                    stopped(&what, limit)
                } else {
                    e
                }
            })?;
            (Some(out.code), out.stdout, out.stderr)
        }
        None => run_here(dir, args, input, limit, &what)?,
    };
    if code == Some(0) {
        return Ok(String::from_utf8_lossy(&stdout).into_owned());
    }
    let said = String::from_utf8_lossy(&stderr).trim().to_owned();
    let said = if said.is_empty() {
        String::from_utf8_lossy(&stdout).trim().to_owned()
    } else {
        said
    };
    Err(if said.is_empty() {
        match code {
            Some(code) => format!("{what} failed (exit status {code})"),
            None => format!("{what} was stopped by a signal"),
        }
    } else {
        said
    })
}

fn stopped(what: &str, limit: Duration) -> String {
    format!(
        "{what} didn't finish within {} seconds and was stopped",
        limit.as_secs_f32().ceil()
    )
}

type Finished = (Option<i32>, Vec<u8>, Vec<u8>);

fn run_here(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    limit: Duration,
    what: &str,
) -> Result<Finished, String> {
    let mut cmd = git_cmd();
    with_profile(&mut cmd, args);
    cmd.arg("-C").arg(dir).args(args);
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    // Its own process group, so that what a hook started is stopped along with it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn().map_err(|e| format!("could not run git: {e}"))?;
    if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
        let input = input.to_vec();
        // On a thread: a big input would otherwise wait on a full pipe nobody reads yet.
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                stop(&mut child);
                return Err(stopped(what, limit));
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(e) => return Err(format!("could not wait for git: {e}")),
        }
    };
    let collect = |h: std::thread::JoinHandle<Vec<u8>>| h.join().unwrap_or_default();
    Ok((status.code(), collect(stdout), collect(stderr)))
}

/// Reads all of `pipe` on a thread of its own.
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut out);
        }
        out
    })
}

/// Stops `child` and everything it started: its process group on Unix, its process tree on
/// Windows.
fn stop(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stderr(Stdio::null())
        .status();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-timed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn reads_its_input() -> Result<(), String> {
        let dir = scratch("input");
        let big = "line é\n".repeat(200_000);
        let out = run_timed(
            &dir,
            &["stripspace"],
            Some(big.as_bytes()),
            Duration::from_secs(30),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(out?, big);
        Ok(())
    }

    #[test]
    fn says_why_it_failed() {
        let dir = scratch("fails");
        let err = run_timed(&dir, &["no-such-command"], None, Duration::from_secs(30));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            err.as_ref().is_err_and(|e| e.contains("no-such-command")),
            "{err:?}"
        );
    }

    /// A call past its limit is stopped, and says so.
    #[cfg(unix)]
    #[test]
    fn stops_what_takes_too_long() -> Result<(), String> {
        let dir = scratch("slow");
        git_cmd()
            .args(["init", "-q"])
            .arg(&dir)
            .status()
            .map_err(|e| e.to_string())?;
        let started = Instant::now();
        let err = run_timed(
            &dir,
            &["-c", "alias.slow=!sleep 30", "slow"],
            None,
            Duration::from_millis(300),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(
            err.as_ref().is_err_and(|e| e.contains("didn't finish")),
            "{err:?}"
        );
        Ok(())
    }
}
