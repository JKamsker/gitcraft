//! Bounded object reads with cancellation and explicit download policies.

use super::*;

/// Bounded, non-fetching preparation for opt-in local syntax comparison.
pub(crate) fn syntax_read(
    repo: &Path,
    args: &[&str],
    limit: usize,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
    if crate::remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-syntax-comparison-is-currently-available-for-local-repositories-s-c279dd",
        )
        .to_owned());
    }
    let args: Vec<_> = std::iter::once("--no-lazy-fetch")
        .chain(args.iter().copied())
        .collect();
    bounded::local(repo, &args, Vec::new(), limit, timeout).map_err(syntax_read_error)
}

pub(super) fn syntax_read_error(error: String) -> String {
    if error
        .lines()
        .any(|line| line.trim() == "unknown option: --no-lazy-fetch")
    {
        crate::i18n::message(
            "backend-syntax-preparation-requires-git-with-no-lazy-fetch-support-error-0c471c",
            &[("detail", (error).to_string().into())],
        )
    } else {
        error
    }
}

/// Bounded raw snapshots: no implicit network fetches, filters or detached workers.
pub(in crate::git) fn comparison_output(
    repo: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    active: &dyn Fn() -> bool,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Comparison cancelled".into());
    }
    let guarded: Vec<_> = std::iter::once("--no-lazy-fetch")
        .chain(args.iter().copied())
        .collect();
    let out = if let Some(target) = remote::parse(repo) {
        let mut request = bounded::request(&target, &guarded, Vec::new(), 8 * 1024 * 1024);
        request
            .env
            .extend(env.iter().map(|(k, v)| ((*k).into(), v.clone())));
        comparison_remote(&target.host, request, active)?
    } else {
        let extra = Extra {
            env: env.to_vec(),
            ..Default::default()
        };
        let cmd = local_command(Some(repo), &extra, &guarded);
        let out = crate::processes::run_while(
            cmd,
            crate::processes::Options {
                timeout: Some(std::time::Duration::from_secs(20)),
                output_limit: 8 * 1024 * 1024,
                stderr_limit: Some(64 * 1024),
                ..Default::default()
            },
            active,
        )?
        .complete()?;
        remote::Output {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        }
    };
    if out.stdout.len() > 8 * 1024 * 1024 || out.stderr.len() > 64 * 1024 {
        return Err(crate::i18n::text(
            "backend-comparison-output-exceeds-the-size-limit-choose-fewer-files-727a73",
        )
        .into());
    }
    if !active() {
        return Err("Comparison cancelled".into());
    }
    Ok(out)
}

pub(in crate::git) fn comparison_remote(
    host: &remote::Host,
    mut request: remote::Request,
    active: &dyn Fn() -> bool,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Comparison cancelled".into());
    }
    request.timeout = Some(std::time::Duration::from_secs(20));
    request.long = true;
    let out = remote::exec(host, &request)?;
    if !active() {
        return Err("Comparison cancelled".into());
    }
    Ok(out)
}

/// Bounded isolated object reads/replays on the shared process service. Task cancellation
/// owns both local children and remote completion; no detached transport is introduced.
pub(crate) fn forecast_bytes(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    draft_bytes(dir, args, env, Vec::new(), ok, active)
}

pub(crate) fn draft_bytes(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    input: Vec<u8>,
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if !active() || crate::tasks::cancellation().is_cancelled() {
        return Err("Read cancelled".into());
    }
    let extra = Extra {
        env: [("GIT_NO_REPLACE_OBJECTS", "1".into())]
            .into_iter()
            .chain(env.iter().cloned())
            .collect(),
        stdin: Some(input),
        long: true,
        timeout: Some(std::time::Duration::from_secs(15)),
        output_limit: Some(64 << 20),
        ..Default::default()
    };
    const REMOVE: &[&str] = &[
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_ATTR_SOURCE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ];
    let (code, stdout, stderr) = if let Some(target) = remote::parse(dir) {
        let mut request = remote_request(&target, &extra, args);
        request.unset.extend(REMOVE.iter().map(|s| (*s).to_owned()));
        let out = remote::exec(&target.host, &request)?;
        (out.code, out.stdout, out.stderr)
    } else {
        let mut command = local_command(Some(dir), &extra, args);
        for key in REMOVE {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        let out = crate::processes::run(
            command,
            crate::processes::Options {
                input: extra.stdin,
                background: true,
                timeout: extra.timeout,
                output_limit: 64 << 20,
                stderr_limit: Some(64 << 10),
                ..Default::default()
            },
        )?
        .complete()?;
        (out.status.code().unwrap_or(-1), out.stdout, out.stderr)
    };
    if !active() {
        return Err("Read cancelled".into());
    }
    if stdout.len() > 64 << 20 || stderr.len() > 64 << 10 {
        return Err(crate::i18n::text("backend-read-output-exceeds-limit-042a31").into());
    }
    if ok.contains(&code) && stderr.is_empty() {
        Ok(stdout)
    } else {
        Err(crate::i18n::message(
            "backend-git-command-failed-code-a5e2b6",
            &[
                ("code", (code).to_string().into()),
                (
                    "from_utf8_lossy",
                    (String::from_utf8_lossy(&stderr).trim()).to_string().into(),
                ),
            ],
        ))
    }
}

pub(crate) fn forecast_run(
    dir: &Path,
    args: &[&str],
    env: &[(&'static str, String)],
    ok: &[i32],
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    forecast_bytes(dir, args, env, ok, active).map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub(crate) fn forecast_remote(
    host: &remote::Host,
    mut request: remote::Request,
    active: &dyn Fn() -> bool,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Read cancelled".into());
    }
    request.timeout = Some(std::time::Duration::from_secs(15));
    request.long = true;
    remote::exec(host, &request)
}

/// Raw object recovery transport with bounded output and current task ownership. It never
/// abandons a remote call while that call still owns a session or remote process.
pub(in crate::git) fn recovery_output(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    active: &dyn Fn() -> bool,
    timeout: std::time::Duration,
) -> Result<remote::Output, String> {
    if !active() {
        return Err("Recovery cancelled".into());
    }
    const MAX: usize = 16 << 20;
    const REMOVE: &[&str] = &[
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_ATTR_SOURCE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_SHALLOW_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_SUPER_PREFIX",
        "GIT_LITERAL_PATHSPECS",
        "GIT_GLOB_PATHSPECS",
        "GIT_NOGLOB_PATHSPECS",
        "GIT_ICASE_PATHSPECS",
    ];
    let extra = Extra {
        stdin: input.map(<[u8]>::to_vec),
        timeout: Some(timeout),
        long: true,
        ..Default::default()
    };
    let out = if let Some(target) = remote::parse(dir) {
        let mut request = remote_request(&target, &extra, args);
        request.unset.extend(REMOVE.iter().map(|s| (*s).into()));
        let command = request
            .script
            .strip_prefix("exec ")
            .unwrap_or(&request.script)
            .to_owned();
        request.script = format!(
            "umask 077\nrecovery_io=$(mktemp -d) || exit 1\ntrap 'rm -rf -- \"$recovery_io\"' 0\n\
             mkfifo \"$recovery_io/out\" \"$recovery_io/err\" || exit 1\n\
             head -c {cap} <\"$recovery_io/out\" >\"$recovery_io/out-cap\" & recovery_stdout=$!\n\
             head -c {cap} <\"$recovery_io/err\" >\"$recovery_io/err-cap\" & recovery_stderr=$!\n\
             {command} >\"$recovery_io/out\" 2>\"$recovery_io/err\"; recovery_code=$?\n\
             wait \"$recovery_stdout\"; wait \"$recovery_stderr\"\n\
             cat \"$recovery_io/out-cap\"; cat \"$recovery_io/err-cap\" >&2\nexit \"$recovery_code\"",
            cap = MAX + 1
        );
        remote::exec(&target.host, &request)?
    } else {
        let mut command = local_command(Some(dir), &extra, args);
        for key in REMOVE {
            command.env_remove(key);
        }
        let out = crate::processes::run(
            command,
            crate::processes::Options {
                input: extra.stdin,
                background: true,
                timeout: Some(timeout),
                output_limit: MAX,
                stderr_limit: Some(MAX),
                ..Default::default()
            },
        )?
        .complete()?;
        remote::Output {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        }
    };
    if !active() {
        return Err("Recovery cancelled".into());
    }
    if out.stdout.len() > MAX || out.stderr.len() > MAX {
        return Err(crate::i18n::text("backend-recovery-output-exceeds-limit-3e73fc").into());
    }
    Ok(out)
}

pub(super) fn download_read_error(args: &[&str], error: String) -> String {
    if super::super::clone_options::passive_object_read(args)
        && [
            "missing blob",
            "missing object",
            "bad object",
            "unable to read",
            "could not read",
            "promisor",
        ]
        .iter()
        .any(|text| error.to_lowercase().contains(text))
    {
        crate::i18n::message(
            "backend-error-automatic-object-downloads-are-disabled-in-a-partial-clone-5cfc0a",
            &[("detail", (error).to_string().into())],
        )
    } else {
        error
    }
}

/// Shared by streaming, watched, bounded and ordinary local commands.
pub(super) fn object_read_policy(cmd: &mut Command, args: &[&str]) {
    cmd.env(
        "GIT_NO_LAZY_FETCH",
        super::super::clone_options::download_environment(args),
    );
    if super::super::clone_options::passive_object_read(args) {
        cmd.arg("--no-lazy-fetch").env("GIT_NO_LAZY_FETCH", "1");
    }
}
