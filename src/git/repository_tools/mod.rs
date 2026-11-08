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
            format!("Repository metadata command failed ({:?})", self.code)
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
        stdout: String::from_utf8(out.stdout).map_err(|_| "Repository metadata is not UTF-8")?,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}
pub(super) fn text_codes(
    repo: &std::path::Path,
    args: &[&str],
    accepted: &[i32],
) -> Result<String, String> {
    String::from_utf8(capture_with(repo, args, Vec::new(), 32 << 20, &[], None)?.checked(accepted)?)
        .map_err(|_| "Repository metadata is not UTF-8".into())
}
pub(super) fn capture_with(
    repo: &std::path::Path,
    args: &[&str],
    input: Vec<u8>,
    cap: usize,
    env: &[(&str, String)],
    control: Option<std::sync::Arc<crate::git::OpControl>>,
) -> Result<ReadOutput, String> {
    use crate::git::command;
    if control.as_ref().is_some_and(|c| c.is_cancelled()) {
        return Err(crate::git::CANCELLED.into());
    }
    if let Some(target) = crate::remote::parse(repo) {
        let mut request = command::bounded::request_captured(&target, args, input, cap);
        request
            .env
            .extend(env.iter().map(|(k, v)| ((*k).into(), v.clone())));
        let out = crate::remote::exec(&target.host, &request)?;
        if out.stdout.len() > cap {
            return Err("Repository metadata exceeds the inspection limit".into());
        }
        if control.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err(crate::git::CANCELLED.into());
        }
        return Ok(ReadOutput {
            code: Some(out.code),
            stdout: out.stdout,
            stderr: out.stderr,
        });
    }
    let mut cmd = command::local_command(Some(repo), &command::Extra::default(), args);
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
            timeout: Some(std::time::Duration::from_secs(120)),
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
        assert_eq!(output(Some(1)).checked(&[0, 1]).unwrap(), b"selected");
        assert_eq!(output(Some(129)).checked(&[0, 129]).unwrap(), b"selected");
        assert!(output(Some(128)).checked(&[0, 1]).is_err());
        assert!(output(None).checked(&[0, 1]).is_err());
    }
}
