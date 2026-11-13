//! Line streaming through the shared process owner, including SSH/WSL supervisors.
use std::path::Path;
use std::time::Duration;

use super::command::{Extra, remote_request, repository_command};
use crate::remote;

mod queue;

const DRAIN: Duration = Duration::from_millis(300);

/// Clones request cancellation; only the process service owns termination/reaping.
#[derive(Clone, Default)]
pub struct Stopper(crate::tasks::Cancellation);
impl Stopper {
    pub fn stop(&self) {
        self.0.cancel();
    }
    pub fn stopped(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ended {
    pub code: Option<i32>,
    pub stopped: bool,
}

/// Drain callbacks on the existing caller, keeping the service event loop nonblocking.
/// The output queue, bounded by the bytes it holds, stops an operation whose consumer cannot
/// keep up; it never silently loses output or grows an unbounded pipe-reader queue.
pub fn stream(
    dir: &Path,
    args: &[&str],
    stopper: &Stopper,
    mut line: impl FnMut(String),
) -> Result<Ended, String> {
    if stopper.stopped() {
        return Ok(Ended {
            code: None,
            stopped: true,
        });
    }
    let _timer = super::backend::CliTimer::new(args);
    let (sent, received) = queue::queue(queue::LIMIT);
    let stopping = stopper.clone();
    let full = sent.clone();
    let mut options = crate::processes::Options {
        background: true,
        drain_for: DRAIN,
        cancelled: Some(Box::new(move || stopping.stopped() || full.full())),
        ..Default::default()
    };
    for is_err in [false, true] {
        let sent = sent.clone();
        let callback: crate::processes::Chunk =
            Box::new(move |bytes| sent.push(usize::from(is_err), bytes));
        if is_err {
            options.stderr = Some(callback);
        } else {
            options.stdout = Some(callback);
        }
    }
    drop(sent);
    let command = if let Some(target) = remote::parse(dir) {
        let req = remote_request(&target, &Extra::default(), args);
        options.timeout = Some(req.timeout.unwrap_or(remote::NETWORK_LIMIT));
        remote::streaming_command(&target.host, &req)?.configure(&mut options)
    } else {
        let mut cmd = repository_command(Some(dir), args);
        cmd.args(args);
        cmd
    };
    let ticket = crate::processes::service()?.submit(command, options)?;
    let mut pending = [Vec::new(), Vec::new()];
    let result = loop {
        if let Some((which, bytes)) = received.recv_timeout(Duration::from_millis(20))
            && let Err(error) = feed(&mut pending[which], &bytes, &mut line)
        {
            ticket.cancel();
            return stream_completion(
                crate::processes::checked(ticket.wait()),
                stopper.stopped(),
                Some(error),
            );
        }
        if let Some(result) = ticket.try_result() {
            break crate::processes::checked(result);
        }
    };
    let mut content_error = None;
    while let Some((which, bytes)) = received.try_recv() {
        if content_error.is_none() {
            content_error = feed(&mut pending[which], &bytes, &mut line).err();
        }
    }
    for bytes in pending {
        if !bytes.is_empty() {
            line(
                String::from_utf8_lossy(&bytes)
                    .trim_end_matches('\r')
                    .into(),
            );
        }
    }
    if received.full() {
        content_error.get_or_insert_with(|| {
            crate::i18n::text("backend-stream-consumer-exceeded-the-bounded-output-queue-101b12")
                .into()
        });
    }
    stream_completion(result, stopper.stopped(), content_error)
}
fn stream_completion(
    result: Result<crate::processes::Output, crate::processes::Failure>,
    stopped: bool,
    content_error: Option<String>,
) -> Result<Ended, String> {
    let error = |message: String| match &content_error {
        Some(content) => format!("{content}; {message}"),
        None => message,
    };
    let result = match result {
        Ok(result) => result,
        Err(crate::processes::Failure::CancelledBeforeStart) if stopped => {
            return match content_error {
                Some(content) => Err(content),
                None => Ok(Ended {
                    code: None,
                    stopped: true,
                }),
            };
        }
        Err(failure) => return Err(error(failure.message())),
    };
    if let Some(reason) = result.stopped
        && !(stopped && result.stop_kind == Some(crate::processes::StopKind::Cancelled))
    {
        return Err(error(reason));
    }
    if let Some(content) = content_error {
        return Err(content);
    }
    Ok(Ended {
        code: result.status.code(),
        stopped,
    })
}

fn feed(pending: &mut Vec<u8>, bytes: &[u8], line: &mut impl FnMut(String)) -> Result<(), String> {
    pending.extend_from_slice(bytes);
    while let Some(end) = pending.iter().position(|&b| b == b'\n') {
        let bytes: Vec<_> = pending.drain(..=end).collect();
        line(
            String::from_utf8_lossy(&bytes)
                .trim_end_matches(['\r', '\n'])
                .into(),
        );
    }
    if pending.len() > 1 << 20 {
        return Err(crate::i18n::text("backend-stream-line-exceeds-1-mib-589b21").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_content_errors_preserve_terminal_uncertainty() {
        use crate::processes::{Failure, StopKind, recording_output};
        for content in [
            "stream line exceeds 1 MiB",
            "stream consumer exceeded the bounded output queue",
        ] {
            let error = stream_completion(
                Ok(recording_output(Some(StopKind::Uncertain))),
                true,
                Some(content.into()),
            )
            .err()
            .unwrap_or_default();
            assert!(error.contains(content) && error.contains("cleanup proof missing"));
            let error = stream_completion(
                Err(Failure::Failed("could not finish owned process".into())),
                true,
                Some(content.into()),
            )
            .err()
            .unwrap_or_default();
            assert!(error.contains(content) && error.contains("could not finish owned process"));
        }
        assert!(
            stream_completion(Ok(recording_output(Some(StopKind::Cancelled))), true, None)
                .is_ok_and(|ended| ended.stopped)
        );
        assert!(
            stream_completion(Err(Failure::CancelledBeforeStart), true, None)
                .is_ok_and(|ended| ended.stopped)
        );
    }

    fn repo(name: &str) -> Result<std::path::PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-stream-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        super::super::run(Some(&dir), &["init", "-q"])?;
        Ok(dir)
    }

    /// Both outputs come through, as they come, and the exit code with them.
    #[test]
    fn lines_of_both_outputs_come_through() -> Result<(), String> {
        let dir = repo("lines")?;
        let mut lines = Vec::new();
        let ended = stream(
            &dir,
            &["-c", "alias.say=!echo one; echo two >&2; exit 3", "say"],
            &Stopper::default(),
            |l| lines.push(l),
        );
        let _ = std::fs::remove_dir_all(&dir);
        let ended = ended?;
        lines.sort();
        assert_eq!(lines, ["one", "two"]);
        assert_eq!(
            ended,
            Ended {
                code: Some(3),
                stopped: false
            }
        );
        Ok(())
    }

    /// Many short writes wait for a consumer that falls behind for a moment: none is lost,
    /// and the command's end comes with them.
    #[test]
    fn many_short_lines_wait_for_a_slow_consumer() -> Result<(), String> {
        let dir = repo("slow")?;
        let mut lines = Vec::new();
        let ended = stream(
            &dir,
            &[
                "-c",
                "alias.say=!i=0; while [ $i -lt 300 ]; do echo $i; i=$((i+1)); done; exit 3",
                "say",
            ],
            &Stopper::default(),
            |l| {
                if lines.is_empty() {
                    std::thread::sleep(Duration::from_millis(200));
                }
                lines.push(l);
            },
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(ended?.code, Some(3));
        let want: Vec<String> = (0..300).map(|i| i.to_string()).collect();
        assert_eq!(lines, want);
        Ok(())
    }

    /// A command stopped before it starts never runs.
    #[test]
    fn a_command_stopped_before_it_starts_never_runs() -> Result<(), String> {
        let dir = repo("early")?;
        let stopper = Stopper::default();
        stopper.stop();
        let mut lines = Vec::new();
        let ended = stream(&dir, &["-c", "alias.say=!echo ran", "say"], &stopper, |l| {
            lines.push(l)
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ended?.stopped);
        assert!(lines.is_empty());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_command_elsewhere_says_its_end_whatever_it_leaves_running() -> Result<(), String> {
        let dir = repo("remote-end")?;
        let path = std::path::PathBuf::from(format!("ssh://{}{}", remote::LOOPBACK, dir.display()));
        let mut lines = Vec::new();
        let started = std::time::Instant::now();
        let ended = stream(
            &path,
            &[
                "-c",
                "alias.say=!echo one; echo; echo two >&2; sleep 33 & exit 3",
                "say",
            ],
            &Stopper::default(),
            |l| lines.push(l),
        )?;
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        assert_eq!(ended.code, Some(3));
        lines.sort();
        assert_eq!(lines, ["", "one", "two"]);
        assert!(started.elapsed() < Duration::from_secs(10));
        Ok(())
    }

    /// Delivered lines remain available, but inherited-open output is an explicit
    /// incomplete-result failure even after the direct Git child has exited.
    #[cfg(unix)]
    #[test]
    fn inherited_open_output_reports_failure_and_keeps_delivered_lines() -> Result<(), String> {
        let dir = repo("background")?;
        let started = std::time::Instant::now();
        let mut lines = Vec::new();
        let ended = stream(
            &dir,
            &["-c", "alias.bg=!sleep 34 & echo started", "bg"],
            &Stopper::default(),
            |l| lines.push(l),
        );
        let took = started.elapsed();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(ended, Err(e) if e.contains("kept its output open")));
        assert_eq!(lines, ["started"]);
        assert!(took < Duration::from_secs(20), "{took:?}");
        Ok(())
    }

    /// Stopping ends git and what it runs, however deep.
    #[cfg(unix)]
    #[test]
    fn stopping_ends_the_command_and_what_it_started() -> Result<(), String> {
        let dir = repo("stop")?;
        let effect = dir.join("must-not-survive");
        let stopper = Stopper::default();
        let stopping = stopper.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            stopping.stop();
        });
        let alias = format!(
            "alias.wait=!sh -c {}",
            remote::sh_quote(&format!(
                "sleep 1; printf survived >{}",
                remote::sh_quote(&effect.to_string_lossy())
            ))
        );
        let ended = stream(&dir, &["-c", &alias, "wait"], &stopper, |_| {})?;
        canceller.join().map_err(|_| "canceller panicked")?;
        assert!(ended.stopped);
        std::thread::sleep(Duration::from_secs(1));
        assert!(!effect.exists(), "a child survived cancellation");
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        Ok(())
    }
}
