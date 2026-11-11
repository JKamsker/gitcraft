//! One operation owns preparation, step settlement and the inverse-plan witnesses.
use super::*;
use crate::repo::forks::guard::{CheckoutReplay, Pushed, Rewrite, Scope};

#[path = "executor/preparation.rs"]
mod preparation;
#[path = "executor/settlement.rs"]
mod settlement;
#[path = "executor/steps.rs"]
mod steps;

struct Execution<'a> {
    path: &'a Path,
    ops: &'a [Vec<String>],
    creds: Option<&'a git::Credentials>,
    quiet: bool,
    control: Option<&'a Arc<git::OpControl>>,
    fork_scope: Option<&'a Scope>,
    checkout: Option<CheckoutReplay>,
    head_before: Option<String>,
    stash_before: Option<Option<String>>,
    guarded_fork: bool,
    owned_refs: Vec<(String, String)>,
    rewrite: Option<Rewrite>,
    pushed: Vec<Pushed>,
    tried: Vec<String>,
}

struct StepFailure {
    error: String,
    completed: bool,
}
impl From<String> for StepFailure {
    fn from(error: String) -> Self {
        Self {
            error,
            completed: false,
        }
    }
}

/// [`run_steps_counted`] without where it stopped.
#[cfg(test)]
pub(in crate::repo) fn run_steps(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
) -> (Result<String, String>, OpKind) {
    let (result, kind, _) = run_steps_counted(path, ops, kind, creds, false, None);
    (result, kind)
}

/// Native test harnesses use the same guarded worker as the application.
#[cfg(test)]
pub(crate) fn run_live_checkout(
    path: &Path,
    ops: &[Vec<String>],
    creds: Option<&git::Credentials>,
) -> Result<String, String> {
    run_steps(path, ops, OpKind::plain(), creds).0
}

/// Original source indices include expectations and best-effort steps. A failed
/// post-command guard advances past the completed command before credential retry.
pub(in crate::repo) fn run_steps_counted(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
) -> (Result<String, String>, OpKind, usize) {
    run_steps_guarded(path, ops, kind, creds, quiet, control, None)
}

pub(in crate::repo) fn run_steps_guarded(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
    fork_scope: Option<&Scope>,
) -> (Result<String, String>, OpKind, usize) {
    let mut execution =
        match Execution::prepare(path, ops, &kind, creds, quiet, control, fork_scope) {
            Ok(execution) => execution,
            Err(error) => return (Err(error), kind, 0),
        };
    let (result, stopped) = execution.run_plan();
    let kind = execution.finish(kind, result.is_ok());
    (result, kind, stopped)
}

impl Execution<'_> {
    fn run_plan(&mut self) -> (Result<String, String>, usize) {
        let plan = model::Plan::decode(self.ops);
        let mut result = Ok(String::new());
        for step in plan.steps() {
            #[cfg(test)]
            if !is_expectation(step.argv) {
                crate::repo::forks::before_step(step.argv);
            }
            match self.step(step) {
                Ok(Some(output)) => result = Ok(output),
                Ok(None) => {}
                Err(failure) => {
                    return (
                        Err(failure.error),
                        step.index + usize::from(failure.completed),
                    );
                }
            }
        }
        if !self.tried.is_empty() {
            result = Ok(format!("{TRIED}{}", self.tried.join("; ")));
        }
        (result, self.ops.len())
    }
}
