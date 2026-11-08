//! git calls that read an input and may not take longer than a limit: a hook gitcraft runs on
//! its own (`prepare-commit-msg`), or a filter a message goes through (`interpret-trailers`).
//! Here or on the repository's machine, as every other call.

use std::path::Path;
use std::time::Duration;

#[cfg(test)]
use super::command::git_cmd;
use super::command::{Extra, remote_request, repository_command};
use crate::remote;

/// Runs git with `args` in `dir`, `input` on its standard input, and stops it (with what it
/// started) once `limit` has passed. Returns stdout, or what went wrong in words.
pub fn run_timed(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    limit: Duration,
) -> Result<String, String> {
    let _timer = super::backend::CliTimer::new(args);
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
            Some(code) => crate::i18n::message(
                "backend-what-failed-exit-status-code-4558d1",
                &[
                    ("what", (what).to_string().into()),
                    ("code", (code).to_string().into()),
                ],
            ),
            None => crate::i18n::message(
                "backend-what-was-stopped-by-a-signal-d75f6c",
                &[("what", (what).to_string().into())],
            ),
        }
    } else {
        said
    })
}

fn stopped(what: &str, limit: Duration) -> String {
    crate::i18n::message(
        "backend-what-didn-t-finish-within-seconds-and-was-stopped-ffb894",
        &[
            ("what", (what).to_string().into()),
            ("seconds", (limit.as_secs_f32().ceil()).into()),
        ],
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
    let mut cmd = repository_command(Some(dir), args);
    cmd.args(args);
    let out = crate::processes::run(
        cmd,
        crate::processes::Options {
            input: input.map(<[u8]>::to_vec),
            timeout: Some(limit),
            drain_for: Duration::from_millis(500),
            ..Default::default()
        },
    )
    .map_err(|reason| {
        if reason.contains("timed out") {
            stopped(what, limit)
        } else {
            reason
        }
    })?;
    if let Some(reason) = out.stopped
        && !reason.contains("kept its output open")
    {
        return Err(if reason.contains("timed out") {
            stopped(what, limit)
        } else {
            reason
        });
    }
    Ok((out.status.code(), out.stdout, out.stderr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

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

    /// What a call left running in the background, holding its output open, isn't waited for
    /// past the limit.
    #[cfg(unix)]
    #[test]
    fn what_is_left_running_is_not_waited_for() -> Result<(), String> {
        let dir = scratch("background");
        git_cmd()
            .args(["init", "-q"])
            .arg(&dir)
            .status()
            .map_err(|e| e.to_string())?;
        let started = Instant::now();
        let out = run_timed(
            &dir,
            &["-c", "alias.bg=!sleep 30 & echo started", "bg"],
            None,
            Duration::from_millis(500),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(out.is_ok(), "{out:?}");
        Ok(())
    }
}
