//! Decrypt through stdin/stdout only. Never return a raw tool diagnostic: it may echo data.
use super::model::{Format, MAX_BYTES};
use std::process::Command;
use std::time::Duration;

pub(super) const TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(test)]
thread_local! { pub(super) static TEST_PROGRAM: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) }; }

const FAILED: &str = "repo-core-3923";

pub(super) fn args(format: Format) -> [&'static str; 7] {
    [
        "decrypt",
        "--input-type",
        format.name(),
        "--output-type",
        format.name(),
        "--filename-override",
        format.hint(),
    ]
}

/// Only local process construction is possible here. Repository locations never reach
/// this boundary: the caller fetches ciphertext separately using reviewed Git transport.
pub(super) fn command(format: Format) -> Command {
    let program = std::path::PathBuf::from("sops");
    #[cfg(test)]
    let program = TEST_PROGRAM.with(|p| p.borrow().clone()).unwrap_or(program);
    let mut command = Command::new(program);
    crate::shell_path::apply(&mut command);
    // Avoid any SSH/WSL URI or repository-owned working directory at this boundary.
    command.current_dir(std::env::temp_dir()).args(args(format));
    command
}

pub(super) fn decrypt(
    format: Format,
    input: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    let out = crate::processes::run_while(
        command(format),
        crate::processes::Options {
            input: Some(input.as_bytes().to_vec()),
            background: true,
            timeout: Some(TIMEOUT),
            output_limit: MAX_BYTES,
            stderr_limit: Some(32768),
            ..Default::default()
        },
        active,
    )
    .and_then(|out| out.complete())
    .map_err(safe_failure)?;
    if !out.status.success() {
        return Err(crate::i18n::text(FAILED).into());
    }
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    plaintext(out.stdout)
}

fn safe_failure(error: String) -> String {
    if error.contains(crate::i18n::text("repo-core-2896")) {
        crate::i18n::text("repo-core-3914").into()
    } else if error.contains("timed out") {
        crate::i18n::text("repo-core-3929").into()
    } else if error.contains("exceeded") {
        crate::i18n::text("repo-core-3931").into()
    } else {
        crate::i18n::text("repo-core-3932").into()
    }
}

pub(super) fn plaintext(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.len() > MAX_BYTES || bytes.contains(&0) {
        return Err(crate::i18n::text("repo-core-3933").into());
    }
    let text =
        String::from_utf8(bytes).map_err(|_| crate::i18n::text("repo-core-3934").to_owned())?;
    if text.lines().count() > super::model::MAX_LINES
        || text.lines().any(|line| line.chars().count() > 8192)
    {
        return Err(crate::i18n::text("repo-core-3935").into());
    }
    Ok(text)
}
