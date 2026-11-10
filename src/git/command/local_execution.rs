//! Local backend dispatch, command construction, and per-call environment application.

use super::*;

/// Git's answer to a read it needn't be run for: one kept (see [`memo`]) or made in-process
/// from the repository's files (see `backend::answer`). Only on this machine, for a call with
/// nothing on top.
pub(super) fn answered(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Option<Ran> {
    if !extra.plain() || dir.is_some_and(|d| remote::parse(d).is_some()) {
        return None;
    }
    if let Some(ran) = memo::get(dir, args) {
        return Some(ran);
    }
    let (code, stdout, stderr) = super::super::backend::answer(dir?, args)?;
    if extra.output_limit.is_some_and(|limit| stdout.len() > limit) {
        return None;
    }
    Some(Ran {
        code: Some(code),
        status: exit_text(code),
        stdout,
        stderr,
        failed_hook: None,
    })
}

/// Runs git with `args` in `dir` (on whichever machine it is), `extra` before them; a read it
/// needn't run for is answered without it (see [`answered`]).
pub(super) fn execute(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    if let Some(ran) = answered(dir, extra, args) {
        return Ok(ran);
    }
    let result = execute_git(dir, extra, args);
    if let Ok(ran) = &result
        && extra.plain()
        && dir.is_none_or(|d| remote::parse(d).is_none())
    {
        memo::keep(dir, args, ran);
    }
    result
}

/// Launch the installed Git executable for commands not handled by native readers.
pub(super) fn execute_git(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    let _timer = super::super::backend::CliTimer::new(args);
    let mut extra = extra.clone();
    let _source = source::bind(&mut extra, dir, args)?;
    let extra = &extra;
    if let Some(target) = dir.and_then(remote::parse) {
        let result = execute_there(&target, extra, args);
        if let Ok(out) = &result {
            source::finish(_source.as_ref(), args, out.code == Some(0))?;
            super::super::doctor::hook_log::record(dir, args, out.code == Some(0), &out.stderr);
        }
        return result;
    }
    let mut extra = extra.clone();
    let trace = extra
        .trace_hooks
        .then(super::super::hooks::TraceFile::new)
        .flatten();
    if let Some(trace) = &trace {
        extra.env.push((
            super::super::hooks::TRACE_VAR,
            trace.path().to_string_lossy().into_owned(),
        ));
    }
    let cmd = local_command(dir, &extra, args);
    let out = crate::processes::run(
        cmd,
        crate::processes::Options {
            input: extra.stdin.clone(),
            stdout_file: extra
                .stdout_file
                .as_ref()
                .map(std::fs::File::create)
                .transpose()
                .map_err(|e| {
                    crate::i18n::message(
                        "backend-could-not-save-output-e-d0d53d",
                        &[("detail", (e).to_string().into())],
                    )
                })?,
            output_limit: extra.output_limit.unwrap_or(64 << 20),
            background: extra.long || extra.tool || extra.test.is_some() || goes_over_network(args),
            timeout: extra.timeout,
            ..Default::default()
        },
    )?
    .complete()?;
    source::finish(_source.as_ref(), args, out.status.success())?;
    super::super::doctor::hook_log::record(dir, args, out.status.success(), &out.stderr);
    let failed = !out.status.success();
    Ok(Ran {
        code: out.status.code(),
        status: out.status.to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        failed_hook: trace.filter(|_| failed).and_then(|t| t.failed_hook(args)),
    })
}

/// git with `args` in `dir` on this machine, `extra` before them.
pub(in crate::git) fn local_command(dir: Option<&Path>, extra: &Extra, args: &[&str]) -> Command {
    apply_command_extras(repository_command(dir, args), dir, extra, args)
}

/// Apply the actual production argv/environment policy to an already selected command.
/// Recording callers can provide a fixed executable without PATH discovery.
pub(in crate::git) fn apply_command_extras(
    mut cmd: Command,
    dir: Option<&Path>,
    extra: &Extra,
    args: &[&str],
) -> Command {
    for name in &extra.unset {
        cmd.env_remove(name);
    }
    if extra.normalize_repository && !dir.is_some_and(crate::practice::is_repository) {
        normalize_connection_command(&mut cmd, args);
    }
    cmd.args(&extra.pre).args(&extra.owned_pre);
    for (name, value) in &extra.env {
        cmd.env(name, value);
    }
    if extra
        .owned_pre
        .iter()
        .any(|arg| arg == "gitcraft-credential-transport")
    {
        let selected = cmd.get_program().to_owned();
        cmd.env("GITCRAFT_GIT_ORIGINAL", selected);
    }
    cmd.args(args);
    cmd
}
