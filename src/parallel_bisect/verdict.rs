//! What a test run says about a commit, read from its exit code the way `git bisect run` reads
//! it: 0 good, 125 skip, 1–127 otherwise bad, 128 and more (a signal, a crash) stops the bisect.

/// What one test said about a commit (or about every commit with its tree).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verdict {
    Good,
    Bad,
    /// It exited with 125: this commit can't be tested.
    Skip,
    /// It ran longer than the per-test limit and was stopped: counted as a skip.
    TimedOut,
}

impl Verdict {
    /// Whether the search learns nothing from it (the commit stays a candidate).
    pub fn is_skip(self) -> bool {
        matches!(self, Verdict::Skip | Verdict::TimedOut)
    }

    /// "good", "bad", "skipped", "timed out": as a row's tooltip says it.
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Good => "good",
            Verdict::Bad => "bad",
            Verdict::Skip => "skipped",
            Verdict::TimedOut => "timed out",
        }
    }
}

/// How a test run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// It exited with this code.
    Exited(i32),
    /// It was stopped at the per-test limit.
    TimedOut,
    /// It was stopped because its answer was no longer needed, or the bisect was cancelled.
    Stopped,
}

/// What an exit code says, as `git bisect run` reads it; `Err` with the reason for one that
/// stops the bisect (128 and more: killed by a signal, or a crash `git bisect run` won't read).
pub fn interpret(code: i32) -> Result<Verdict, String> {
    match code {
        0 => Ok(Verdict::Good),
        125 => Ok(Verdict::Skip),
        1..=127 => Ok(Verdict::Bad),
        _ => Err(format!(
            "the test ended with exit code {code}; 128 and more (a crash or a signal) stops a \
             bisect, as with git bisect run"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_read_as_git_bisect_run_reads_them() {
        assert_eq!(interpret(0), Ok(Verdict::Good));
        for bad in [1, 2, 100, 124, 126, 127] {
            assert_eq!(interpret(bad), Ok(Verdict::Bad), "{bad}");
        }
        assert_eq!(interpret(125), Ok(Verdict::Skip));
        for stops in [128, 130, 137, 139, 255, -1, 256] {
            assert!(interpret(stops).is_err(), "{stops}");
        }
    }

    #[test]
    fn skips_and_timeouts_teach_nothing() {
        assert!(Verdict::Skip.is_skip());
        assert!(Verdict::TimedOut.is_skip());
        assert!(!Verdict::Good.is_skip());
        assert!(!Verdict::Bad.is_skip());
    }
}
