//! Binary, captured, and streamed output entry points with their existing limits.

use super::*;

/// How a git call ended, whatever its exit code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    /// `None` when it was ended by a signal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Stdout as git wrote it (`stdout` is it decoded, invalid UTF-8 replaced).
    pub raw: Vec<u8>,
}

/// Runs git in `dir` and returns how it ended, its exit code whatever it is, with `input` on
/// its standard input when given (`hash-object --stdin`…). Blocking: from the background.
pub fn run_captured(dir: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Captured, String> {
    let out = match (remote::parse(dir), input) {
        (Some(target), _) => {
            let _timer = super::super::backend::CliTimer::new(args);
            let mut request = remote_request(&target, &Extra::default(), args);
            request.stdin = input.map(<[u8]>::to_vec);
            let out = remote::exec(&target.host, &request)?;
            if out.code == 1 && remote::is_missing_folder(&out.stderr) {
                return Err(missing_folder(&target));
            }
            Ran {
                code: Some(out.code),
                status: crate::i18n::message(
                    "backend-exit-status-c5b63a",
                    &[("code", (out.code).to_string().into())],
                ),
                stdout: out.stdout,
                stderr: out.stderr,
                failed_hook: None,
            }
        }
        (None, None) => execute(Some(dir), &Extra::default(), args)?,
        (None, Some(input)) => run_here_with_input(dir, args, input)?,
    };
    Ok(Captured {
        code: out.code,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        raw: out.stdout,
    })
}

/// Runs git here in `dir` with `input` on its standard input.
pub(super) fn run_here_with_input(dir: &Path, args: &[&str], input: &[u8]) -> Result<Ran, String> {
    execute(
        Some(dir),
        &Extra {
            stdin: Some(input.to_vec()),
            ..Extra::default()
        },
        args,
    )
}

/// [`run`] with variables of its own on top of what every call gets (`GIT_INDEX_FILE` for a
/// temporary index), on whichever machine the repository is.
pub fn run_env(
    dir: Option<&Path>,
    args: &[&str],
    env: &[(&'static str, &str)],
) -> Result<String, String> {
    let extra = Extra {
        pre: Vec::new(),
        env: env.iter().map(|(n, v)| (*n, (*v).to_owned())).collect(),
        ..Extra::default()
    };
    let out = execute(dir, &extra, args)?;
    if out.code == Some(0) {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(if stderr.is_empty() {
            crate::i18n::message(
                "backend-git-failed-da095f",
                &[
                    ("args", (args.join(" ")).to_string().into()),
                    ("status", (out.status).to_string().into()),
                ],
            )
        } else {
            stderr
        })
    }
}

/// [`run_env`], taking the exit codes `ok` as success.
pub fn run_env_codes(
    dir: Option<&Path>,
    args: &[&str],
    env: &[(&'static str, &str)],
    ok: &[i32],
) -> Result<String, String> {
    let extra = Extra {
        pre: Vec::new(),
        env: env.iter().map(|(n, v)| (*n, (*v).to_owned())).collect(),
        ..Extra::default()
    };
    finish(execute(dir, &extra, args)?, args, ok)
}

/// Runs git and returns stdout as raw bytes (file contents: images…).
pub fn run_bytes(dir: Option<&Path>, args: &[&str]) -> Result<Vec<u8>, String> {
    // Blob callers already return a Vec, but need more headroom than command diagnostics.
    // Keep a finite per-read bound rather than allowing arbitrary process output in memory.
    let out = execute(
        dir,
        &Extra {
            output_limit: Some(512 << 20),
            ..Default::default()
        },
        args,
    )?;
    if out.code == Some(0) {
        Ok(out.stdout)
    } else {
        Err(download_read_error(
            args,
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    }
}

/// Bounded raw-object plumbing for source preparation before containment. This never
/// checks out files, invokes filters/hooks, or permits lazy object fetches/transports.
pub fn run_inert_objects(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    output_limit: usize,
    ok: &[i32],
) -> Result<Vec<u8>, String> {
    let extra = inert_object_options(args, input, output_limit)?;
    let out = execute(Some(dir), &extra, args)?;
    if out.code.is_some_and(|code| ok.contains(&code)) {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

pub(super) fn inert_object_options(
    args: &[&str],
    input: Option<&[u8]>,
    output_limit: usize,
) -> Result<Extra, String> {
    if !matches!(
        args.first().copied(),
        Some("ls-tree" | "cat-file" | "rev-parse" | "hash-object" | "update-ref")
    ) {
        return Err(crate::i18n::text(
            "backend-sandbox-source-preparation-permits-only-raw-object-plumbing-c05abe",
        )
        .into());
    }
    Ok(Extra {
        pre: vec![
            "--no-pager",
            "--no-replace-objects",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.allow=never",
        ],
        env: vec![("GIT_NO_LAZY_FETCH", "1".into())],
        timeout: Some(std::time::Duration::from_secs(15)),
        stdin: input.map(<[u8]>::to_vec),
        output_limit: Some(output_limit),
        ..Default::default()
    })
}

/// Read-only pack generation for a bisect transfer. Standalone remote supervision keeps
/// the requested bounded binary capture independent of the pooled shell's 64 MiB frames.
/// Nonzero Git exits stay distinct from uncertain transport/cancellation errors.
pub(crate) fn transfer_pack(
    dir: &Path,
    revisions: Vec<u8>,
    thin: bool,
    limit: usize,
) -> Result<Exit, String> {
    let extra = Extra {
        stdin: Some(revisions),
        env: vec![("GIT_NO_LAZY_FETCH", "1".into())],
        timeout: Some(std::time::Duration::from_secs(1800)),
        ..Default::default()
    };
    let mut args = vec![
        "pack-objects",
        "--stdout",
        "--revs",
        "--delta-base-offset",
        "--missing=error",
    ];
    if thin {
        args.push("--thin");
    }
    let mut options = crate::processes::Options {
        input: extra.stdin.clone(),
        background: true,
        timeout: extra.timeout,
        output_limit: limit,
        ..Default::default()
    };
    let command = if let Some(target) = remote::parse(dir) {
        let request = remote_request(&target, &extra, &args);
        remote::streaming_command_bounded(&target.host, &request, None, None, false, limit)?
            .configure(&mut options)
    } else {
        local_command(Some(dir), &extra, &args)
    };
    let out = crate::processes::run(command, options)?.complete()?;
    Ok(Exit {
        code: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// Save a command's stdout without capturing a local export in memory. Remote exports
/// retain the existing persistent-shell transport and its binary-output behavior.
pub fn run_to_file(dir: &Path, args: &[&str], file: &Path) -> Result<(), String> {
    if remote::is_remote(dir) {
        return run_bytes(Some(dir), args).and_then(|bytes| {
            std::fs::write(file, bytes).map_err(|e| {
                crate::i18n::message(
                    "backend-could-not-save-it-e-24f824",
                    &[("detail", (e).to_string().into())],
                )
            })
        });
    }
    let out = execute(
        Some(dir),
        &Extra {
            stdout_file: Some(file.to_owned()),
            ..Default::default()
        },
        args,
    )?;
    if out.code == Some(0) {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Runs git and returns its exit code with stdout as raw bytes, when the code is one of `ok`
/// (`merge-tree` answers on stdout and exits 1 on conflicts); else the error output.
pub fn run_code(dir: Option<&Path>, args: &[&str], ok: &[i32]) -> Result<(i32, Vec<u8>), String> {
    let out = execute(dir, &Extra::default(), args)?;
    match out.code {
        Some(code) if ok.contains(&code) => Ok((code, out.stdout)),
        _ => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(if stderr.is_empty() {
                crate::i18n::message(
                    "backend-git-failed-da095f",
                    &[
                        ("args", (args.join(" ")).to_string().into()),
                        ("status", (out.status).to_string().into()),
                    ],
                )
            } else {
                stderr
            })
        }
    }
}
