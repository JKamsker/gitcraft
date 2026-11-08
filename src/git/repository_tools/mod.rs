//! Advanced repository views. All commands run on the repository machine through shared Git.
pub(crate) mod all_refs;
pub(crate) mod object;

/// Read plumbing against the selected repository, independent of inherited Git redirection.
pub(super) fn read(
    repo: &std::path::Path,
    args: &[&str],
    input: Vec<u8>,
    cap: usize,
) -> Result<Vec<u8>, String> {
    read_with(repo, args, input, cap, &[], None)
}
pub(super) fn read_with(
    repo: &std::path::Path,
    args: &[&str],
    input: Vec<u8>,
    cap: usize,
    env: &[(&str, String)],
    control: Option<std::sync::Arc<crate::git::OpControl>>,
) -> Result<Vec<u8>, String> {
    capture_with(repo, args, input, cap, env, control)?.checked(&[0])
}
#[derive(Debug)]
pub(super) struct ReadOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
impl ReadOutput {
    pub fn checked(self, accepted: &[i32]) -> Result<Vec<u8>, String> {
        if self.code.is_some_and(|c| accepted.contains(&c)) {
            return Ok(self.stdout);
        }
        let message = String::from_utf8_lossy(&self.stderr).trim().to_owned();
        Err(if message.is_empty() {
            crate::i18n::message(
                "backend-repository-metadata-command-failed-089f86",
                &[("code", format!("{:?}", self.code).into())],
            )
        } else {
            message
        })
    }
}
pub(super) fn captured(
    repo: &std::path::Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<crate::git::Captured, String> {
    let out = capture_with(
        repo,
        args,
        input.unwrap_or_default().to_vec(),
        32 << 20,
        &[],
        None,
    )?;
    Ok(crate::git::Captured {
        code: out.code,
        raw: out.stdout.clone(),
        stdout: String::from_utf8(out.stdout)
            .map_err(|_| crate::i18n::text("backend-repository-metadata-is-not-utf-8-aa6912"))?,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}
pub(super) fn text_codes(
    repo: &std::path::Path,
    args: &[&str],
    accepted: &[i32],
) -> Result<String, String> {
    String::from_utf8(capture_with(repo, args, Vec::new(), 32 << 20, &[], None)?.checked(accepted)?)
        .map_err(|_| crate::i18n::text("backend-repository-metadata-is-not-utf-8-aa6912").into())
}
pub(super) fn capture_with(
    repo: &std::path::Path,
    args: &[&str],
    input: Vec<u8>,
    cap: usize,
    env: &[(&str, String)],
    control: Option<std::sync::Arc<crate::git::OpControl>>,
) -> Result<ReadOutput, String> {
    capture_with_timeout(
        repo,
        args,
        input,
        cap,
        env,
        control,
        std::time::Duration::from_secs(120),
    )
}
pub(super) fn capture_with_timeout(
    repo: &std::path::Path,
    args: &[&str],
    input: Vec<u8>,
    cap: usize,
    env: &[(&str, String)],
    control: Option<std::sync::Arc<crate::git::OpControl>>,
    timeout: std::time::Duration,
) -> Result<ReadOutput, String> {
    use crate::git::command;
    if crate::tasks::cancellation().is_cancelled()
        || control.as_ref().is_some_and(|c| c.is_cancelled())
    {
        return Err(crate::git::CANCELLED.into());
    }
    if let Some(target) = crate::remote::parse(repo) {
        let mut request = command::bounded::request_captured(&target, args, input, cap);
        request
            .env
            .extend(env.iter().map(|(k, v)| ((*k).into(), v.clone())));
        request.timeout = Some(timeout);
        let out = match &control {
            Some(control) => crate::tasks::with_cancellation(control.cancellation(), || {
                crate::remote::exec(&target.host, &request)
            }),
            None => crate::remote::exec(&target.host, &request),
        }?;
        if out.stdout.len() > cap {
            return Err(crate::i18n::text(
                "backend-repository-metadata-exceeds-the-inspection-limit-5d3406",
            )
            .into());
        }
        if crate::tasks::cancellation().is_cancelled()
            || control.as_ref().is_some_and(|c| c.is_cancelled())
        {
            return Err(crate::git::CANCELLED.into());
        }
        return Ok(ReadOutput {
            code: Some(out.code),
            stdout: out.stdout,
            stderr: out.stderr,
        });
    }
    let mut cmd = command::local_command(Some(repo), &command::Extra::repository(), args);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = crate::processes::run(
        cmd,
        crate::processes::Options {
            cancelled: control
                .map(|c| Box::new(move || c.is_cancelled()) as Box<dyn Fn() -> bool + Send + Sync>),
            input: Some(input),
            output_limit: cap,
            stderr_limit: Some(65536),
            timeout: Some(timeout),
            background: true,
            ..Default::default()
        },
    )?
    .complete()?;
    Ok(ReadOutput {
        code: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}
pub(crate) mod dependencies;
pub(crate) mod guard;
pub(crate) mod publication;
pub(crate) mod reftable;
pub(crate) mod remote_config;
pub(crate) mod replacements;
pub(crate) mod routes;
pub(crate) mod submodule_conflict;
mod submodule_index;

pub(crate) mod sharing;

#[cfg(test)]
mod scoped_read_recording {
    use super::*;
    #[test]
    fn only_declared_git_statuses_are_accepted() {
        let output = |code| ReadOutput {
            code,
            stdout: b"selected".to_vec(),
            stderr: b"failure".to_vec(),
        };
        assert_eq!(output(Some(1)).checked(&[0, 1]), Ok(b"selected".to_vec()));
        assert_eq!(
            output(Some(129)).checked(&[0, 129]),
            Ok(b"selected".to_vec())
        );
        assert!(output(Some(128)).checked(&[0, 1]).is_err());
        assert!(output(None).checked(&[0, 1]).is_err());
    }
}

/// One analysis budget bounds total work, rather than only each subprocess.
pub(super) struct ReadBudget {
    remaining: std::cell::Cell<usize>,
    deadline: std::time::Instant,
}
impl ReadBudget {
    pub fn new(commands: usize, time: std::time::Duration) -> Self {
        Self {
            remaining: std::cell::Cell::new(commands),
            deadline: std::time::Instant::now() + time,
        }
    }
    pub fn take(&self) -> Result<std::time::Duration, String> {
        if crate::tasks::cancellation().is_cancelled() {
            return Err(crate::git::CANCELLED.into());
        }
        let left = self
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        if left.is_zero() || self.remaining.get() == 0 {
            return Err(crate::i18n::text(
                "backend-repository-analysis-reached-its-time-work-limit-narrow-the-select-a665b8",
            )
            .into());
        }
        self.remaining.set(self.remaining.get() - 1);
        Ok(left.min(std::time::Duration::from_secs(120)))
    }
    pub fn read(
        &self,
        repo: &std::path::Path,
        args: &[&str],
        control: Option<std::sync::Arc<crate::git::OpControl>>,
    ) -> Result<ReadOutput, String> {
        capture_with_timeout(repo, args, Vec::new(), 1 << 20, &[], control, self.take()?)
    }
    pub fn text(
        &self,
        repo: &std::path::Path,
        args: &[&str],
        control: Option<std::sync::Arc<crate::git::OpControl>>,
    ) -> Result<String, String> {
        String::from_utf8(self.read(repo, args, control)?.checked(&[0])?).map_err(|_| {
            crate::i18n::text("backend-repository-metadata-is-not-utf-8-aa6912").into()
        })
    }
}
#[cfg(test)]
mod budget_pure {
    use super::*;
    #[test]
    fn exhausted_work_and_elapsed_deadline_refuse_without_spawning() {
        let budget = ReadBudget::new(1, std::time::Duration::from_secs(10));
        assert!(budget.take().is_ok());
        assert!(budget.take().is_err());
        assert!(
            ReadBudget::new(5, std::time::Duration::ZERO)
                .take()
                .is_err()
        );
        let token = crate::tasks::Cancellation::default();
        token.cancel();
        crate::tasks::with_cancellation(token, || {
            assert!(crate::tasks::cancellation().is_cancelled());
            crate::tasks::with_cancellation(crate::tasks::Cancellation::default(), || {
                assert!(!crate::tasks::cancellation().is_cancelled())
            });
            assert!(crate::tasks::cancellation().is_cancelled());
        });
    }
}

#[cfg(test)]
mod held_regressions;
pub(crate) mod outbox;

pub(crate) mod subtrees;
