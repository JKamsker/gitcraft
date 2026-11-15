//! Read-only download of one exact GitHub Actions job's log, for evidence the job and step
//! states don't carry (test outcomes, cache hits, toolchain versions).

use super::*;

/// So much of a log is read; past it the log is cut and counts as incomplete.
const MAX_LOG_BYTES: usize = 64 * 1024 * 1024;

/// A job's log text, and whether it had to be cut at [`MAX_LOG_BYTES`].
pub fn job_log(repo: &Path, project: &Project, job: u64) -> Result<(String, bool), String> {
    if project.forge != Forge::GitHub {
        return Err("Job logs are read for GitHub Actions only.".into());
    }
    if job == 0 {
        return Err("The job has no recorded ID.".into());
    }
    // Captured up to the log bound, not the usual tool output limit: a longer log comes back
    // cut there instead of unreadable.
    let endpoint = format!("repos/{}/actions/jobs/{job}/logs", project.path);
    let (text, cut) = project
        .hosting()
        .tool_capped(repo, &["api", &endpoint], MAX_LOG_BYTES)?;
    let (text, bounded) = bounded(text);
    Ok((text, cut || bounded))
}

fn bounded(mut text: String) -> (String, bool) {
    if text.len() <= MAX_LOG_BYTES {
        return (text, false);
    }
    let mut cut = MAX_LOG_BYTES;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    (text, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_logs_are_cut_on_a_character_boundary_and_flagged() {
        assert_eq!(bounded("short".into()), ("short".into(), false));
        let mut long = "é".repeat(MAX_LOG_BYTES / 2);
        long.push('x');
        let (text, cut) = bounded(long);
        assert!(cut && text.len() <= MAX_LOG_BYTES && text.ends_with('é'));
    }
}
