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
            crate::i18n::message(
                "backend-git-failed-7124da",
                &[("status", (out.status).to_string().into())],
            )
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
        return Err(
            crate::i18n::text("backend-bundle-output-exceeds-the-size-limit-caef83").to_owned(),
        );
    }
    if out.code != 0 {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            crate::i18n::message(
                "backend-git-failed-with-exit-code-2cecca",
                &[("code", (out.code).to_string().into())],
            )
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
    request_with_extra(target, args, input, limit, &super::Extra::default())
}

fn request_with_extra(
    target: &remote::Target,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
    extra: &super::Extra,
) -> remote::Request {
    let mut req = super::remote_request(target, extra, args);
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

/// Preserve expected nonzero plumbing statuses and their bounded stdout. The
/// ordinary success-only stream API retains its existing behavior.
pub(in crate::git) fn request_captured(
    target: &remote::Target,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
) -> remote::Request {
    let mut req = request_with_extra(target, args, input, limit, &super::Extra::repository());
    req.script = req
        .script
        .replace("[ \"$code\" = 0 ] || exit \"$code\"\n", "");
    req.script.push_str("\nexit \"$code\"");
    req
}

#[cfg(test)]
mod captured_recording {
    use super::*;
    #[test]
    fn expected_nonzero_reads_keep_output_and_selected_repository_environment() {
        let target = remote::Target {
            host: remote::Host::Ssh("uncontacted.invalid".into()),
            path: "/selected".into(),
        };
        let req = request_captured(
            &target,
            &["config", "--bool", "--get", "remote.origin.mirror"],
            Vec::new(),
            65536,
        );
        assert!(!req.script.contains("[ \"$code\" = 0 ] || exit"));
        assert!(req.script.ends_with("exit \"$code\""));
        assert!(req.script.contains("head -c 65536"));
        for key in super::super::REPOSITORY_ENV_REMOVED {
            assert!(req.unset.iter().any(|k| k == key), "{key}");
        }
        let args = ["rev-parse", "--verify", "HEAD"];
        let mut inherited = std::process::Command::new("/recorded/git");
        inherited
            .env("GIT_DIR", "/decoy")
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "remote.origin.mirror");
        let command = super::super::apply_command_extras(
            inherited,
            None,
            &super::super::Extra::repository(),
            &args,
        );
        let env: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        for key in ["GIT_DIR", "GIT_CONFIG_COUNT"] {
            assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None));
        }
    }
}
