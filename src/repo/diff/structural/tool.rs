//! Bounded, noninteractive tool execution on the repository's machine.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[cfg(test)]
use base64::Engine;

use crate::remote;

const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_ERROR: usize = 32 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);
const ARGS: &[&str] = &[
    "--display",
    "json",
    "--color",
    "never",
    "--parse-error-limit",
    "0",
    "--byte-limit",
    "2097152",
    "--graph-limit",
    "1000000",
    "--strip-cr",
    "off",
];

/// User display/ignore overrides mustn't silently omit comments, force text, or skip output.
fn removed_env() -> Vec<String> {
    let mut names: Vec<String> = [
        "DFT_CHECK_ONLY",
        "DFT_IGNORE_COMMENTS",
        "DFT_SKIP_UNCHANGED",
        "DFT_EXIT_CODE",
        "DFT_OVERRIDE",
        "DFT_OVERRIDE_BINARY",
        "DFT_IGNORE",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for base in ["DFT_OVERRIDE", "DFT_OVERRIDE_BINARY"] {
        for i in 1..=9 {
            names.push(format!("{base}_{i}"));
        }
    }
    names
}

pub(super) fn compare(repo: &Path, path: &str, old: &str, new: &str) -> Result<Vec<u8>, String> {
    if remote::is_remote(repo) {
        return Err(crate::i18n::text("repo-core-2337").to_owned());
    }
    let (temp, before, after) = prepare(old, new)?;
    let mut command = Command::new("difft");
    crate::shell_path::apply(&mut command);
    command
        .current_dir(&temp.0)
        .args(ARGS)
        .arg("--")
        .args(protocol_args(path, &before, &after))
        .env("DFT_UNSTABLE", "yes");
    for name in removed_env() {
        command.env_remove(name);
    }
    let out = run(command, TIMEOUT)?;
    output(out.code, out.stdout, out.stderr)
}

/// Difftastic's documented GIT_EXTERNAL_DIFF protocol separates the display/language
/// hint from the files it opens. Git paths must only appear in the display argument:
/// even legal tree names can be absolute paths or Windows drive/device names.
pub(super) fn protocol_args(path: &str, before: &Path, after: &Path) -> [OsString; 7] {
    [
        path.into(),
        before.into(),
        "0000000".into(),
        "100644".into(),
        after.into(),
        "0000000".into(),
        "100644".into(),
    ]
}

/// Only application-owned constants reach the temporary-file write boundary.
pub(super) fn prepare(old: &str, new: &str) -> Result<(Temp, PathBuf, PathBuf), String> {
    let temp = Temp::new()?;
    let (before, after) = (
        temp.0.join("before").join("source"),
        temp.0.join("after").join("source"),
    );
    for (file, text) in [(&before, old), (&after, new)] {
        std::fs::write(file, text).map_err(|e| {
            crate::i18n::message("repo-core-4069", &[("e", format!("{}", e).into())])
        })?;
    }
    Ok((temp, before, after))
}

fn output(code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> Result<Vec<u8>, String> {
    if code == 127 {
        return Err(crate::i18n::text("repo-core-4070").to_owned());
    }
    if stdout.len() > MAX_OUTPUT || stderr.len() > MAX_ERROR {
        return Err(crate::i18n::text("repo-core-4071").to_owned());
    }
    if code != 0 {
        let error = String::from_utf8_lossy(&stderr);
        return Err(crate::i18n::message(
            "repo-core-4072",
            &[("trim", error.trim().to_string().into())],
        ));
    }
    Ok(stdout)
}

#[cfg(test)]
pub(super) fn request(
    target: &remote::Target,
    path: &str,
    old: &str,
    new: &str,
) -> remote::Request {
    let display = remote::sh_quote(path);
    let args = ARGS
        .iter()
        .map(|a| remote::sh_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    // A private directory on that machine, removed whether difft succeeds or fails. The
    // source is transported as data; the quoted Git path is display metadata only.
    // The seven-argument protocol opens only the fixed application-owned source paths.
    let script = format!(
        "umask 077\n\
         d=$(mktemp -d) || exit 1\n\
         trap 'rm -rf \"$d\"' 0\n\
         mkdir \"$d/before\" \"$d/after\" || exit 1\n\
         IFS= read -r before && IFS= read -r after || exit 1\n\
         printf '%s' \"$before\" | base64 -d > \"$d/before/source\" || exit 1\n\
         printf '%s' \"$after\" | base64 -d > \"$d/after/source\" || exit 1\n\
         cd \"$d\" || exit 1\n\
         (ulimit -f 32768; difft {args} -- {display} \"$d/before/source\" 0000000 100644 \"$d/after/source\" 0000000 100644) > \"$d/report\" 2> \"$d/error\"\n\
         code=$?\n\
         head -c {max_error} \"$d/error\" >&2\n\
         [ \"$code\" = 0 ] || exit \"$code\"\n\
         bytes=$(wc -c < \"$d/report\")\n\
         [ \"$bytes\" -le {max_output} ] || {{ echo 'syntax report too large' >&2; exit 1; }}\n\
         cat \"$d/report\"",
        max_error = MAX_ERROR,
        max_output = MAX_OUTPUT,
    );
    let encode = |text: &str| base64::engine::general_purpose::STANDARD.encode(text);
    remote::Request {
        cwd: Some(target.path.clone()),
        script,
        stdin: Some(format!("{}\n{}\n", encode(old), encode(new)).into_bytes()),
        env: vec![("DFT_UNSTABLE".to_owned(), "yes".to_owned())],
        unset: removed_env(),
        timeout: Some(TIMEOUT),
        long: true,
        ..Default::default()
    }
}

pub(super) struct Temp(PathBuf);
impl Temp {
    fn new() -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        for _ in 0..100 {
            let path = root.join(format!(
                "gitcraft-syntax-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => {
                    let temp = Self(path);
                    for side in ["before", "after"] {
                        std::fs::create_dir(temp.0.join(side)).map_err(|e| e.to_string())?;
                    }
                    return Ok(temp);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(crate::i18n::message(
                        "repo-core-4069",
                        &[("e", format!("{}", e).into())],
                    ));
                }
            }
        }
        Err(crate::i18n::text("repo-core-4074").to_owned())
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Use the shared subprocess supervisor and inherit the tab's cancellation token.
pub(super) fn run(command: Command, timeout: Duration) -> Result<remote::Output, String> {
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            background: true,
            timeout: Some(timeout),
            output_limit: MAX_OUTPUT,
            stderr_limit: Some(MAX_ERROR),
            ..Default::default()
        },
    )
    .map_err(|e| crate::i18n::message("repo-core-4075", &[("e", e.to_string().into())]))?
    .complete()?;
    Ok(remote::Output {
        code: out.status.code().unwrap_or(-1),
        stdout: out.stdout,
        stderr: out.stderr,
    })
}
