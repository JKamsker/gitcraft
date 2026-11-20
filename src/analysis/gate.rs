//! Provider-independent quality-gate verdicts. The rule is conservative: an observation is
//! shown as passed only when its analysis is proven to be of the commit the user has open.

use super::Target;

/// The gate status as the server reported it, normalized across API generations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServerGate {
    Ok,
    Error,
    /// SonarQube before 7.6 could report an orange warning; it is not a pass.
    Warn,
    /// No quality gate is associated with the analysis.
    None,
    /// Absent or unrecognized.
    Unknown,
}

/// The analysis the gate belongs to, as far as the server identifies it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Analysis {
    /// The server's analysis key, when the gate was read through it.
    pub key: Option<String>,
    /// When it ran, as the server wrote it (ISO 8601 with offset).
    pub date: Option<String>,
    /// The analyzed commit (`sonar.scm.revision`), when the server records one.
    pub revision: Option<String>,
}

/// What a provider observed for one [`super::Query`], before it is compared with the checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Observation {
    pub target: Target,
    pub gate: ServerGate,
    /// `None`: the target was never analyzed (or its analyses were purged).
    pub analysis: Option<Analysis>,
    /// A background task for the target is queued or running.
    pub pending: bool,
    /// The newest background task for the target failed, so the shown analysis is older.
    pub last_task_failed: bool,
    /// Metric keys of the failing conditions, in server order.
    pub failing: Vec<String>,
    /// The dashboard of the target on the server.
    pub dashboard: String,
}

/// Why an observation cannot be trusted for the commit that is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Stale {
    /// The server records no commit for the analysis, so it cannot be matched.
    RevisionUnknown,
    /// The checkout has no commit to compare with (unborn branch, not loaded yet).
    HeadUnknown,
    /// The analysis is of another commit than the one open.
    Mismatch { analyzed: String, open: String },
    /// The newest analysis attempt failed; the result shown is from an earlier one.
    TaskFailed,
}

/// Why there is no gate to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    NotAnalyzed,
    NoGate,
    Unrecognized,
}

/// What the badge says. There is deliberately no way to reach [`Verdict::Passed`] without a
/// verified analysis revision equal to the open commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Passed,
    Failed,
    Pending,
    Unavailable(Unavailable),
    /// The server's gate is shown only as context: it is not about the open commit.
    Stale(Stale, ServerGate),
}

/// Compares an observation with the open commit `open` (a full object id).
pub(crate) fn evaluate(observation: &Observation, open: Option<&str>) -> Verdict {
    let Some(analysis) = &observation.analysis else {
        return if observation.pending {
            Verdict::Pending
        } else {
            Verdict::Unavailable(Unavailable::NotAnalyzed)
        };
    };
    if observation.pending {
        return Verdict::Pending;
    }
    match observation.gate {
        ServerGate::None => return Verdict::Unavailable(Unavailable::NoGate),
        ServerGate::Unknown => return Verdict::Unavailable(Unavailable::Unrecognized),
        ServerGate::Ok | ServerGate::Error | ServerGate::Warn => {}
    }
    let stale = |why| Verdict::Stale(why, observation.gate);
    let Some(revision) = analysis.revision.as_deref().filter(|r| is_object_id(r)) else {
        return stale(Stale::RevisionUnknown);
    };
    let Some(open) = open.filter(|o| is_object_id(o)) else {
        return stale(Stale::HeadUnknown);
    };
    if !revision.eq_ignore_ascii_case(open) {
        return stale(Stale::Mismatch {
            analyzed: revision.to_ascii_lowercase(),
            open: open.to_ascii_lowercase(),
        });
    }
    if observation.last_task_failed {
        return stale(Stale::TaskFailed);
    }
    match observation.gate {
        ServerGate::Ok => Verdict::Passed,
        _ => Verdict::Failed,
    }
}

/// A full SHA-1 or SHA-256 object id; abbreviated or decorated values never match.
fn is_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
