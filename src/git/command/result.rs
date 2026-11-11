//! Recorded outcomes, hook failures, and command result formatting.

use super::*;

/// What a call is run with on top of [`GIT_ENV`]: nobody asked to sign in when `quiet`,
/// `creds` given to git when there are some.
pub(super) fn extra_for(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
    quiet: bool,
) -> Result<Extra, String> {
    let mut extra = if quiet {
        unattended_extra(cfg!(unix) || dir.is_some_and(remote::is_remote))
    } else {
        Extra::default()
    };
    if let Some(c) = creds {
        extra.normalize_repository = true;
        let policy = credential_command_policy(dir, c, args)?;
        extra.owned_pre.extend(policy.arguments);
        extra.env.extend(policy.environment);
    }
    Ok(extra)
}

pub(super) fn run_inner_extra(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    quiet: bool,
    timeout: Option<std::time::Duration>,
) -> Result<String, String> {
    let mut extra = extra_for(dir, args, creds, quiet)?;
    extra.timeout = timeout;
    finish(execute(dir, &extra, args)?, args, ok)
}

pub(super) fn run_extra(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    mut extra: Extra,
) -> Result<String, String> {
    let credentials = extra_for(dir, args, creds, false)?;
    extra.pre.extend(credentials.pre);
    extra.owned_pre.extend(credentials.owned_pre);
    extra.normalize_repository |= credentials.normalize_repository;
    extra.env.extend(credentials.env);
    finish(execute(dir, &extra, args)?, args, ok)
}

/// The error of a git call that failed: what it said, or how it ended when it said nothing.
pub(super) fn failure_message(args: &[&str], status: &str, stdout: &str, stderr: &str) -> String {
    // Merges and pulls report conflicts on stdout and the summary on stderr.
    let (stderr, stdout) = (stderr.trim(), stdout.trim());
    match (stderr.is_empty(), stdout.is_empty()) {
        (true, true) => crate::i18n::message(
            "backend-git-failed-status-db85b2",
            &[
                ("args", (args.join(" ")).to_string().into()),
                ("status", (status).to_string().into()),
            ],
        ),
        (false, true) => stderr.to_owned(),
        (true, false) => stdout.to_owned(),
        (false, false) => format!("{stderr}\n{stdout}"),
    }
}

pub(super) fn run_inner(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    quiet: bool,
) -> Result<String, String> {
    let extra = extra_for(dir, args, creds, quiet)?;
    finish(execute(dir, &extra, args)?, args, ok)
}

/// [`run`] with `GIT_INDEX_FILE` set to `index`, a path on the machine git runs on: for work
/// on a copy of the index (or a new one) that leaves the repository's own index alone.
pub fn run_with_index(dir: &Path, index: &str, args: &[&str]) -> Result<String, String> {
    let extra = Extra {
        env: vec![("GIT_INDEX_FILE", index.to_owned())],
        ..Extra::default()
    };
    let out = execute(Some(dir), &extra, args)?;
    outcome(out, args, &[0])
}

/// Stdout when git exited with one of the codes `ok`, else what it said.
pub(super) fn outcome(out: Ran, args: &[&str], ok: &[i32]) -> Result<String, String> {
    finish(out, args, ok)
}

/// [`run`] with variables for git on top of the usual ones, here or on the machine git runs on
/// for `dir`: a scratch index (`GIT_INDEX_FILE`), the author of a commit being made.
pub fn run_env_owned(
    dir: &Path,
    env: &[(&'static str, String)],
    args: &[&str],
) -> Result<String, String> {
    let extra = Extra {
        pre: Vec::new(),
        env: env.to_vec(),
        ..Extra::default()
    };
    finish(execute(Some(dir), &extra, args)?, args, &[0])
}

/// What a call that ended as `out` gives: its output when its exit code is one of `ok`, else
/// what it said went wrong.
pub(super) fn finish(out: Ran, args: &[&str], ok: &[i32]) -> Result<String, String> {
    // For the doctor: what hooks print is lost once the command succeeds.
    let succeeded = out.code.is_some_and(|c| ok.contains(&c));
    if succeeded {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        // Merges and pulls report conflicts on stdout and the summary on stderr.
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let said = match (stderr.is_empty(), stdout.is_empty()) {
            (true, true) => String::new(),
            (false, true) => stderr,
            (true, false) => stdout,
            (false, false) => format!("{stderr}\n{stdout}"),
        };
        Err(download_read_error(
            args,
            match (&out.failed_hook, said.is_empty()) {
                (Some(hook), _) => hook.explain(&said, args),
                (None, true) => crate::i18n::message(
                    "backend-git-failed-da095f",
                    &[
                        ("args", (args.join(" ")).to_string().into()),
                        ("status", (out.status).to_string().into()),
                    ],
                ),
                (None, false) => said,
            },
        ))
    }
}

/// [`run_inner`], and what the call did, for the activity log.
pub(super) fn run_inner_recorded(
    dir: Option<&Path>,
    args: &[&str],
    ok: &[i32],
    creds: Option<&Credentials>,
    quiet: bool,
) -> (Result<String, String>, Recorded) {
    let started = std::time::Instant::now();
    let mut extra = match extra_for(dir, args, creds, quiet) {
        Ok(extra) => extra,
        Err(e) => {
            return (
                Err(e.clone()),
                Recorded::new(args, None, &e, creds, started),
            );
        }
    };
    extra.trace_hooks = !quiet && super::super::hooks::wants_trace(args);
    let out = match execute(dir, &extra, args) {
        Ok(out) => out,
        Err(e) => {
            return (
                Err(e.clone()),
                Recorded::new(args, None, &e, creds, started),
            );
        }
    };
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let recorded = Recorded::new(args, out.code, &both(&stdout, &stderr), creds, started);
    let result = finish(out, args, ok);
    (result, recorded)
}

/// What a command printed, its output then its error output.
pub(super) fn both(stdout: &str, stderr: &str) -> String {
    match (stdout.trim_end(), stderr.trim_end()) {
        ("", err) => err.to_owned(),
        (out, "") => out.to_owned(),
        (out, err) => format!("{out}\n{err}"),
    }
}

/// What one git call did, for the activity log: its command line, how it ended, what it
/// printed and how long it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    /// The arguments after `git`, as the caller gave them (credentials never are: they go to
    /// git through the environment).
    pub args: Vec<String>,
    /// Its exit code; `None` when it couldn't run, or was stopped.
    pub code: Option<i32>,
    /// What it printed (output, then error output; progress lines as last drawn).
    pub output: String,
    pub took: std::time::Duration,
}

impl Recorded {
    /// What a call did, its secrets hidden (see [`super::redact`]): the password typed into
    /// the tab (`creds`) among them.
    pub(crate) fn new(
        args: &[&str],
        code: Option<i32>,
        output: &str,
        creds: Option<&Credentials>,
        started: std::time::Instant,
    ) -> Self {
        crate::diagnostics::crash::operation(args, code, started.elapsed());
        let password = creds.map(|c| c.password.as_str());
        Self {
            args: args
                .iter()
                .map(|a| super::super::redact(a, password))
                .collect(),
            code,
            output: super::super::redact(output, password),
            took: started.elapsed(),
        }
    }
}

/// [`run_with`] (or [`run_unattended`] when `quiet`), and what the call did.
pub fn run_recorded(
    dir: Option<&Path>,
    args: &[&str],
    creds: Option<&Credentials>,
    quiet: bool,
) -> (Result<String, String>, Recorded) {
    run_inner_recorded(dir, args, &[0], creds, quiet)
}
