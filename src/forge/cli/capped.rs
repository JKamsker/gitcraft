//! A forge tool run whose answer may outgrow the usual capture limit. Past its own limit the
//! answer is kept cut short instead of refused: the start of a job's log is still evidence,
//! as long as it is marked as only a part.

use super::*;

/// [`run`], with stdout captured up to `limit` bytes. The text and whether it was cut there.
pub(in crate::forge) fn run_capped(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
    limit: usize,
) -> Result<(String, bool), String> {
    let (cmd, start_error) = prepared(tool, noun, repo, hosting, args)?;
    #[cfg(test)]
    if let Some(answer) = recording::answer(&cmd, None) {
        return answer.map(|text| (text, false));
    }
    let options = crate::processes::Options {
        output_limit: limit,
        ..bounded_options(TOOL_TIMEOUT, None)
    };
    capped(
        tool,
        limit,
        dispatch(cmd, options, start_error, crate::processes::run_checked),
    )
}

/// A run the process service stopped because stdout filled `limit` is a cut answer; any other
/// stop (a timeout, a cancel, stderr overflowing, uncertain cleanup) stays the error it is.
fn capped(
    tool: &str,
    limit: usize,
    result: Result<crate::processes::Output, String>,
) -> Result<(String, bool), String> {
    let output = result?;
    let overflowed = output.stopped.is_some()
        && output.stop_kind == Some(crate::processes::StopKind::Failed)
        && output.stdout.len() == limit;
    if overflowed {
        return Ok((String::from_utf8_lossy(&output.stdout).into_owned(), true));
    }
    finish_run(tool, TOOL_TIMEOUT, Ok(output)).map(|text| (text, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::{StopKind, recording_output};

    fn output(stdout: &[u8], kind: Option<StopKind>) -> crate::processes::Output {
        crate::processes::Output {
            stdout: stdout.to_vec(),
            ..recording_output(kind)
        }
    }

    #[test]
    fn stdout_filling_the_limit_comes_back_cut_not_refused() {
        let full = output(&[b'x'; 8], Some(StopKind::Failed));
        assert_eq!(capped("gh", 8, Ok(full)), Ok(("x".repeat(8), true)));
        let whole = output(b"log", None);
        assert_eq!(capped("gh", 8, Ok(whole)), Ok(("log".into(), false)));
    }

    #[test]
    fn other_stops_stay_errors_even_with_a_full_buffer() {
        for kind in [StopKind::TimedOut, StopKind::Cancelled, StopKind::Uncertain] {
            assert!(capped("gh", 8, Ok(output(&[b'x'; 8], Some(kind)))).is_err());
        }
        // stderr overflowing stops the process with stdout short of the limit.
        assert!(capped("gh", 8, Ok(output(b"part", Some(StopKind::Failed)))).is_err());
        assert!(capped("gh", 8, Err("not installed".into())).is_err());
    }
}
