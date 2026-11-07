//! Decrypt through stdin/stdout only. Never return a raw tool diagnostic: it may echo data.
use super::model::{Format, MAX_BYTES};
use crate::remote;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub(super) const TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(test)]
thread_local! { pub(super) static TEST_PROGRAM: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) }; }

const FAILED: &str = "SOPS could not decrypt this side. Check the installed sops version and this machine's keys/permissions; the encrypted patch is still available. Tool diagnostics are hidden because they can contain secrets.";

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

pub(super) fn request(target: &remote::Target, format: Format, input: &str) -> remote::Request {
    remote::Request {
        cwd: Some(target.path.clone()),
        script: format!(
            "tool=$(command -v sops) || exit 127\ncase \"$tool\" in /*) ;; *) exit 127;; esac\nexec \"$tool\" {}",
            args(format)
                .iter()
                .map(|s| remote::sh_quote(s))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        stdin: Some(input.as_bytes().to_vec()),
        timeout: Some(TIMEOUT),
        long: true,
        ..Default::default()
    }
}

pub(super) fn decrypt(
    repo: &Path,
    format: Format,
    input: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    let bytes = if let Some(target) = remote::parse(repo) {
        let out = remote::private_output(
            &target.host,
            &request(&target, format, input),
            active,
            MAX_BYTES as u64,
        )
        .map_err(safe_failure)?;
        if out.code != 0 {
            return Err(if out.code == 127 {
                "Install SOPS with the decrypt stdin command (SOPS 3.10 or later) on the repository's machine.".into()
            } else {
                FAILED.into()
            });
        }
        out.stdout
    } else {
        let program = std::path::PathBuf::from("sops");
        #[cfg(test)]
        let program = TEST_PROGRAM.with(|p| p.borrow().clone()).unwrap_or(program);
        let mut command = Command::new(program);
        crate::shell_path::apply(&mut command);
        command.current_dir(repo).args(args(format));
        let out = crate::bounded_process::run_input(
            command,
            Some(input.as_bytes().to_vec()),
            active,
            TIMEOUT,
            MAX_BYTES as u64,
            32768,
        )
        .map_err(safe_failure)?;
        if !out.status.success() {
            return Err(FAILED.into());
        }
        out.stdout
    };
    if !active() {
        return Err("Decryption cancelled.".into());
    }
    plaintext(bytes)
}

fn safe_failure(error: String) -> String {
    if error.contains("cancelled") {
        "Decryption cancelled.".into()
    } else if error.contains("timed out") {
        "Decryption exceeded 20 seconds and was stopped. The encrypted patch is still available."
            .into()
    } else if error.contains("exceeded") {
        "SOPS output exceeded its size limit (2 MiB stdout, 32 KiB diagnostics). The encrypted patch is still available.".into()
    } else {
        "Could not start SOPS or reach the repository's machine. Install SOPS 3.10 or later there and check the connection. The encrypted patch is still available.".into()
    }
}

pub(super) fn plaintext(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.len() > MAX_BYTES || bytes.contains(&0) {
        return Err("Decrypted comparison supports UTF-8 text within 2 MiB per side; binary output is not displayed.".into());
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| "Decrypted comparison supports UTF-8 text only.".to_owned())?;
    if text.lines().count() > super::model::MAX_LINES
        || text.lines().any(|line| line.chars().count() > 8192)
    {
        return Err("Decrypted text exceeded 20,000 lines or 8,192 characters per line. Use the encrypted patch.".into());
    }
    Ok(text)
}
