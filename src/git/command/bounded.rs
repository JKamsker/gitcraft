//! Binary Git streams with input/output limits and a deadline, also on remote machines.
use std::path::Path;
use std::time::Duration;

use crate::remote;

const ERROR_LIMIT: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);

pub(in crate::git) fn run(
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if let Some(target) = remote::parse(repo) {
        return there(&target, args, input, limit);
    }
    local(repo, args, input, limit, TIMEOUT)
}

pub(in crate::git) fn local(
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let command = super::local_command(Some(repo), &super::Extra::default(), args);
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            input: Some(input),
            background: true,
            timeout: Some(timeout),
            output_limit: limit,
            stderr_limit: Some(ERROR_LIMIT),
            ..Default::default()
        },
    )?
    .complete()?;
    if !out.status.success() {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            format!("Git failed: {}", out.status)
        } else {
            message
        });
    }
    Ok(out.stdout)
}

fn there(
    target: &remote::Target,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let request = request(target, args, input, limit);
    let out = remote::exec(&target.host, &request)?;
    if out.stdout.len() > limit || out.stderr.len() > ERROR_LIMIT {
        return Err("Bundle output exceeds the size limit".to_owned());
    }
    if out.code != 0 {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            format!("Git failed with exit code {}", out.code)
        } else {
            message
        });
    }
    Ok(out.stdout)
}

pub(in crate::git) fn request(
    target: &remote::Target,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
) -> remote::Request {
    let mut req = super::remote_request(target, &super::Extra::default(), args);
    let git = req.script;
    // Git stdout is binary. Bound disk use before passing it through the remote protocol.
    // The inner subshell keeps `exec git` from replacing the cleanup shell.
    req.script = format!(
        "umask 077\nd=$(mktemp -d) || exit 1\ntrap 'rm -rf \"$d\"' 0\ntrap 'exit 1' HUP INT TERM PIPE\n\
         (ulimit -f {blocks} || exit 1; {git}) >\"$d/out\" 2>\"$d/err\"\n\
         code=$?\nhead -c {errors} \"$d/err\" >&2\n[ \"$code\" = 0 ] || exit \"$code\"\n\
         [ \"$(wc -c <\"$d/out\")\" -le {limit} ] || {{ echo 'Bundle output exceeds the size limit' >&2; exit 1; }}\n\
         cat \"$d/out\"",
        // Unbundle writes the incoming pack into the object store, so the file limit
        // must cover the bundle size even when only a short ref list is returned.
        blocks = (limit.max(128 * 1024 * 1024) / 512) + 1,
        errors = ERROR_LIMIT,
    );
    req.stdin = Some(input);
    req.timeout = Some(TIMEOUT);
    req.long = true;
    req
}
